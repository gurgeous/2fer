//! Error reporting.

use std::{fmt, io::ErrorKind, path::PathBuf};

//
// types
//

/// User-facing conversion failure.
#[derive(Debug)]
pub enum Error {
  BinaryStdout(&'static str),
  Calamine(calamine::Error),
  Csv(csv::Error),
  DuplicateHeader(String),
  EmptyFile,
  EmptyHeader,
  FileRead { path: PathBuf, error: std::io::Error },
  Json(serde_json::Error),
  JsonArrayExpected,
  JsonObjectExpected,
  JsonlRowMustBeObject(usize),
  LongRow { row: usize, expected: usize, actual: usize },
  MdShape(String),
  NoWorksheets,
  OutputFormatConflict(Vec<String>),
  OutputFormatRequired,
  SqliteCliFailed(String),
  SqliteCliMissing,
  SqliteInvalidTable(String, Vec<String>),
  SqliteNoTables,
  TableOptionRequiresSqlite,
  StdinRead,
  Stdout(ErrorKind),
  UnsupportedInputMishap(&'static str),
  UnsupportedOutputFormat(String),
  UnsupportedOutputExtension(String),
  Usage(String),
  WriteFile { path: PathBuf, error: std::io::Error },
  Xlsx(rust_xlsxwriter::XlsxError),
  Yml(serde_yaml::Error),
  YmlShape(String),
}

pub type Result<T> = std::result::Result<T, Error>;

//
// format
//

pub fn usage_hint(command_name: &str) -> String {
  format!("{command_name}: try '{command_name} --help' for more information\n")
}

pub fn format(error: &Error, command_name: &str) -> String {
  let mut out = String::new();
  for line in error.to_string().lines() {
    out.push_str(command_name);
    out.push_str(": ");
    out.push_str(line);
    out.push('\n');
  }
  if matches!(error, Error::Usage(_)) {
    out.push_str(&usage_hint(command_name));
  }
  out
}

//
// display
//

impl fmt::Display for Error {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      Self::BinaryStdout(format) => {
        write!(f, "Refusing to write {format} bytes to your terminal; use --output or redirect stdout")
      }
      Self::Calamine(error) => write!(f, "Could not read that xlsx file: {error}"),
      Self::Csv(error) => write!(f, "That delimited file does not look right: {error}"),
      Self::DuplicateHeader(header) => write!(f, "Duplicate header '{header}'"),
      Self::EmptyFile => f.write_str("Uh oh, the input file is empty"),
      Self::EmptyHeader => f.write_str("Empty headers are not allowed"),
      Self::FileRead { path, .. } => write!(f, "Could not read file '{}'", path.display()),
      Self::Json(error) => write!(f, "That JSON/JSONL file does not look right: {error}"),
      Self::JsonArrayExpected => f.write_str("JSON file must be an array of objects"),
      Self::JsonObjectExpected => f.write_str("Rows must be JSON objects"),
      Self::JsonlRowMustBeObject(line) => write!(f, "JSONL line {line} must be an object"),
      Self::LongRow { row, expected, actual } => {
        write!(f, "Row {row} has {actual} cells, but the header has {expected}")
      }
      Self::MdShape(message) => write!(f, "That markdown file does not look right: {message}"),
      Self::NoWorksheets => f.write_str("Workbook has no sheets"),
      Self::OutputFormatConflict(signals) => {
        write!(f, "Output format signals disagree: {}", signals.join(", "))
      }
      Self::OutputFormatRequired => {
        f.write_str("Output format is required; use --as, --output with an extension, or a 2csv/2json/etc symlink")
      }
      Self::SqliteCliFailed(message) if message.is_empty() => f.write_str("Could not run sqlite3"),
      Self::SqliteCliFailed(message) => write!(f, "Could not run sqlite3: {message}"),
      Self::SqliteCliMissing => f.write_str("Could not run `sqlite3`. Is it installed?"),
      Self::SqliteInvalidTable(table, tables) => {
        writeln!(f, "Table '{table}' was not found in that sqlite file.")?;
        write!(f, "Here are the tables in that file:")?;
        for table in tables {
          write!(f, "\n  {table}")?;
        }
        Ok(())
      }
      Self::SqliteNoTables => f.write_str("That sqlite file has no tables"),
      Self::TableOptionRequiresSqlite => f.write_str("--table only works with sqlite"),
      Self::StdinRead => f.write_str("Could not read from stdin"),
      Self::Stdout(_) => f.write_str("Could not write to stdout"),
      Self::UnsupportedInputMishap(kind) => {
        write!(f, "That file appears to be {kind}, not a supported tabular format")
      }
      Self::UnsupportedOutputFormat(format) => write!(f, "Unsupported output format '{format}'"),
      Self::UnsupportedOutputExtension(ext) => write!(f, "Unsupported output extension '.{ext}'"),
      Self::Usage(message) => f.write_str(message),
      Self::WriteFile { path, .. } => write!(f, "Could not write file '{}'", path.display()),
      Self::Xlsx(error) => write!(f, "Could not write XLSX output: {error}"),
      Self::Yml(error) => write!(f, "That YML file does not look right: {error}"),
      Self::YmlShape(message) => write!(f, "That YML file does not look right: {message}"),
    }
  }
}

//
// sources
//

impl std::error::Error for Error {
  fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
    match self {
      Self::Calamine(error) => Some(error),
      Self::Csv(error) => Some(error),
      Self::FileRead { error, .. } => Some(error),
      Self::Json(error) => Some(error),
      Self::WriteFile { error, .. } => Some(error),
      Self::Xlsx(error) => Some(error),
      Self::Yml(error) => Some(error),
      _ => None,
    }
  }
}

//
// constructors
//

impl Error {
  pub(crate) fn stdout(error: std::io::Error) -> Self {
    Self::Stdout(error.kind())
  }
}

//
// conversions
//

impl From<csv::Error> for Error {
  fn from(error: csv::Error) -> Self {
    Self::Csv(error)
  }
}

impl From<serde_json::Error> for Error {
  fn from(error: serde_json::Error) -> Self {
    Self::Json(error)
  }
}

impl From<calamine::Error> for Error {
  fn from(error: calamine::Error) -> Self {
    Self::Calamine(error)
  }
}

impl From<calamine::XlsxError> for Error {
  fn from(error: calamine::XlsxError) -> Self {
    Self::Calamine(calamine::Error::Xlsx(error))
  }
}

impl From<rust_xlsxwriter::XlsxError> for Error {
  fn from(error: rust_xlsxwriter::XlsxError) -> Self {
    Self::Xlsx(error)
  }
}

impl From<serde_yaml::Error> for Error {
  fn from(error: serde_yaml::Error) -> Self {
    Self::Yml(error)
  }
}

//
// tests
//

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_usage_hint() {
    assert_eq!("2csv: try '2csv --help' for more information\n", usage_hint("2csv"));
  }

  #[test]
  fn test_format_uses_command_name() {
    assert_eq!("2json: Could not read from stdin\n", format(&Error::StdinRead, "2json"));
    assert_eq!(
      "2csv: bad flag\n2csv: try '2csv --help' for more information\n",
      format(&Error::Usage("bad flag".to_owned()), "2csv")
    );
  }
}
