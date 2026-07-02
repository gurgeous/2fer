//! Main app struct, this does all the work.

use std::{
  io::{self, IsTerminal, Read, Write},
  path::Path,
  time::Instant,
};

use crate::{
  args::Args,
  error::{Error, Result},
  formats,
  table::Table,
  util::{self, ReplayReader},
};

#[derive(Debug)]
pub struct App {
  // Parsed command-line arguments.
  pub args: Args,

  // Input format, set during `read`.
  pub input_format: Option<&'static dyn formats::Format>,

  // Output format, from symlink, --output, --as, etc.
  pub output_format: &'static dyn formats::Format,
}

//
// App
//

impl App {
  //
  // Build app state and validate output-format signals.
  //

  pub fn build(args: Args, symlink: Option<String>) -> Result<Self> {
    let output_format = Self::build_output_format(&args, symlink.as_deref())?;
    Ok(Self { args, input_format: None, output_format })
  }

  //
  // main
  //

  pub fn main(mut self) -> Result<()> {
    // read
    util::log_2fer("starting read");
    let tm = Instant::now();
    let table = self.read()?;
    let shape = format_args!("({}x{})", table.rows.len(), table.headers.len());

    validate_input_table(&table)?;
    self.validate_sqlite()?;
    util::log_2fer(format_args!("read {} ({shape})", util::format_elapsed(tm.elapsed()),));

    // write
    util::log_2fer("starting write");
    let tm = Instant::now();
    self.write(&table)?;
    util::log_2fer(format_args!("write {} ({shape})", util::format_elapsed(tm.elapsed()),));

    // success
    Ok(())
  }

  //
  // read
  //

  fn read(&mut self) -> Result<Table> {
    // stdin
    let input = self.args.input.as_ref().map(|path| path.to_string_lossy().into_owned()).unwrap_or_default();
    if input.is_empty() && io::stdin().is_terminal() {
      return Err(Error::StdinRead);
    }
    if input.is_empty() || input == "-" {
      util::log_2fer("  input stdin");
      return self.read_from_stdin();
    }

    // path
    let path = self.args.input.clone().expect("file input checked above");
    util::log_2fer(format_args!("  input file path={}", path.display()));
    let format = formats::detect_by_path(&path)?;
    self.read_from_path_with_format(&path, format)
  }

  fn read_from_stdin(&mut self) -> Result<Table> {
    let stdin = io::stdin();
    let mut reader = ReplayReader::new(stdin.lock());

    // 1. sniff first 4k, what do we see?
    let format = {
      let prefix = reader.peek(util::SNIFF_BYTES).map_err(|_| Error::StdinRead)?;
      formats::detect_by_sample(prefix)
    };
    if let Some(format) = format {
      util::log_2fer(format_args!("  sniff stdin format={}", format.name()));
      self.input_format = Some(format);
      return format.read_from_reader(self, &mut reader);
    }

    // 2. fallback - spill once so path-backed formats get one last chance.
    util::log_2fer("  sniff stdin format=unknown");
    let file = self.spill_stdin(&mut reader)?;
    let path = file.path();
    util::log_2fer(format_args!("  stdin spilled path={}", path.display()));
    formats::detect_by_path(path).and_then(|f| self.read_from_path_with_format(path, f))
  }

  fn read_from_path_with_format(&mut self, path: &Path, format: &'static dyn formats::Format) -> Result<Table> {
    self.input_format = Some(format);
    format.read_from_path(self, path)
  }

  fn spill_stdin<R: Read>(&self, reader: &mut ReplayReader<R>) -> Result<tempfile::NamedTempFile> {
    let mut file = util::temp_file("stdin", "input")?;
    io::copy(reader, file.as_file_mut()).map_err(|error| Error::WriteFile { path: file.path().to_owned(), error })?;
    file.as_file_mut().flush().map_err(|error| Error::WriteFile { path: file.path().to_owned(), error })?;
    Ok(file)
  }

  /// Validate --table, which can apply to both input and output.
  pub fn validate_sqlite(&self) -> Result<()> {
    if self.args.table.is_none() {
      return Ok(());
    }
    let uses_sqlite = self.input_format.is_some_and(|f| f.name() == "sqlite") || self.output_format.name() == "sqlite";
    if uses_sqlite { Ok(()) } else { Err(Error::TableOptionRequiresSqlite) }
  }

  //
  // write
  //

  fn write(&self, table: &Table) -> Result<()> {
    // stdout
    if self.args.output.is_none() {
      let stdout = io::stdout();
      if self.output_format.binary_output() && stdout.is_terminal() {
        return Err(Error::BinaryStdout(self.output_format.name()));
      }
      util::log_2fer(format_args!("  output stdout format={}", self.output_format.name()));
      let mut stdout = stdout.lock();
      return self.output_format.write_to_writer(self, table, &mut stdout);
    }

    // path
    let path = self.args.output.as_deref().expect("file output checked above");
    util::log_2fer(format_args!("  output file path={} format={}", path.display(), self.output_format.name()));
    self.output_format.write_to_path(self, path, table)
  }

  //
  // Build output format once before reading and writing. There are three
  // signals and they all have to match if present:
  //
  // 1. symlink name
  // 2. --output blah.ext
  // 3. --as format
  //

  fn build_output_format(args: &Args, symlink: Option<&str>) -> Result<&'static dyn formats::Format> {
    // Collect every independent output-format signal before choosing one.
    let mut signals = Vec::new();

    // 1. symlink name, if any
    if let Some(symlink) = symlink
      && let Some(format) = format_from_command(symlink)
    {
      signals.push(("command name", format));
    }

    // 2. --output blah.ext, if any
    if let Some(path) = args.output.as_deref() {
      let ext = util::path_ext(path);
      if ext.is_empty() {
        // Extensionless output paths need --as or a symlink to choose a format.
      } else if let Some(format) = formats::find(ext) {
        signals.push(("--output", format));
      } else {
        return Err(Error::UnsupportedOutputExtension(ext.to_owned()));
      }
    }

    // 3. --as format, if any
    if let Some(format) = args.as_format.as_deref() {
      let Some(format) = formats::find(format) else {
        return Err(Error::UnsupportedOutputFormat(format.to_owned()));
      };
      signals.push(("--as", format));
    }

    // must be 1+ signal, and they all have to match
    let Some((_, first)) = signals.first() else {
      return Err(Error::OutputFormatRequired);
    };
    if signals.iter().any(|(_, format)| format.name() != first.name()) {
      return Err(Error::OutputFormatConflict(
        signals.into_iter().map(|(source, format)| format!("{source}={}", format.name())).collect(),
      ));
    }

    Ok(*first)
  }
}

//
// helpers
//

/// Return the default output format implied by a 2xxx command name.
pub(crate) fn format_from_command(symlink: &str) -> Option<&'static dyn formats::Format> {
  let name = Path::new(symlink).file_name().and_then(|name| name.to_str()).unwrap_or(symlink);
  let name = util::strip_prefix_ignore_ascii_case(name, "2").unwrap_or(name);
  let name = util::strip_suffix_ignore_ascii_case(name, ".exe").unwrap_or(name);
  formats::find(name)
}

fn validate_input_table(table: &Table) -> Result<()> {
  if table.headers.is_empty() || table.rows.is_empty() {
    return Err(Error::EmptyFile);
  }
  Ok(())
}

//
// tests
//

#[cfg(test)]
mod tests {
  use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
  };

  use super::*;

  #[test]
  fn test_build() {
    for (symlink, output, as_format, expected) in [
      (Some("2csv"), None, None, "csv"),
      (Some("2tsv"), None, None, "tsv"),
      (Some("2json"), None, None, "json"),
      (Some("2jsonl"), None, None, "jsonl"),
      (Some("2md"), None, None, "md"),
      (Some("2sqlite"), None, None, "sqlite"),
      (Some("2yml"), None, None, "yml"),
      (Some("2xlsx"), None, None, "xlsx"),
      (Some("2json.exe"), None, None, "json"),
      (Some("2fer"), Some("out.json"), None, "json"),
      (Some("2json"), Some("out.json"), Some("json"), "json"),
      (Some("2fer"), Some("out.ndjson"), None, "jsonl"),
      (Some("2fer"), Some("out.md"), None, "md"),
      (Some("2fer"), Some("out.sqlite"), None, "sqlite"),
    ] {
      let app = output_app(symlink, output, as_format).unwrap();
      assert_eq!(expected, app.output_format.name());
    }

    assert!(matches!(output_app(Some("2fer"), None, None), Err(Error::OutputFormatRequired)));
    assert!(matches!(output_app(Some("2csv"), Some("out.json"), None), Err(Error::OutputFormatConflict(_))));
    assert!(matches!(output_app(Some("2fer"), None, Some("nope")), Err(Error::UnsupportedOutputFormat(_))));
    assert!(matches!(
      output_app(Some("2fer"), Some("out.nope"), None),
      Err(Error::UnsupportedOutputExtension(ext)) if ext == "nope"
    ));
    assert!(matches!(
      output_app(Some("2fer"), Some("out.nope"), Some("json")),
      Err(Error::UnsupportedOutputExtension(ext)) if ext == "nope"
    ));
    assert!(matches!(output_app(Some("2xls"), None, None), Err(Error::OutputFormatRequired)));
    assert!(matches!(
      output_app(Some("2fer"), Some("out.xls"), None),
      Err(Error::UnsupportedOutputExtension(ext)) if ext == "xls"
    ));
    assert!(
      matches!(output_app(Some("2fer"), None, Some("xls")), Err(Error::UnsupportedOutputFormat(format)) if format == "xls")
    );
  }

  //
  // helpers
  //

  fn output_app(symlink: Option<&str>, output: Option<&str>, as_format: Option<&str>) -> Result<App> {
    let args = Args {
      output: output.map(PathBuf::from),
      as_format: as_format.map(str::to_owned),
      delim: None,
      table: None,
      vanilla: false,
      compact: false,
      completion: None,
      help: false,
      version: false,
      input: None,
      argv_had_args: false,
    };
    App::build(args, symlink.map(str::to_owned))
  }

  fn input_app(
    input: Option<PathBuf>,
    table: Option<&str>,
    output_format: &'static dyn formats::Format,
    input_format: Option<&'static dyn formats::Format>,
  ) -> App {
    let args = Args {
      output: None,
      as_format: Some(output_format.name().to_owned()),
      delim: None,
      table: table.map(str::to_owned),
      vanilla: false,
      compact: false,
      completion: None,
      help: false,
      version: false,
      input,
      argv_had_args: false,
    };
    let mut app = App::build(args, None).unwrap();
    if let Some(input_format) = input_format {
      app.input_format = Some(input_format);
    }
    app
  }

  fn dispatch_app(delim: Option<u8>, vanilla: bool) -> App {
    let args = Args {
      output: None,
      as_format: Some("json".to_owned()),
      delim,
      table: None,
      vanilla,
      compact: false,
      completion: None,
      help: false,
      version: false,
      input: None,
      argv_had_args: false,
    };
    App::build(args, None).unwrap()
  }

  fn tmp_path(name: &str) -> PathBuf {
    std::env::temp_dir()
      .join(format!("2fer-app-{name}-{}", SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()))
  }

  #[test]
  fn test_format_from_command() {
    for (command, expected) in [
      ("2csv", "csv"),
      ("2tsv", "tsv"),
      ("2json", "json"),
      ("2json.exe", "json"),
      ("2jsonl", "jsonl"),
      ("2ndjson", "jsonl"),
      ("2md", "md"),
      ("2markdown", "md"),
      ("2db", "sqlite"),
      ("2sqlite", "sqlite"),
      ("2sqlite3", "sqlite"),
      ("2xlsx", "xlsx"),
      ("2yml", "yml"),
      ("2yaml", "yml"),
      ("2tsv.ExE", "tsv"),
    ] {
      assert_eq!(expected, format_from_command(command).unwrap().name(), "{command}");
    }
    assert!(format_from_command("2xls").is_none());
    assert!(format_from_command("2fer").is_none());
  }

  #[test]
  fn test_main_writes_to_output_path() {
    let input = tmp_path("main-input").with_extension("csv");
    let output = tmp_path("main-output").with_extension("json");
    fs::write(&input, b"name,score\nalice,1\n").unwrap();

    let args = Args {
      output: Some(output.clone()),
      as_format: None,
      delim: None,
      table: None,
      vanilla: false,
      compact: false,
      completion: None,
      help: false,
      version: false,
      input: Some(input.clone()),
      argv_had_args: false,
    };

    App::build(args, Some("2fer".to_owned())).unwrap().main().unwrap();

    let text = fs::read_to_string(&output).unwrap();
    assert!(text.contains("\"name\": \"alice\""));
    assert!(text.contains("\"score\": 1"));

    fs::remove_file(input).unwrap();
    fs::remove_file(output).unwrap();
  }

  #[test]
  fn test_main_rejects_empty_input() {
    for (name, bytes) in [("empty", &b""[..]), ("headers-only", &b"name,score\n"[..])] {
      let input = tmp_path(name).with_extension("csv");
      let output = tmp_path(name).with_extension("json");
      fs::write(&input, bytes).unwrap();

      let args = Args {
        output: Some(output.clone()),
        as_format: None,
        delim: None,
        table: None,
        vanilla: false,
        compact: false,
        completion: None,
        help: false,
        version: false,
        input: Some(input.clone()),
        argv_had_args: false,
      };

      let error = App::build(args, Some("2fer".to_owned())).unwrap().main().unwrap_err();
      assert!(matches!(error, Error::EmptyFile), "{name}");

      fs::remove_file(input).unwrap();
      let _ = fs::remove_file(output);
    }
  }

  #[test]
  fn test_read_file_csv_sniffs_semicolon() {
    let mut app = dispatch_app(None, false);
    let path = tmp_path("semicolon");
    fs::write(&path, b"a;b\n1;2\n3;4\n5;6\n").unwrap();

    let format = formats::detect_by_path(&path).unwrap();
    let table = app.read_from_path_with_format(&path, format).unwrap();

    assert_eq!(["a", "b"], table.headers.as_slice());
    assert_eq!("1", table.rows[0][0].text());
    fs::remove_file(path).unwrap();
  }

  #[test]
  fn test_read_file_csv_sniffs_tab() {
    let mut app = dispatch_app(None, false);
    let path = tmp_path("tab");
    fs::write(&path, b"a\tb\n1\t2\n3\t4\n5\t6\n").unwrap();

    let format = formats::detect_by_path(&path).unwrap();
    let table = app.read_from_path_with_format(&path, format).unwrap();

    assert_eq!(["a", "b"], table.headers.as_slice());
    assert_eq!("1", table.rows[0][0].text());
    fs::remove_file(path).unwrap();
  }

  #[test]
  fn test_read_file_strips_bom() {
    let mut app = dispatch_app(None, false);
    let path = tmp_path("bom");
    fs::write(&path, b"\xef\xbb\xbfa,b\n1,2\n").unwrap();

    let format = formats::detect_by_path(&path).unwrap();
    let table = app.read_from_path_with_format(&path, format).unwrap();

    assert_eq!(["a", "b"], table.headers.as_slice());
    assert_eq!("1", table.rows[0][0].text());
    assert_eq!("csv", app.input_format.unwrap().name());
    fs::remove_file(path).unwrap();
  }

  #[test]
  fn test_validate_table_arg() {
    let csv_path = tmp_path("input").with_extension("csv");
    let sqlite_path = tmp_path("input").with_extension("sqlite");

    for (app, expected) in [
      (input_app(None, Some("players"), formats::find("json").unwrap(), None), Err(Error::TableOptionRequiresSqlite)),
      (
        input_app(Some(PathBuf::from("-")), Some("players"), formats::find("json").unwrap(), None),
        Err(Error::TableOptionRequiresSqlite),
      ),
      (
        input_app(
          Some(csv_path.clone()),
          Some("players"),
          formats::find("json").unwrap(),
          Some(formats::find("csv").unwrap()),
        ),
        Err(Error::TableOptionRequiresSqlite),
      ),
      (input_app(None, Some("players"), formats::find("sqlite").unwrap(), None), Ok(())),
      (
        input_app(
          Some(sqlite_path.clone()),
          Some("players"),
          formats::find("json").unwrap(),
          Some(formats::find("sqlite").unwrap()),
        ),
        Ok(()),
      ),
      (
        input_app(Some(csv_path.clone()), None, formats::find("json").unwrap(), Some(formats::find("csv").unwrap())),
        Ok(()),
      ),
    ] {
      assert_eq!(error_kind(expected), error_kind(app.validate_sqlite()));
    }
  }

  fn error_kind(result: Result<()>) -> Option<&'static str> {
    match result {
      Ok(()) => None,
      Err(Error::TableOptionRequiresSqlite) => Some("table_option_requires_sqlite"),
      Err(_) => Some("other"),
    }
  }
}
