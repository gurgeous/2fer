//! CSV/TSV support. Mostly a wrapper around `csv` crate. with delim
//! sniffing.

use super::{Format, infer, nose};
use crate::{app::App, cell::Cell, error::Result, table::Table};

//
// format
//

#[derive(Debug)]
pub(super) struct Csv {
  delimiter: u8,
  exts: &'static [&'static str],
  sniff: bool,
}

impl Format for Csv {
  fn exts(&self) -> &'static [&'static str] {
    self.exts
  }

  //
  // read/write
  //

  fn read_from_bytes(&self, app: &App, bytes: &[u8]) -> Result<Table> {
    read_csv(bytes, self.input_delimiter(app, bytes), app.args.vanilla)
  }
  fn write_to_bytes(&self, _app: &App, table: &Table) -> Result<Vec<u8>> {
    let mut writer = csv::WriterBuilder::new()
      .delimiter(self.delimiter)
      .terminator(csv::Terminator::Any(b'\n'))
      .from_writer(Vec::new());
    writer.write_record(&table.headers)?;
    for row in &table.rows {
      writer.write_record(row.iter().map(Cell::text))?;
    }
    Ok(writer.into_inner().expect("csv writer over Vec should not fail"))
  }
}

//
// helpers
//

impl Csv {
  pub(super) const fn new(exts: &'static [&'static str], delimiter: u8, sniff: bool) -> Self {
    Self { exts, delimiter, sniff }
  }

  fn input_delimiter(&self, app: &App, bytes: &[u8]) -> u8 {
    if let Some(delimiter) = app.args.delim {
      crate::util::log_2fer(format_args!("  delimiter source=args value={}", crate::util::inspect_byte(delimiter)));
      return delimiter;
    }

    if self.sniff
      && let Some(delimiter) = nose::sniff(&String::from_utf8_lossy(bytes))
    {
      crate::util::log_2fer(format_args!("  delimiter source=sniff value={}", crate::util::inspect_byte(delimiter)));
      return delimiter;
    }

    crate::util::log_2fer(format_args!(
      "  delimiter source=default value={}",
      crate::util::inspect_byte(self.delimiter)
    ));
    self.delimiter
  }
}

pub(super) fn read_csv(bytes: &[u8], delimiter: u8, vanilla: bool) -> Result<Table> {
  // read csv
  let mut reader = csv::ReaderBuilder::new().has_headers(false).flexible(true).delimiter(delimiter).from_reader(bytes);
  let mut records = Vec::new();
  for record in reader.byte_records() {
    let record = record?;
    records.push(record.iter().map(|field| String::from_utf8_lossy(field).into_owned()).collect::<Vec<_>>());
  }

  // bail on empty
  if records.is_empty() {
    return Ok(Table { headers: Vec::new(), rows: Vec::new() });
  }

  // pull out headers
  let headers = records.remove(0);
  if vanilla {
    // don't infer
    let rows = records.into_iter().map(|row| row.into_iter().map(Cell::Text).collect()).collect();
    return Table::from_grid(headers, rows);
  }

  // infer
  infer::infer_table(headers, records)
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::{cell::Cell, error::Error};

  fn app(as_format: &str) -> App {
    let args = crate::args::Args {
      output: None,
      as_format: Some(as_format.to_owned()),
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

  #[test]
  fn test_read_pads_short_rows_and_rejects_long_rows() {
    let table = read_csv(b"a,b\n1\n", b',', false).unwrap();
    assert_eq!(vec![Cell::Int(1), Cell::Null], table.rows[0]);

    assert!(matches!(read_csv(b"a,b\n1,2,3\n", b',', false), Err(Error::LongRow { .. })));
  }

  #[test]
  fn test_read_rejects_headers() {
    assert!(matches!(read_csv(b"a,a\n1,2\n", b',', false), Err(Error::DuplicateHeader(_))));
    assert!(matches!(read_csv(b"a,\n1,2\n", b',', false), Err(Error::EmptyHeader)));
  }

  #[test]
  fn test_read_vanilla_keeps_strings() {
    let table = read_csv(b"a\n1\n", b',', true).unwrap();
    assert_eq!(vec![Cell::Text("1".to_owned())], table.rows[0]);
  }

  #[test]
  fn test_write_csv_nested_as_json() {
    let table = Table {
      headers: vec!["name".to_owned(), "score".to_owned(), "meta".to_owned()],
      rows: vec![vec![Cell::Text("alice".to_owned()), Cell::Int(1), Cell::Json(serde_json::json!({"ok": true}))]],
    };
    let csv = Csv::new(&["csv"], b',', true);
    let out = String::from_utf8(csv.write_to_bytes(&app("csv"), &table).unwrap()).unwrap();
    assert_eq!("name,score,meta\nalice,1,\"{\"\"ok\"\":true}\"\n", out);
  }

  #[test]
  fn test_write_tsv() {
    let table = Table {
      headers: vec!["name".to_owned(), "score".to_owned()],
      rows: vec![vec![Cell::Text("alice".to_owned()), Cell::Int(1)]],
    };
    let tsv = Csv::new(&["tsv"], b'\t', false);
    let out = String::from_utf8(tsv.write_to_bytes(&app("tsv"), &table).unwrap()).unwrap();
    assert_eq!("name\tscore\nalice\t1\n", out);
  }
}
