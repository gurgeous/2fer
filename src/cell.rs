//! A single typed table cell.

use serde_json::{Number, Value};

//
// types
//

/// Typed table cell.
#[derive(Clone, Debug, PartialEq)]
pub enum Cell {
  Null,
  Bool(bool),
  Float(f64),
  Int(i64),
  Json(Value),
  Text(String),
}

//
// Cell
//

impl Cell {
  pub fn from_json(value: Value) -> Self {
    match value {
      Value::Null => Self::Null,
      Value::Bool(value) => Self::Bool(value),
      Value::Number(value) => number_cell(value),
      Value::String(value) => Self::Text(value),
      Value::Array(_) | Value::Object(_) => Self::Json(value),
    }
  }

  pub fn text(&self) -> String {
    match self {
      Self::Null => String::new(),
      Self::Bool(value) => value.to_string(),
      Self::Int(value) => value.to_string(),
      Self::Float(value) => value.to_string(),
      Self::Text(value) => value.clone(),
      Self::Json(value) => serde_json::to_string(value).expect("JSON value serialization should succeed"),
    }
  }
}

//
// helpers
//

fn number_cell(value: Number) -> Cell {
  if let Some(value) = value.as_i64() {
    Cell::Int(value)
  } else if let Some(value) = value.as_f64() {
    Cell::Float(value)
  } else {
    Cell::Text(value.to_string())
  }
}

//
// tests
//

#[cfg(test)]
mod tests {
  use serde_json::{Value, json};

  use super::*;

  #[test]
  fn test_from_json_scalars() {
    for (value, expected) in [
      (Value::Null, Cell::Null),
      (json!(true), Cell::Bool(true)),
      (json!(7), Cell::Int(7)),
      (json!(1.5), Cell::Float(1.5)),
      (json!("alice"), Cell::Text("alice".to_owned())),
      (json!([1, 2]), Cell::Json(json!([1, 2]))),
      (json!({"a": 1}), Cell::Json(json!({"a": 1}))),
    ] {
      assert_eq!(expected, Cell::from_json(value));
    }
  }

  #[test]
  fn test_text() {
    for (cell, expected) in [
      (Cell::Null, ""),
      (Cell::Bool(false), "false"),
      (Cell::Int(7), "7"),
      (Cell::Float(1.0), "1"),
      (Cell::Float(1.5), "1.5"),
      (Cell::Text("alice".to_owned()), "alice"),
    ] {
      assert_eq!(expected, cell.text());
    }
  }

  #[test]
  fn test_nested_text_is_compact_json() {
    let cell = Cell::Json(json!({"b": [1, true]}));
    assert_eq!("{\"b\":[1,true]}", cell.text());
  }
}
