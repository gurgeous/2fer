//! Array of json objects

use std::{
  fs::File,
  io::{self, BufWriter, Write},
  path::Path,
};

use serde::Serialize;
use serde_json::Value;

use super::{Format, json_value, jsonl};
use crate::{
  app::App,
  error::{Error, Result},
  table::Table,
};

//
// format
//

/// JSON format entry.
#[derive(Debug)]
pub(super) struct Json;

impl Format for Json {
  fn exts(&self) -> &'static [&'static str] {
    &["json"]
  }

  fn detect_sample(&self, bytes: &[u8]) -> bool {
    let trimmed = bytes.trim_ascii_start();
    matches!(trimmed.first(), Some(b'[' | b'{')) && !jsonl::Jsonl.detect_sample(bytes)
  }

  fn read_from_bytes(&self, _app: &App, bytes: &[u8]) -> Result<Table> {
    let value: Value = serde_json::from_slice(bytes)?;
    let Value::Array(rows) = value else {
      return Err(Error::JsonArrayExpected);
    };
    json_value::object_rows_to_table(rows.into_iter().enumerate().map(|(index, row)| (index + 1, row)), |_| {
      Error::JsonObjectExpected
    })
  }

  fn write_to_bytes(&self, app: &App, table: &Table) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    write_json(app, table, &mut out)?;
    Ok(out)
  }

  fn write_to_writer(&self, app: &App, table: &Table, out: &mut dyn Write) -> Result<()> {
    let mut out = BufWriter::new(out);
    write_json(app, table, &mut out).map_err(|_| Error::Stdout)
  }

  fn write_to_path(&self, app: &App, path: &Path, table: &Table) -> Result<()> {
    let file = File::create(path).map_err(|error| Error::WriteFile { path: path.to_owned(), error })?;
    let mut out = BufWriter::new(file);
    write_json(app, table, &mut out)
      .map_err(|error| Error::WriteFile { path: path.to_owned(), error: io::Error::other(error) })
  }
}

//
// helpers
//

fn write_json<W: Write + ?Sized>(app: &App, table: &Table, out: &mut W) -> std::result::Result<(), serde_json::Error> {
  {
    let mut serializer = serde_json::Serializer::pretty(&mut *out);
    json_value::serializable_rows(table, app.args.compact).serialize(&mut serializer)?;
  }
  out.write_all(b"\n").map_err(serde_json::Error::io)?;
  out.flush().map_err(serde_json::Error::io)?;
  Ok(())
}

#[cfg(test)]
mod tests {
  use serde_json::json;

  use super::*;
  use crate::cell::Cell;

  fn table() -> Table {
    Table {
      headers: vec!["name".to_owned(), "score".to_owned(), "meta".to_owned()],
      rows: vec![vec![Cell::Text("alice".to_owned()), Cell::Int(1), Cell::Json(json!({"ok": true}))]],
    }
  }

  fn app(compact: bool) -> App {
    let args = crate::args::Args {
      output: None,
      as_format: Some("json".to_owned()),
      delim: None,
      table: None,
      vanilla: false,
      compact,
      completion: None,
      help: false,
      version: false,
      input: None,
      argv_had_args: false,
    };
    App::build(args, None).unwrap()
  }

  #[test]
  fn test_json_detect_sample() {
    for (bytes, expected) in [
      (&br#"[{"a":1}]"#[..], true),
      (&b"{\"a\":1}"[..], true),
      (&b"{\"a\":1}\n{\"a\":2}\n"[..], false),
      (&b"{\"a\":1}\n{\"a\":"[..], false),
      (&b"- a: 1\n"[..], false),
    ] {
      assert_eq!(expected, Json.detect_sample(bytes), "{:?}", String::from_utf8_lossy(bytes));
    }
  }

  #[test]
  fn test_read_array_of_objects() {
    let table = Json.read_from_bytes(&app(false), br#"[{"b":1,"a":{"ok":true}},{"c":2}]"#).unwrap();
    assert_eq!(["b", "a", "c"], table.headers.as_slice());
    assert_eq!(Cell::Json(json!({"ok": true})), table.rows[0][1]);
    assert_eq!(Cell::Null, table.rows[1][0]);
  }

  #[test]
  fn test_json_null_is_distinct_from_null_string() {
    let table = Json.read_from_bytes(&app(false), br#"[{"a":null,"b":"null"}]"#).unwrap();
    assert_eq!(Cell::Null, table.rows[0][0]);
    assert_eq!(Cell::Text("null".to_owned()), table.rows[0][1]);
  }

  #[test]
  fn test_reject_shapes() {
    assert!(matches!(Json.read_from_bytes(&app(false), br#"{"a":1}"#), Err(Error::JsonArrayExpected)));
    assert!(matches!(Json.read_from_bytes(&app(false), br#"[["a"]]"#), Err(Error::JsonObjectExpected)));
  }

  #[test]
  fn test_write_json_preserves_nested() {
    let out = String::from_utf8(Json.write_to_bytes(&app(false), &table()).unwrap()).unwrap();
    assert!(out.contains("\"meta\": {\n      \"ok\": true\n    }"));
  }

  #[test]
  fn test_write_compact_elides_null_object_fields() {
    let table = Table {
      headers: vec!["name".to_owned(), "score".to_owned(), "meta".to_owned()],
      rows: vec![vec![
        Cell::Text("alice".to_owned()),
        Cell::Null,
        Cell::Json(json!({"ok": true, "extra": null, "items": [null, {"drop": null, "keep": 1}]})),
      ]],
    };

    let json = String::from_utf8(Json.write_to_bytes(&app(true), &table).unwrap()).unwrap();
    assert!(!json.contains("\"score\""));
    assert!(!json.contains("\"extra\""));
    assert!(json.contains("\"items\""));
    assert!(json.contains("null"));
    assert!(json.contains("\"keep\": 1"));
  }
}
