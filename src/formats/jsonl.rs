//! JSONL and NDJSON support.

use std::{
  fs::File,
  io::{self, BufWriter, ErrorKind, Write},
  path::Path,
};

use serde_json::Value;

use super::{Format, json_value};
use crate::{
  app::App,
  error::{Error, Result},
  table::Table,
};

//
// format
//

const JSONL_SAMPLE_LINES: usize = 5;

/// JSONL format entry.
#[derive(Debug)]
pub(super) struct Jsonl;

impl Format for Jsonl {
  fn exts(&self) -> &'static [&'static str] {
    &["jsonl", "ndjson"]
  }

  fn detect_sample(&self, bytes: &[u8]) -> bool {
    is_jsonl_sample(bytes)
  }

  fn read_from_bytes(&self, _app: &App, bytes: &[u8]) -> Result<Table> {
    let text = String::from_utf8_lossy(bytes);
    let rows = text
      .lines()
      .enumerate()
      .filter(|(_, line)| !line.trim().is_empty())
      .map(|(index, line)| serde_json::from_str::<Value>(line).map(|value| (index + 1, value)).map_err(Error::Json));
    let mut values = Vec::new();
    for row in rows {
      values.push(row?);
    }
    json_value::object_rows_to_table(values, Error::JsonlRowMustBeObject)
  }

  fn write_to_bytes(&self, app: &App, table: &Table) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    write_jsonl(app, table, &mut out)?;
    Ok(out)
  }

  fn write_to_writer(&self, app: &App, table: &Table, out: &mut dyn Write) -> Result<()> {
    let mut out = BufWriter::new(out);
    write_jsonl(app, table, &mut out).map_err(stdout_error)
  }

  fn write_to_path(&self, app: &App, path: &Path, table: &Table) -> Result<()> {
    let file = File::create(path).map_err(|error| Error::WriteFile { path: path.to_owned(), error })?;
    let mut out = BufWriter::new(file);
    write_jsonl(app, table, &mut out)
      .map_err(|error| Error::WriteFile { path: path.to_owned(), error: io::Error::other(error) })
  }
}

//
// write
//

fn write_jsonl<W: Write + ?Sized>(app: &App, table: &Table, out: &mut W) -> std::result::Result<(), serde_json::Error> {
  for row in &table.rows {
    serde_json::to_writer(&mut *out, &json_value::serializable_row(&table.headers, row, app.args.compact))?;
    out.write_all(b"\n").map_err(serde_json::Error::io)?;
  }
  out.flush().map_err(serde_json::Error::io)?;
  Ok(())
}

fn stdout_error(error: serde_json::Error) -> Error {
  Error::Stdout(error.io_error_kind().unwrap_or(ErrorKind::Other))
}

//
// is_jsonl_sample
//

fn is_jsonl_sample(bytes: &[u8]) -> bool {
  let text = String::from_utf8_lossy(bytes);
  let mut lines =
    text.lines().map(str::trim).filter(|line| !line.is_empty()).take(JSONL_SAMPLE_LINES + 1).collect::<Vec<_>>();

  // Check bounded complete rows; a short sample may end mid-row.
  if lines.len() <= JSONL_SAMPLE_LINES && sample_ends_mid_nonblank_line(&text) {
    lines.pop();
  }

  lines
    .into_iter()
    .take(JSONL_SAMPLE_LINES)
    .map(serde_json::from_str::<Value>)
    .try_fold(0, |count, value| match value {
      Ok(Value::Object(_)) => Ok(count + 1),
      _ => Err(()),
    })
    .is_ok_and(|count| count > 0)
}

fn sample_ends_mid_nonblank_line(text: &str) -> bool {
  if text.ends_with('\n') || text.ends_with('\r') {
    return false;
  }
  let tail = text.rsplit_once('\n').map_or(text, |(_, tail)| tail);
  !tail.trim().is_empty()
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
      as_format: Some("jsonl".to_owned()),
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
  fn test_jsonl_detect_sample() {
    for (bytes, expected) in [
      (&b"{\"a\":1}\n{\"a\":2}\n"[..], true),
      (&b"\n{\"a\":1}\n\n{\"a\":2}\n"[..], true),
      (&b"{\"a\":1}\n{\"a\":"[..], true),
      (&b"{\"a\":"[..], false),
      (&b"{\"a\":1}\nnot json\n"[..], false),
      (&b"{\"a\":1}\n[2]\n"[..], false),
      (&b"{\"a\":1}\n{\"a\":2}\n{\"a\":3}\n{\"a\":4}\n{\"a\":5}\nnot json\n"[..], true),
    ] {
      assert_eq!(expected, Jsonl.detect_sample(bytes), "{:?}", String::from_utf8_lossy(bytes));
    }
  }

  #[test]
  fn test_read_jsonl() {
    let table = Jsonl.read_from_bytes(&app(false), b"{\"a\":1}\n\n{\"a\":2,\"b\":true}\n").unwrap();
    assert_eq!(["a", "b"], table.headers.as_slice());
    assert_eq!(Cell::Bool(true), table.rows[1][1]);
  }

  #[test]
  fn test_read_jsonl_rejects_non_object_rows() {
    assert!(matches!(Jsonl.read_from_bytes(&app(false), b"[1]\n"), Err(Error::JsonlRowMustBeObject(1))));
  }

  #[test]
  fn test_write_jsonl() {
    let out = String::from_utf8(Jsonl.write_to_bytes(&app(false), &table()).unwrap()).unwrap();
    assert_eq!("{\"name\":\"alice\",\"score\":1,\"meta\":{\"ok\":true}}\n", out);
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

    let jsonl = String::from_utf8(Jsonl.write_to_bytes(&app(true), &table).unwrap()).unwrap();
    assert_eq!("{\"name\":\"alice\",\"meta\":{\"ok\":true,\"items\":[null,{\"keep\":1}]}}\n", jsonl);
  }
}
