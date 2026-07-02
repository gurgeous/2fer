//! Format trait for reading/writing/detection. Trait is implemented by csv, json, md, etc.

use std::{
  fmt::Debug,
  io::{Read, Write},
  path::Path,
};

use crate::{
  app::App,
  error::{Error, Result},
  table::Table,
  util::strip_utf8_bom,
};

//
// trait
//

pub trait Format: Debug + Sync {
  //
  // simple attributes
  //

  /// Filename ext(s), like .json or .csv. First ext is the canonical name.
  fn exts(&self) -> &'static [&'static str];

  /// Canonical format name, used for display and `--as`.
  fn name(&self) -> &'static str {
    self.exts().first().copied().expect("format must define at least one extension")
  }

  /// Magic byte prefix, if any. Empty prefixes are ignored.
  fn magic(&self) -> &'static [&'static [u8]] {
    &[]
  }

  /// True when output is binary and unsafe to dump to stdout (ex - sqlite)
  fn binary_output(&self) -> bool {
    false
  }

  //
  // detect
  //

  /// Is this one of our exts? Case-insensitive
  fn has_ext(&self, ext: &str) -> bool {
    self.exts().iter().any(|candidate| ext.eq_ignore_ascii_case(candidate))
  }

  /// Does this chunk of data appear to be in our format? Look for magic
  /// numbers, heuristics, etc.
  fn detect_sample(&self, sample: &[u8]) -> bool {
    self.magic().iter().any(|magic| !magic.is_empty() && sample.starts_with(magic))
  }

  /// Does this file appear to be in our format? detect_sample is excellent and
  /// fast, but we can't rely on it for all file types. Example - xslx, which is
  /// wrapped in a zip archive.
  fn detect_path(&self, _path: &Path) -> bool {
    false
  }

  //
  // Read entrypoints. By default, formats only need to implement
  // read_from_bytes. Some formats are hamstrung and override these for perf
  // reasons. For example, sqlite relies on the external `sqlite3` command, and
  // has to create temp files without `read_from_path`.
  //

  /// Read stdin or buffered file contents.
  fn read_from_bytes(&self, app: &App, bytes: &[u8]) -> Result<Table>;

  /// Read streaming input, just calls read_from_bytes by default.
  fn read_from_reader(&self, app: &App, reader: &mut dyn Read) -> Result<Table> {
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes).map_err(|_| Error::StdinRead)?;
    self.read_from_bytes(app, strip_utf8_bom(&bytes))
  }

  /// Read file input, just calls read_from_bytes by default.
  fn read_from_path(&self, app: &App, path: &Path) -> Result<Table> {
    let bytes = std::fs::read(path).map_err(|error| Error::FileRead { path: path.to_owned(), error })?;
    self.read_from_bytes(app, strip_utf8_bom(&bytes))
  }

  //
  // Write entrypoints. Same story as above, by default, formats only need to
  // implement write_to_bytes but formats might choose to implement the others
  // for performance reasons. Example - jsonl streaming.
  //

  /// Write table to buf.
  fn write_to_bytes(&self, app: &App, table: &Table) -> Result<Vec<u8>>;

  /// Write table to writer, just calls write_to_bytes by default.
  fn write_to_writer(&self, app: &App, table: &Table, out: &mut dyn Write) -> Result<()> {
    let bytes = self.write_to_bytes(app, table)?;
    out.write_all(&bytes).map_err(|_| Error::Stdout)
  }

  /// Write table to file, just calls write_to_bytes by default.
  fn write_to_path(&self, app: &App, path: &Path, table: &Table) -> Result<()> {
    let bytes = self.write_to_bytes(app, table)?;
    std::fs::write(path, bytes).map_err(|error| Error::WriteFile { path: path.to_owned(), error })
  }
}

//
// tests
//

#[cfg(test)]
mod tests {
  use std::{
    fs,
    io::Cursor,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
  };

  use super::*;
  use crate::cell::Cell;

  //
  // test formats
  //

  /// Test-only byte format.
  #[derive(Debug)]
  struct ByteFormat;

  impl Format for ByteFormat {
    fn name(&self) -> &'static str {
      "byte"
    }

    fn exts(&self) -> &'static [&'static str] {
      &[]
    }

    fn read_from_bytes(&self, _app: &App, bytes: &[u8]) -> Result<Table> {
      Ok(Table {
        headers: vec!["value".to_owned()],
        rows: vec![vec![Cell::Text(String::from_utf8_lossy(bytes).into_owned())]],
      })
    }

    fn write_to_bytes(&self, _app: &App, _table: &Table) -> Result<Vec<u8>> {
      Ok(b"bytes\n".to_vec())
    }
  }

  /// Test-only direct path writer.
  #[derive(Debug)]
  struct DirectPathFormat;

  /// Test-only format with invalid empty magic.
  #[derive(Debug)]
  struct EmptyMagicFormat;

  /// Test-only magic format without deep path detection.
  #[derive(Debug)]
  struct MagicFormat;

  impl Format for DirectPathFormat {
    fn name(&self) -> &'static str {
      "direct-path"
    }

    fn exts(&self) -> &'static [&'static str] {
      &[]
    }

    fn read_from_bytes(&self, _app: &App, _bytes: &[u8]) -> Result<Table> {
      unreachable!("direct path test format does not read")
    }

    fn write_to_bytes(&self, _app: &App, _table: &Table) -> Result<Vec<u8>> {
      Ok(b"bytes\n".to_vec())
    }

    fn write_to_path(&self, _app: &App, path: &Path, _table: &Table) -> Result<()> {
      fs::write(path, b"path\n").map_err(|error| Error::WriteFile { path: path.to_owned(), error })
    }
  }

  impl Format for EmptyMagicFormat {
    fn name(&self) -> &'static str {
      "empty-magic"
    }

    fn exts(&self) -> &'static [&'static str] {
      &[]
    }

    fn magic(&self) -> &'static [&'static [u8]] {
      &[b""]
    }

    fn read_from_bytes(&self, _app: &App, _bytes: &[u8]) -> Result<Table> {
      unreachable!("empty magic test format does not read")
    }

    fn write_to_bytes(&self, _app: &App, _table: &Table) -> Result<Vec<u8>> {
      unreachable!("empty magic test format does not write")
    }
  }

  impl Format for MagicFormat {
    fn name(&self) -> &'static str {
      "magic"
    }

    fn exts(&self) -> &'static [&'static str] {
      &[]
    }

    fn magic(&self) -> &'static [&'static [u8]] {
      &[b"magic"]
    }

    fn read_from_bytes(&self, _app: &App, _bytes: &[u8]) -> Result<Table> {
      unreachable!("magic test format does not read")
    }

    fn write_to_bytes(&self, _app: &App, _table: &Table) -> Result<Vec<u8>> {
      unreachable!("magic test format does not write")
    }
  }

  //
  // helpers
  //

  fn app() -> App {
    let args = crate::args::Args {
      output: None,
      as_format: Some("json".to_owned()),
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
    App::build(args, None).unwrap()
  }

  fn table() -> Table {
    Table { headers: vec!["name".to_owned()], rows: vec![vec![Cell::Text("alice".to_owned())]] }
  }

  fn tmp_path(name: &str) -> std::path::PathBuf {
    std::env::temp_dir()
      .join(format!("2fer-format-{name}-{}", SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()))
  }

  #[test]
  fn test_read_from_path_defaults_to_bytes() {
    let path = tmp_path("read");
    fs::write(&path, b"value").unwrap();

    let table = ByteFormat.read_from_path(&app(), &path).unwrap();

    assert_eq!("value", table.rows[0][0].text());
    fs::remove_file(path).unwrap();
  }

  #[test]
  fn test_read_from_reader_defaults_to_bytes() {
    let mut reader = Cursor::new(b"\xef\xbb\xbfvalue");

    let table = ByteFormat.read_from_reader(&app(), &mut reader).unwrap();

    assert_eq!("value", table.rows[0][0].text());
  }

  #[test]
  fn test_write_to_path_defaults_to_bytes() {
    let path = tmp_path("bytes-output");

    ByteFormat.write_to_path(&app(), &path, &table()).unwrap();

    assert_eq!(b"bytes\n", fs::read(&path).unwrap().as_slice());
    fs::remove_file(path).unwrap();
  }

  #[test]
  fn test_write_to_writer_defaults_to_bytes() {
    let mut out = Vec::new();

    ByteFormat.write_to_writer(&app(), &table(), &mut out).unwrap();

    assert_eq!(b"bytes\n", out.as_slice());
  }

  #[test]
  fn test_write_to_path_can_be_overridden() {
    let path = tmp_path("direct-output");

    DirectPathFormat.write_to_path(&app(), &path, &table()).unwrap();

    assert_eq!(b"path\n", fs::read(&path).unwrap().as_slice());
    fs::remove_file(path).unwrap();
  }

  #[test]
  fn test_detect_sample_ignores_empty_magic() {
    assert!(!ByteFormat.detect_sample(b"value"));
    assert!(!EmptyMagicFormat.detect_sample(b"value"));
  }

  #[test]
  fn test_detect_path_defaults_false() {
    let path = tmp_path("detect-path");
    fs::write(&path, b"magic").unwrap();

    assert!(MagicFormat.detect_sample(b"magic"));
    assert!(!MagicFormat.detect_path(&path));

    fs::remove_file(path).unwrap();
  }

  #[test]
  fn test_binary_output_defaults_false() {
    assert!(!ByteFormat.binary_output());
  }
}
