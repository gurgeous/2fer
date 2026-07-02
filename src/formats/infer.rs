//! Infer bool/numeric column types from string data. This is helpful for
//! conversions like csv => json, where we only receive strings and we want to
//! use the correct JSON type. Note that we examine the ENTIRE column but fail
//! fast, we don't want to accidentally mess up a stray cell.
//!
//! We lean heavily on the helpers in parse.rs to infer individual cell types.

use std::{fmt, time::Instant};

use super::parse::{parse_bool, parse_float, parse_int};
use crate::{cell::Cell, error::Result, table::Table, util};

//
// types
//

/// What kind of column is this?
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ColumnType {
  Bool,
  Float,
  Int,
  String,
}

impl fmt::Display for ColumnType {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      Self::Bool => f.write_str("bool"),
      Self::Float => f.write_str("float"),
      Self::Int => f.write_str("int"),
      Self::String => f.write_str("string"),
    }
  }
}

//
// don't infer on these columns, which are often false-positives
//

const SKIP: &[&str] = &[
  // zip codes
  "zip",
  "zipcode",
  "zip_code",
  "zip5",
  "zip_5",
  "zip9",
  "zip_9",
  // postal codes
  "postal",
  "postalcode",
  "postal_code",
  "postcode",
  // phone numbers
  "phone",
  "phone_number",
  "tel",
  "telephone",
  "fax",
  "cell",
  "cell_phone",
  "mobile",
  "mobile_number",
  // tax identifiers
  "ssn",
  "ein",
  "tax_id",
  // product and classification codes
  "ean",
  "fips",
  "gtin",
  "isbn",
  "naics",
  "sic",
  "sku",
  "upc",
];

//
// infer
//

/// Convert text rows after inferring each column as a whole.
pub fn infer_table(headers: Vec<String>, rows: Vec<Vec<String>>) -> Result<Table> {
  let start = Instant::now();
  let types: Vec<ColumnType> =
    headers.iter().enumerate().map(|(index, header)| infer_column(header, &rows, index)).collect();
  util::log_2fer(format_args!(
    "  infer types {} {}",
    format_column_types(&headers, &types),
    util::format_elapsed(start.elapsed())
  ));

  // convert
  let rows = rows
    .into_iter()
    .map(|row| {
      row
        .into_iter()
        .enumerate()
        .map(|(index, value)| convert_cell(value, types.get(index).copied().unwrap_or(ColumnType::String)))
        .collect()
    })
    .collect();
  Table::from_grid(headers, rows)
}

//
// helpers
//

fn format_column_types(headers: &[String], types: &[ColumnType]) -> String {
  let out = headers.iter().zip(types).map(|(header, ty)| format!("{header}={ty}")).collect::<Vec<_>>().join(", ");
  if out.is_empty() { "none".to_owned() } else { out }
}

// Examine a specific columm - what type is it?
fn infer_column(header: &str, rows: &[Vec<String>], index: usize) -> ColumnType {
  if skip(header) {
    return ColumnType::String;
  }

  let mut ty = None;
  for value in rows.iter().filter_map(|row| row.get(index)) {
    if value.is_empty() {
      continue;
    }
    let nxt = value_type(value);
    ty = Some(match (ty, nxt) {
      (None, nxt) => nxt,
      (Some(prev), nxt) if prev == nxt => prev,
      (Some(ColumnType::Int | ColumnType::Float), ColumnType::Float | ColumnType::Int) => ColumnType::Float,
      _ => ColumnType::String,
    });
    if ty == Some(ColumnType::String) {
      break;
    }
  }
  ty.unwrap_or(ColumnType::String)
}

fn skip(header: &str) -> bool {
  SKIP.iter().any(|candidate| header.eq_ignore_ascii_case(candidate))
}

// Look at str, determine type (or string)
fn value_type(value: &str) -> ColumnType {
  if parse_bool(value).is_some() {
    ColumnType::Bool
  } else if parse_int(value).is_some() {
    ColumnType::Int
  } else if parse_float(value).is_some() {
    ColumnType::Float
  } else {
    ColumnType::String
  }
}

//
// convert
//

fn convert_cell(value: String, ty: ColumnType) -> Cell {
  if value.is_empty() {
    return Cell::Null;
  }
  match ty {
    ColumnType::Bool => Cell::Bool(parse_bool(&value).expect("column infer guarantees bool")),
    ColumnType::Int => Cell::Int(parse_int(&value).expect("column infer guarantees int")),
    ColumnType::Float => Cell::Float(
      parse_float(&value)
        .or_else(|| parse_int(&value).map(|value| value as f64))
        .expect("column infer guarantees float"),
    ),
    ColumnType::String => Cell::Text(value),
  }
}

//
// tests
//

#[cfg(test)]
mod tests {
  use super::*;

  //
  // helpers
  //

  fn make_table(headers: &[&str], rows: &[Vec<&str>]) -> Table {
    infer_table(
      headers.iter().map(|header| (*header).to_owned()).collect(),
      rows.iter().map(|row| row.iter().map(|value| (*value).to_owned()).collect()).collect(),
    )
    .unwrap()
  }

  #[test]
  fn test_infer_types() {
    let table = infer_table(
      vec![
        "bool".to_owned(),
        "int".to_owned(),
        "float".to_owned(),
        "currency".to_owned(),
        "percent".to_owned(),
        "blank".to_owned(),
      ],
      vec![vec!["true", "1", "1.5", "$1,234.50", "12.5%", ""], vec!["false", "2", "2", "$2.00", "5%", ""]]
        .into_iter()
        .map(|row| row.into_iter().map(str::to_owned).collect())
        .collect(),
    )
    .unwrap();
    assert_eq!(Cell::Bool(true), table.rows[0][0]);
    assert_eq!(Cell::Int(1), table.rows[0][1]);
    assert_eq!(Cell::Float(1.5), table.rows[0][2]);
    assert_eq!(Cell::Text("$1,234.50".to_owned()), table.rows[0][3]);
    assert_eq!(Cell::Text("12.5%".to_owned()), table.rows[0][4]);
    assert_eq!(Cell::Null, table.rows[0][5]);
    assert_eq!(Cell::Bool(false), table.rows[1][0]);
    assert_eq!(Cell::Int(2), table.rows[1][1]);
    assert_eq!(Cell::Float(2.0), table.rows[1][2]);
    assert_eq!(Cell::Text("$2.00".to_owned()), table.rows[1][3]);
    assert_eq!(Cell::Text("5%".to_owned()), table.rows[1][4]);
    assert_eq!(Cell::Null, table.rows[1][5]);
  }

  #[test]
  fn test_format_column_types() {
    assert_eq!(
      "name=string, score=int",
      format_column_types(&["name".to_owned(), "score".to_owned()], &[ColumnType::String, ColumnType::Int])
    );
    assert_eq!("none", format_column_types(&[], &[]));
  }

  #[test]
  fn test_mixed_types_stay_string() {
    let table = infer_table(vec!["a".to_owned()], vec![vec!["1".to_owned()], vec!["true".to_owned()]]).unwrap();
    assert_eq!(Cell::Text("1".to_owned()), table.rows[0][0]);
  }

  #[test]
  fn test_whitespace_values_stay_string() {
    let table = make_table(&["a"], &[vec!["1"], vec![" 2"]]);
    assert_eq!(Cell::Text("1".to_owned()), table.rows[0][0]);
    assert_eq!(Cell::Text(" 2".to_owned()), table.rows[1][0]);

    let table = make_table(&["a"], &[vec!["true"], vec![" false"]]);
    assert_eq!(Cell::Text("true".to_owned()), table.rows[0][0]);
    assert_eq!(Cell::Text(" false".to_owned()), table.rows[1][0]);

    let table = make_table(&["a"], &[vec!["null"], vec![" null"]]);
    assert_eq!(Cell::Text("null".to_owned()), table.rows[0][0]);
    assert_eq!(Cell::Text(" null".to_owned()), table.rows[1][0]);
  }

  #[test]
  fn test_empty_cells_do_not_force_type() {
    let table = make_table(&["a"], &[vec![""], vec!["7"]]);
    assert_eq!(Cell::Null, table.rows[0][0]);
    assert_eq!(Cell::Int(7), table.rows[1][0]);
  }

  #[test]
  fn test_literal_null_stays_string() {
    let table = make_table(&["a"], &[vec!["NULL"], vec!["null"]]);
    assert_eq!(Cell::Text("NULL".to_owned()), table.rows[0][0]);
    assert_eq!(Cell::Text("null".to_owned()), table.rows[1][0]);
  }

  #[test]
  fn test_leading_zero_ints_stay_string() {
    let table = infer_table(vec!["zip".to_owned()], vec![vec!["007".to_owned()], vec!["008".to_owned()]]).unwrap();
    assert_eq!(Cell::Text("007".to_owned()), table.rows[0][0]);
  }

  #[test]
  fn test_text_headers_skip_infer() {
    for header in [
      "zip",
      "ZIPCODE",
      "zip_code",
      "zip5",
      "zip_9",
      "postal",
      "postalcode",
      "postal_code",
      "postcode",
      "phone",
      "phone_number",
      "telephone",
      "tel",
      "fax",
      "cell",
      "cell_phone",
      "mobile",
      "mobile_number",
      "ssn",
      "ein",
      "tax_id",
      "fips",
      "sku",
      "upc",
      "ean",
      "gtin",
      "isbn",
      "naics",
      "sic",
    ] {
      let table = make_table(&[header], &[vec!["90210"]]);
      assert_eq!(Cell::Text("90210".to_owned()), table.rows[0][0], "{header}");
    }
  }

  #[test]
  fn test_text_headers_do_not_match_substrings() {
    for header in ["grid", "zipcode_extra", "home zip", "phone_number_alt", "year"] {
      let table = make_table(&[header], &[vec!["90210"]]);
      assert_eq!(Cell::Int(90210), table.rows[0][0], "{header}");
    }
  }

  #[test]
  fn test_overflowing_ints_stay_string() {
    let table = make_table(&["a"], &[vec!["9223372036854775808"]]);
    assert_eq!(Cell::Text("9223372036854775808".to_owned()), table.rows[0][0]);
  }

  #[test]
  fn test_non_finite_floats_stay_string() {
    let table = make_table(&["a"], &[vec!["1.0e999"]]);
    assert_eq!(Cell::Text("1.0e999".to_owned()), table.rows[0][0]);
  }

  #[test]
  fn test_currency_values_stay_string() {
    let table = make_table(&["amount"], &[vec!["-$1,234.50"], vec!["$2.00"]]);
    assert_eq!(Cell::Text("-$1,234.50".to_owned()), table.rows[0][0]);
    assert_eq!(Cell::Text("$2.00".to_owned()), table.rows[1][0]);
  }

  #[test]
  fn test_dollar_before_minus_stays_string() {
    let table = make_table(&["amount"], &[vec!["-$1.00"], vec!["$-2.00"]]);
    assert_eq!(Cell::Text("-$1.00".to_owned()), table.rows[0][0]);
    assert_eq!(Cell::Text("$-2.00".to_owned()), table.rows[1][0]);
  }

  #[test]
  fn test_percent_values_stay_string() {
    let table = make_table(&["a"], &[vec!["-3.5%"], vec!["5%"]]);
    assert_eq!(Cell::Text("-3.5%".to_owned()), table.rows[0][0]);
    assert_eq!(Cell::Text("5%".to_owned()), table.rows[1][0]);
  }

  #[test]
  fn test_int_float_promotion() {
    let table = infer_table(vec!["score".to_owned()], vec![vec!["1".to_owned()], vec!["3.5".to_owned()]]).unwrap();
    assert_eq!(Cell::Float(1.0), table.rows[0][0]);
    assert_eq!(Cell::Float(3.5), table.rows[1][0]);
  }

  #[test]
  fn test_late_mixed_value_stays_string() {
    let mut rows = vec![vec!["1"]; 50];
    rows.push(vec!["later"]);
    let table = make_table(&["a"], &rows);
    assert_eq!(Cell::Text("1".to_owned()), table.rows[0][0]);
    assert_eq!(Cell::Text("later".to_owned()), table.rows[50][0]);
  }

  #[test]
  fn test_short_rows_are_padded_after_infer() {
    let table = make_table(&["a", "b"], &[vec!["1"], vec!["2", "x"]]);
    assert_eq!(Cell::Int(1), table.rows[0][0]);
    assert_eq!(Cell::Null, table.rows[0][1]);
    assert_eq!(Cell::Int(2), table.rows[1][0]);
    assert_eq!(Cell::Text("x".to_owned()), table.rows[1][1]);
  }
}
