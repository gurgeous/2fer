//! Markdown table format support.

use super::{Format, infer};
use crate::{
  app::App,
  cell::Cell,
  error::{Error, Result},
  table::Table,
};

//
// format
//

/// Markdown table format entry.
#[derive(Debug)]
pub(super) struct Md;

impl Format for Md {
  fn exts(&self) -> &'static [&'static str] {
    &["md", "markdown"]
  }

  fn detect_sample(&self, bytes: &[u8]) -> bool {
    is_md(bytes)
  }

  fn read_from_bytes(&self, app: &App, bytes: &[u8]) -> Result<Table> {
    read_md(bytes, app.args.vanilla)
  }

  fn write_to_bytes(&self, _app: &App, table: &Table) -> Result<Vec<u8>> {
    write_md(table)
  }
}

//
// helpers
//

fn is_md(bytes: &[u8]) -> bool {
  let text = String::from_utf8_lossy(bytes);
  let mut lines = text.lines().map(str::trim).filter(|line| !line.is_empty());
  let Some(header) = lines.next().and_then(split_row) else {
    return false;
  };
  let Some(separator) = lines.next().and_then(split_row) else {
    return false;
  };
  !header.is_empty() && valid_separator(&separator) && header.len() == separator.len()
}

/// Parse a GitHub-style pipe table.
fn read_md(bytes: &[u8], vanilla: bool) -> Result<Table> {
  let rows = parse_rows(bytes)?;
  let Some((headers, rows)) = rows.split_first() else {
    return Ok(Table { headers: Vec::new(), rows: Vec::new() });
  };
  let rows = rows.to_vec();
  if vanilla {
    return Table::from_grid(
      headers.clone(),
      rows.into_iter().map(|row| row.into_iter().map(Cell::Text).collect()).collect(),
    );
  }
  infer::infer_table(headers.clone(), rows)
}

fn write_md(table: &Table) -> Result<Vec<u8>> {
  let headers = table.headers.iter().map(|header| output_cell(header)).collect::<Vec<_>>();
  let rows = table
    .rows
    .iter()
    .map(|row| row.iter().map(|value| output_cell(&value.text())).collect::<Vec<_>>())
    .collect::<Vec<_>>();
  let widths = column_widths(&headers, &rows);

  let mut out = String::new();
  push_row(&mut out, &headers, &widths);
  push_row(&mut out, &widths.iter().map(|width| "-".repeat(*width)).collect::<Vec<_>>(), &widths);
  for row in rows {
    push_row(&mut out, &row, &widths);
  }
  Ok(out.into_bytes())
}

fn column_widths(headers: &[String], rows: &[Vec<String>]) -> Vec<usize> {
  let mut widths = headers.iter().map(|header| display_width(header).max(3)).collect::<Vec<_>>();
  for row in rows {
    for (index, cell) in row.iter().enumerate() {
      widths[index] = widths[index].max(display_width(cell));
    }
  }
  widths
}

fn push_row(out: &mut String, cells: &[String], widths: &[usize]) {
  out.push('|');
  for (cell, width) in cells.iter().zip(widths) {
    out.push(' ');
    out.push_str(cell);
    out.push_str(&" ".repeat(width.saturating_sub(display_width(cell))));
    out.push_str(" |");
  }
  out.push('\n');
}

fn parse_rows(bytes: &[u8]) -> Result<Vec<Vec<String>>> {
  let text = String::from_utf8_lossy(bytes);
  let mut lines = text.lines().map(str::trim).filter(|line| !line.is_empty());
  let Some(headers) = lines.next().and_then(split_row) else {
    return Err(Error::MdShape("expected a pipe table header".to_owned()));
  };
  let Some(separator) = lines.next().and_then(split_row) else {
    return Err(Error::MdShape("expected a pipe table separator".to_owned()));
  };
  if headers.is_empty() || headers.len() != separator.len() || !valid_separator(&separator) {
    return Err(Error::MdShape("expected a pipe table separator".to_owned()));
  }

  let mut rows = Vec::new();
  rows.push(headers);
  for line in lines {
    let Some(row) = split_row(line) else {
      return Err(Error::MdShape("expected every row to be a pipe table row".to_owned()));
    };
    rows.push(row);
  }
  Ok(rows)
}

fn split_row(line: &str) -> Option<Vec<String>> {
  let mut line = line.trim();
  if !line.contains('|') {
    return None;
  }
  line = line.strip_prefix('|').unwrap_or(line);
  line = line.strip_suffix('|').unwrap_or(line);

  let mut cells = Vec::new();
  let mut cell = String::new();
  let mut chars = line.chars().peekable();
  while let Some(ch) = chars.next() {
    if ch == '\\' && chars.peek() == Some(&'|') {
      chars.next();
      cell.push('|');
    } else if ch == '|' {
      cells.push(cell.trim().to_owned());
      cell.clear();
    } else {
      cell.push(ch);
    }
  }
  cells.push(cell.trim().to_owned());
  Some(cells)
}

fn valid_separator(cells: &[String]) -> bool {
  cells.iter().all(|cell| {
    let cell = cell.trim();
    let cell = cell.strip_prefix(':').unwrap_or(cell);
    let cell = cell.strip_suffix(':').unwrap_or(cell);
    cell.len() >= 3 && cell.bytes().all(|byte| byte == b'-')
  })
}

fn output_cell(value: &str) -> String {
  value.replace(['\r', '\n'], " ").replace('|', r"\|")
}

fn display_width(value: &str) -> usize {
  value.chars().count()
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::cell::Cell;

  #[test]
  fn test_read_md() {
    let table = read_md(b"| name | score |\n| :--- | ---: |\n| alice | 1 |\n| bob | 2.5 |\n", false).unwrap();
    assert_eq!(["name", "score"], table.headers.as_slice());
    assert_eq!(Cell::Text("alice".to_owned()), table.rows[0][0]);
    assert_eq!(Cell::Float(2.5), table.rows[1][1]);
  }

  #[test]
  fn test_read_md_vanilla() {
    let table = read_md(b"| score |\n| --- |\n| 1 |\n", true).unwrap();
    assert_eq!(Cell::Text("1".to_owned()), table.rows[0][0]);
  }

  #[test]
  fn test_read_md_escaped_pipe() {
    let table = read_md(b"| name |\n| --- |\n| a\\|b |\n", false).unwrap();
    assert_eq!(Cell::Text("a|b".to_owned()), table.rows[0][0]);
  }

  #[test]
  fn test_looks_like_table() {
    assert!(is_md(b"| a |\n| --- |\n| 1 |\n"));
    assert!(!is_md(b"a,b\n1,2\n"));
    assert!(!is_md(b"| a |\n| nope |\n"));
  }

  #[test]
  fn test_write_md() {
    let table = Table {
      headers: vec!["name".to_owned(), "notes".to_owned()],
      rows: vec![
        vec![Cell::Text("alice".to_owned()), Cell::Text("a|b\nc".to_owned())],
        vec![Cell::Text("bob".to_owned()), Cell::Text("longer".to_owned())],
      ],
    };
    let out = String::from_utf8(write_md(&table).unwrap()).unwrap();
    assert_eq!("| name  | notes  |\n| ----- | ------ |\n| alice | a\\|b c |\n| bob   | longer |\n", out);
  }
}
