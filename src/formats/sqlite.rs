//! SQLite format support. Unlike our other formats, this requires shelling out
//! to the `sqlite3` bin.

use std::{
  fs,
  io::{self, Read, Write},
  path::{Path, PathBuf},
  process::{Command, Stdio},
};

use serde_json::Value;

use super::{Format, json_value};
use crate::{
  app::App,
  cell::Cell,
  error::{Error, Result},
  table::Table,
};

//
// format
//

const BIN: &str = "sqlite3";
const SQLITE_MAGIC: &[u8] = b"SQLite format 3";

/// SQLite format entry.
#[derive(Debug)]
pub(super) struct Sqlite;

impl Format for Sqlite {
  fn exts(&self) -> &'static [&'static str] {
    &["sqlite", "db", "sqlite3"]
  }

  fn detect_sample(&self, sample: &[u8]) -> bool {
    sample.starts_with(SQLITE_MAGIC)
  }

  fn binary_output(&self) -> bool {
    true
  }

  fn read_from_bytes(&self, app: &App, bytes: &[u8]) -> Result<Table> {
    let mut file = crate::util::temp_file("sqlite", "sqlite")?;
    file.write_all(bytes).map_err(|error| Error::WriteFile { path: file.path().to_owned(), error })?;
    file.flush().map_err(|error| Error::WriteFile { path: file.path().to_owned(), error })?;
    read_sqlite(file.path(), app.args.table.as_deref())
  }

  fn read_from_reader(&self, app: &App, reader: &mut dyn Read) -> Result<Table> {
    let mut file = crate::util::temp_file("sqlite", "sqlite")?;
    io::copy(reader, file.as_file_mut()).map_err(|error| Error::WriteFile { path: file.path().to_owned(), error })?;
    file.as_file_mut().flush().map_err(|error| Error::WriteFile { path: file.path().to_owned(), error })?;
    read_sqlite(file.path(), app.args.table.as_deref())
  }

  fn read_from_path(&self, app: &App, path: &Path) -> Result<Table> {
    read_sqlite(path, app.args.table.as_deref())
  }

  fn write_to_bytes(&self, app: &App, table: &Table) -> Result<Vec<u8>> {
    let file = crate::util::temp_file("sqlite", "sqlite")?;
    sqlite3_write(table, app.args.table.as_deref().unwrap_or("data"), file.path())
      .and_then(|_| fs::read(file.path()).map_err(|error| Error::FileRead { path: file.path().to_owned(), error }))
  }

  fn write_to_path(&self, app: &App, path: &Path, table: &Table) -> Result<()> {
    write_sqlite_to_path(table, app.args.table.as_deref().unwrap_or("data"), path)
  }
}

//
// helpers
//

/// SQLite storage class selected for one output column.
#[derive(Clone, Copy, Eq, PartialEq)]
enum SqliteType {
  Integer,
  Real,
  Text,
}

//
// read
//

/// Export one SQLite table as JSON, then reuse JSON table conversion.
fn read_sqlite(path: &Path, selected_table: Option<&str>) -> Result<Table> {
  let tables = list_tables(path)?;
  let table = choose_table(&tables, selected_table)?;
  let sql = format!("SELECT * FROM {};", quote_identifier(&table));
  let stdout = sqlite3_read(path, &["-batch", "-json"], &sql)?;
  let Value::Array(rows) = serde_json::from_slice(&stdout)? else {
    return Err(Error::JsonArrayExpected);
  };
  json_value::object_rows_to_table(rows.into_iter().enumerate().map(|(index, row)| (index + 1, row)), |_| {
    Error::JsonObjectExpected
  })
}

fn list_tables(path: &Path) -> Result<Vec<String>> {
  let sql = "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name;";
  let stdout = sqlite3_read(path, &["-batch", "-noheader"], sql)?;
  let text = String::from_utf8_lossy(&stdout);
  Ok(text.lines().map(str::trim).filter(|line| !line.is_empty()).map(ToOwned::to_owned).collect())
}

fn choose_table(tables: &[String], selected_table: Option<&str>) -> Result<String> {
  if tables.is_empty() {
    return Err(Error::SqliteNoTables);
  }
  if let Some(selected_table) = selected_table {
    return tables
      .iter()
      .find(|table| table.eq_ignore_ascii_case(selected_table))
      .cloned()
      .ok_or_else(|| Error::SqliteInvalidTable(selected_table.to_owned(), tables.to_vec()));
  }

  Ok(tables[0].clone())
}

// Keep sqlite3 invocation shell-free and read-only.
fn sqlite3_read(path: &Path, args: &[&str], sql: &str) -> Result<Vec<u8>> {
  let mut command = Command::new(BIN);
  command.arg("-readonly").args(args).arg(path_arg(path)).arg(sql);

  let output = command.output().map_err(|err| match err.kind() {
    std::io::ErrorKind::NotFound => Error::SqliteCliMissing,
    _ => Error::SqliteCliFailed(err.to_string()),
  })?;
  if !output.status.success() {
    return Err(Error::SqliteCliFailed(sqlite_error(&output.stderr)));
  }
  Ok(output.stdout)
}

//
// write
//

fn sqlite3_write(table: &Table, table_name: &str, path: &Path) -> Result<()> {
  let sql = script(table, table_name);
  let mut child =
    Command::new(BIN).arg(path).stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::piped()).spawn().map_err(
      |error| match error.kind() {
        std::io::ErrorKind::NotFound => Error::SqliteCliMissing,
        _ => Error::SqliteCliFailed(error.to_string()),
      },
    )?;
  child
    .stdin
    .take()
    .expect("sqlite3 stdin should be piped")
    .write_all(sql.as_bytes())
    .map_err(|error| Error::SqliteCliFailed(error.to_string()))?;

  let output = child.wait_with_output().map_err(|error| Error::SqliteCliFailed(error.to_string()))?;
  if output.status.success() { Ok(()) } else { Err(Error::SqliteCliFailed(sqlite_error(&output.stderr))) }
}

// Build the new database beside the destination, then atomically replace it.
fn write_sqlite_to_path(table: &Table, table_name: &str, path: &Path) -> Result<()> {
  let parent = path.parent().filter(|path| !path.as_os_str().is_empty()).unwrap_or_else(|| Path::new("."));
  let file = tempfile::Builder::new()
    .prefix(".2fer-sqlite-")
    .suffix(".sqlite")
    .tempfile_in(parent)
    .map_err(|error| Error::WriteFile { path: parent.to_owned(), error })?;
  sqlite3_write(table, table_name, file.path())?;
  file.persist(path).map_err(|error| Error::WriteFile { path: path.to_owned(), error: error.error })?;
  Ok(())
}

fn script(table: &Table, table_name: &str) -> String {
  let table_name = quote_identifier(table_name);
  let columns = table
    .headers
    .iter()
    .enumerate()
    .map(|(index, header)| format!("{} {}", quote_identifier(header), column_type(table, index)))
    .collect::<Vec<_>>()
    .join(", ");
  let names = table.headers.iter().map(|header| quote_identifier(header)).collect::<Vec<_>>().join(", ");

  let mut sql = format!("BEGIN;\nCREATE TABLE {table_name} ({columns});\n");
  for row in &table.rows {
    let values = row.iter().map(value).collect::<Vec<_>>().join(", ");
    sql.push_str(&format!("INSERT INTO {table_name} ({names}) VALUES ({values});\n"));
  }
  sql.push_str("COMMIT;\n");
  sql
}

fn column_type(table: &Table, index: usize) -> &'static str {
  let mut ty = None;
  for row in &table.rows {
    ty = match (ty, &row[index]) {
      (Some(SqliteType::Text), _) | (_, Cell::Text(_) | Cell::Json(_)) => Some(SqliteType::Text),
      (Some(SqliteType::Real), Cell::Null | Cell::Int(_) | Cell::Bool(_) | Cell::Float(_)) => Some(SqliteType::Real),
      (Some(SqliteType::Integer), Cell::Float(_)) => Some(SqliteType::Real),
      (Some(SqliteType::Integer), Cell::Null | Cell::Int(_) | Cell::Bool(_)) => Some(SqliteType::Integer),
      (None, Cell::Float(_)) => Some(SqliteType::Real),
      (None, Cell::Int(_) | Cell::Bool(_)) => Some(SqliteType::Integer),
      (None, Cell::Null) => None,
    };
  }
  match ty.unwrap_or(SqliteType::Text) {
    SqliteType::Integer => "INTEGER",
    SqliteType::Real => "REAL",
    SqliteType::Text => "TEXT",
  }
}

fn value(cell: &Cell) -> String {
  match cell {
    Cell::Null => "NULL".to_owned(),
    Cell::Bool(value) => (*value as u8).to_string(),
    Cell::Int(value) => value.to_string(),
    Cell::Float(value) => value.to_string(),
    Cell::Text(value) => quote_literal(value),
    Cell::Json(_) => quote_literal(&cell.text()),
  }
}

// Prevent dash-starting relative paths from being parsed as flags.
fn path_arg(path: &Path) -> PathBuf {
  if path.is_relative() && path.to_string_lossy().starts_with('-') {
    PathBuf::from(".").join(path)
  } else {
    path.to_path_buf()
  }
}

fn quote_identifier(value: &str) -> String {
  format!("\"{}\"", value.replace('"', "\"\""))
}

fn quote_literal(value: &str) -> String {
  format!("'{}'", value.replace('\'', "''"))
}

fn sqlite_error(stderr: &[u8]) -> String {
  String::from_utf8_lossy(stderr).trim().to_owned()
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::cell::Cell;

  #[test]
  fn test_path_arg() {
    assert_eq!(PathBuf::from("./-data.db"), path_arg(Path::new("-data.db")));
    assert_eq!(PathBuf::from("data.db"), path_arg(Path::new("data.db")));
  }

  #[test]
  fn test_quote_identifier() {
    assert_eq!("\"users\"", quote_identifier("users"));
    assert_eq!("\"a\"\"b\"", quote_identifier("a\"b"));
  }

  #[test]
  fn test_quote_literal() {
    assert_eq!("'alice'", quote_literal("alice"));
    assert_eq!("'a''b'", quote_literal("a'b"));
  }

  #[test]
  fn test_column_type() {
    let table = Table {
      headers: vec!["int".to_owned(), "float".to_owned(), "text".to_owned()],
      rows: vec![
        vec![Cell::Int(1), Cell::Int(2), Cell::Int(3)],
        vec![Cell::Null, Cell::Float(2.5), Cell::Text("x".to_owned())],
      ],
    };

    assert_eq!("INTEGER", column_type(&table, 0));
    assert_eq!("REAL", column_type(&table, 1));
    assert_eq!("TEXT", column_type(&table, 2));
  }

  #[test]
  fn test_binary_output() {
    assert!(Sqlite.binary_output());
  }
}

#[cfg(all(test, not(windows)))]
mod sqlite3_tests {
  use std::time::{SystemTime, UNIX_EPOCH};

  use super::*;
  use crate::cell::Cell;

  fn tmp_path(name: &str) -> PathBuf {
    std::env::temp_dir()
      .join(format!("2fer-{name}-{}", SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()))
  }

  fn app(table: Option<&str>) -> App {
    let args = crate::args::Args {
      output: None,
      as_format: Some("sqlite".to_owned()),
      delim: None,
      table: table.map(ToOwned::to_owned),
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

  #[test]
  fn test_read() {
    let db = tmp_path("sqlite.db");
    let status = Command::new(BIN)
      .arg(&db)
      .arg(
        "create table players(name text, score integer, empty text, missing text, code text); \
         insert into players values ('alice', 1, '', null, '123');",
      )
      .status()
      .unwrap();
    assert!(status.success());

    let table = read_sqlite(&db, Some("players")).unwrap();
    assert_eq!(["name", "score", "empty", "missing", "code"], table.headers.as_slice());
    assert_eq!(Cell::Text("alice".to_owned()), table.rows[0][0]);
    assert_eq!(Cell::Int(1), table.rows[0][1]);
    assert_eq!(Cell::Text(String::new()), table.rows[0][2]);
    assert_eq!(Cell::Null, table.rows[0][3]);
    assert_eq!(Cell::Text("123".to_owned()), table.rows[0][4]);
    assert!(matches!(read_sqlite(&db, Some("missing")), Err(Error::SqliteInvalidTable(..))));

    fs::remove_file(db).unwrap();
  }

  #[test]
  fn test_read_bytes() {
    let db = tmp_path("sqlite-bytes.db");
    let status = Command::new(BIN)
      .arg(&db)
      .arg("create table players(name text, score integer); insert into players values ('alice', 1);")
      .status()
      .unwrap();
    assert!(status.success());

    let bytes = fs::read(&db).unwrap();
    let table = Sqlite.read_from_bytes(&app(Some("players")), &bytes).unwrap();
    assert_eq!(["name", "score"], table.headers.as_slice());
    assert_eq!(Cell::Text("alice".to_owned()), table.rows[0][0]);
    assert_eq!(Cell::Int(1), table.rows[0][1]);

    fs::remove_file(db).unwrap();
  }

  #[test]
  fn test_read_defaults_to_first_table() {
    let db = tmp_path("sqlite-default.db");
    let status = Command::new(BIN)
      .arg(&db)
      .arg(
        "create table zed(name text); \
         create table alpha(name text); \
         insert into zed values ('wrong'); \
         insert into alpha values ('alice');",
      )
      .status()
      .unwrap();
    assert!(status.success());

    let table = read_sqlite(&db, None).unwrap();
    assert_eq!(["name"], table.headers.as_slice());
    assert_eq!(Cell::Text("alice".to_owned()), table.rows[0][0]);

    fs::remove_file(db).unwrap();
  }

  #[test]
  fn test_write_sqlite_round_trips_values() {
    let source = Table {
      headers: vec!["name".to_owned(), "score".to_owned(), "ok".to_owned(), "meta".to_owned()],
      rows: vec![vec![
        Cell::Text("alice".to_owned()),
        Cell::Float(1.5),
        Cell::Bool(true),
        Cell::Json(serde_json::json!({"ok": true})),
      ]],
    };
    let bytes = Sqlite.write_to_bytes(&app(Some("players")), &source).unwrap();
    let file = crate::util::temp_file("sqlite", "sqlite").unwrap();
    fs::write(file.path(), bytes).unwrap();

    let table = read_sqlite(file.path(), Some("players")).unwrap();
    assert_eq!(source.headers, table.headers);
    assert_eq!(Cell::Text("alice".to_owned()), table.rows[0][0]);
    assert_eq!(Cell::Float(1.5), table.rows[0][1]);
    assert_eq!(Cell::Int(1), table.rows[0][2]);
    assert_eq!(Cell::Text("{\"ok\":true}".to_owned()), table.rows[0][3]);
  }

  #[test]
  fn test_write_to_path_sqlite_round_trips_values() {
    let source = Table {
      headers: vec!["name".to_owned(), "score".to_owned()],
      rows: vec![vec![Cell::Text("alice".to_owned()), Cell::Int(1)]],
    };
    let file = crate::util::temp_file("sqlite", "sqlite").unwrap();

    Sqlite.write_to_path(&app(Some("players")), file.path(), &source).unwrap();

    let table = read_sqlite(file.path(), Some("players")).unwrap();
    assert_eq!(source.headers, table.headers);
    assert_eq!(Cell::Text("alice".to_owned()), table.rows[0][0]);
    assert_eq!(Cell::Int(1), table.rows[0][1]);
  }

  #[test]
  fn test_write_to_path_replaces_existing_after_successful_write() {
    let source = Table {
      headers: vec!["name".to_owned(), "score".to_owned()],
      rows: vec![vec![Cell::Text("alice".to_owned()), Cell::Int(1)]],
    };
    let file = crate::util::temp_file("sqlite", "sqlite").unwrap();
    fs::write(file.path(), b"old content").unwrap();

    Sqlite.write_to_path(&app(Some("players")), file.path(), &source).unwrap();

    let table = read_sqlite(file.path(), Some("players")).unwrap();
    assert_eq!(source.headers, table.headers);
    assert_eq!(Cell::Text("alice".to_owned()), table.rows[0][0]);
    assert_eq!(Cell::Int(1), table.rows[0][1]);
  }
}
