//! Table model with typed cells

use std::collections::{HashMap, HashSet};

use crate::{
  cell::Cell,
  error::{Error, Result},
};

//
// types
//

/// Rectangular table with headers.
#[derive(Clone, Debug, PartialEq)]
pub struct Table {
  pub headers: Vec<String>,
  pub rows: Vec<Vec<Cell>>,
}

//
// table
//

impl Table {
  pub fn from_grid(headers: Vec<String>, rows: Vec<Vec<Cell>>) -> Result<Self> {
    validate_headers(&headers)?;
    let expected = headers.len();
    let mut out = Vec::with_capacity(rows.len());
    for (index, mut row) in rows.into_iter().enumerate() {
      if row.len() > expected {
        return Err(Error::LongRow { row: index + 2, expected, actual: row.len() });
      }
      row.resize(expected, Cell::Null);
      out.push(row);
    }
    Ok(Self { headers, rows: out })
  }

  /// Build rows from sparse records while preserving first-seen header order.
  pub fn from_records(records: Vec<Vec<(String, Cell)>>) -> Result<Self> {
    let mut headers = Vec::new();
    let mut indexes = HashMap::new();
    for record in &records {
      for (key, _) in record {
        if !indexes.contains_key(key) {
          indexes.insert(key.clone(), headers.len());
          headers.push(key.clone());
        }
      }
    }
    validate_headers(&headers)?;

    let mut rows = Vec::with_capacity(records.len());
    for record in records {
      let mut row = vec![Cell::Null; headers.len()];
      for (key, value) in record {
        let index = indexes[&key];
        row[index] = value;
      }
      rows.push(row);
    }
    Ok(Self { headers, rows })
  }
}

//
// helpers
//

fn validate_headers(headers: &[String]) -> Result<()> {
  let mut seen = HashSet::new();
  for header in headers {
    if header.trim().is_empty() {
      return Err(Error::EmptyHeader);
    }
    if !seen.insert(header) {
      return Err(Error::DuplicateHeader(header.clone()));
    }
  }
  Ok(())
}

//
// tests
//

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_from_grid_pads_short_rows() {
    let table = Table::from_grid(vec!["a".to_owned(), "b".to_owned()], vec![vec![Cell::Text("x".to_owned())]]).unwrap();
    assert_eq!(vec![Cell::Text("x".to_owned()), Cell::Null], table.rows[0]);
  }

  #[test]
  fn test_from_grid_rejects_long_rows() {
    assert!(matches!(
      Table::from_grid(vec!["a".to_owned()], vec![vec![Cell::Null, Cell::Null]]),
      Err(Error::LongRow { .. })
    ));
  }

  #[test]
  fn test_from_grid_rejects_headers() {
    assert!(matches!(Table::from_grid(vec!["".to_owned()], vec![]), Err(Error::EmptyHeader)));
    assert!(matches!(Table::from_grid(vec!["a".to_owned(), "a".to_owned()], vec![]), Err(Error::DuplicateHeader(_))));
  }

  #[test]
  fn test_from_records_unions_keys_in_first_seen_order() {
    let table = Table::from_records(vec![
      vec![("b".to_owned(), Cell::Int(1)), ("a".to_owned(), Cell::Int(2))],
      vec![("c".to_owned(), Cell::Int(3))],
    ])
    .unwrap();
    assert_eq!(["b", "a", "c"], table.headers.as_slice());
    assert_eq!(vec![Cell::Null, Cell::Null, Cell::Int(3)], table.rows[1]);
  }

  #[test]
  fn test_from_records_rejects_empty_headers() {
    assert!(matches!(Table::from_records(vec![vec![("".to_owned(), Cell::Null)]]), Err(Error::EmptyHeader)));
  }
}
