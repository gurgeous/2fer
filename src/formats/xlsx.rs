//! XLSX format support. Detection can be tricky, because xlsx is a zip file.

use std::{
  fs::File,
  io::{Read, Seek},
  path::Path,
};

use calamine::{Data, Reader, Xlsx as CalamineXlsx, open_workbook, open_workbook_from_rs};
use rust_xlsxwriter::{Workbook, Worksheet, XlsxError};

use super::Format;
use crate::{
  app::App,
  cell::Cell,
  error::{Error, Result},
  table::Table,
};

//
// format
//

const XLSX_CONTENT_TYPES: &str = "[Content_Types].xml";
const XLSX_WORKBOOK: &str = "xl/workbook.xml";

/// XLSX format entry.
#[derive(Debug)]
pub(super) struct Xlsx;

impl Format for Xlsx {
  fn exts(&self) -> &'static [&'static str] {
    &["xlsx"]
  }

  fn detect_path(&self, path: &Path) -> bool {
    let Ok(file) = File::open(path) else {
      return false;
    };
    archive_has_workbook(file)
  }

  fn binary_output(&self) -> bool {
    true
  }

  fn read_from_bytes(&self, _app: &App, bytes: &[u8]) -> Result<Table> {
    let cursor = std::io::Cursor::new(bytes);
    let mut workbook: CalamineXlsx<_> = open_workbook_from_rs(cursor)?;
    read_xlsx(&mut workbook)
  }

  fn read_from_path(&self, _app: &App, path: &Path) -> Result<Table> {
    let mut workbook: CalamineXlsx<_> = open_workbook(path)?;
    read_xlsx(&mut workbook)
  }

  fn write_to_bytes(&self, _app: &App, table: &Table) -> Result<Vec<u8>> {
    write_xlsx(table)
  }
}

//
// helpers
//

fn archive_has_workbook<R: Read + Seek>(reader: R) -> bool {
  let Ok(mut archive) = zip::ZipArchive::new(reader) else {
    return false;
  };
  archive.by_name(XLSX_CONTENT_TYPES).is_ok() && archive.by_name(XLSX_WORKBOOK).is_ok()
}

fn read_xlsx<RS: Read + Seek>(workbook: &mut CalamineXlsx<RS>) -> Result<Table> {
  let range = workbook.worksheet_range_at(0).ok_or(Error::NoWorksheets)??;

  let mut rows = range.rows();
  let Some(headers) = rows.next() else {
    return Ok(Table { headers: Vec::new(), rows: Vec::new() });
  };
  let headers = headers.iter().map(|value| input_cell(value).text()).collect();
  let rows = rows.map(|row| row.iter().map(input_cell).collect()).collect();
  Table::from_grid(headers, rows)
}

fn write_xlsx(table: &Table) -> Result<Vec<u8>> {
  let mut workbook = Workbook::new();
  let worksheet = workbook.add_worksheet();

  for (col, header) in table.headers.iter().enumerate() {
    worksheet.write_string(0, col as u16, header)?;
  }
  for (row_index, row) in table.rows.iter().enumerate() {
    let row_number = (row_index + 1) as u32;
    for (col, cell) in row.iter().enumerate() {
      write_cell(worksheet, row_number, col as u16, cell)?;
    }
  }

  workbook.save_to_buffer().map_err(Into::into)
}

fn input_cell(value: &Data) -> Cell {
  match value {
    Data::Empty => Cell::Null,
    Data::String(value) => Cell::Text(value.clone()),
    Data::Float(value) => Cell::Float(*value),
    Data::Int(value) => Cell::Int(*value),
    Data::Bool(value) => Cell::Bool(*value),
    Data::DateTime(value) => Cell::Text(value.to_string()),
    Data::DateTimeIso(value) => Cell::Text(value.clone()),
    Data::DurationIso(value) => Cell::Text(value.clone()),
    Data::Error(value) => Cell::Text(value.to_string()),
  }
}

// Preserve scalar types where XLSX supports them.
fn write_cell(worksheet: &mut Worksheet, row: u32, col: u16, cell: &Cell) -> std::result::Result<(), XlsxError> {
  match cell {
    Cell::Null => Ok(()),
    Cell::Bool(value) => worksheet.write_boolean(row, col, *value).map(|_| ()),
    Cell::Int(value) => worksheet.write_number(row, col, *value as f64).map(|_| ()),
    Cell::Float(value) => worksheet.write_number(row, col, *value).map(|_| ()),
    Cell::Text(value) => worksheet.write_string(row, col, value).map(|_| ()),
    Cell::Json(_) => worksheet.write_string(row, col, cell.text()).map(|_| ()),
  }
}

#[cfg(test)]
mod tests {
  use std::fs;

  use super::*;

  fn app() -> App {
    let args = crate::args::Args {
      output: None,
      as_format: Some("xlsx".to_owned()),
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
  fn test_write_xlsx_round_trips_values() {
    let source = Table {
      headers: vec!["name".to_owned(), "score".to_owned(), "ok".to_owned(), "meta".to_owned()],
      rows: vec![vec![
        Cell::Text("alice".to_owned()),
        Cell::Float(1.5),
        Cell::Bool(true),
        Cell::Json(serde_json::json!({"ok": true})),
      ]],
    };
    let out = write_xlsx(&source).unwrap();
    assert!(!Xlsx.detect_sample(&out));
    let table = Xlsx.read_from_bytes(&app(), &out).unwrap();
    assert_eq!(source.headers, table.headers);
    assert_eq!(Cell::Text("alice".to_owned()), table.rows[0][0]);
    assert_eq!(Cell::Float(1.5), table.rows[0][1]);
    assert_eq!(Cell::Bool(true), table.rows[0][2]);
    assert_eq!(Cell::Text("{\"ok\":true}".to_owned()), table.rows[0][3]);

    let file = crate::util::temp_file("xlsx", "xlsx").unwrap();
    fs::write(file.path(), &out).unwrap();
    assert!(Xlsx.detect_path(file.path()));
    assert_eq!(source.headers, Xlsx.read_from_path(&app(), file.path()).unwrap().headers);
  }

  #[test]
  fn test_binary_output() {
    assert!(Xlsx.binary_output());
  }
}
