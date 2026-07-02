//! A single typed table cell.

use serde_json::{Number, Value};

use crate::error::{Error, Result};

//
// types
//

const MAX_NESTING_DEPTH: usize = 64;

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
  pub fn from_json(value: Value) -> Result<Self> {
    check_depth_at(&value, 0)?;
    Ok(match value {
      Value::Null => Self::Null,
      Value::Bool(value) => Self::Bool(value),
      Value::Number(value) => number_cell(value),
      Value::String(value) => Self::Text(value),
      Value::Array(_) | Value::Object(_) => Self::Json(value),
    })
  }

  pub fn to_json(&self) -> Value {
    match self {
      Self::Null => Value::Null,
      Self::Bool(value) => Value::Bool(*value),
      Self::Int(value) => Value::Number(Number::from(*value)),
      Self::Float(value) => Number::from_f64(*value).map(Value::Number).unwrap_or(Value::Null),
      Self::Text(value) => Value::String(value.clone()),
      Self::Json(value) => value.clone(),
    }
  }

  pub fn text(&self) -> String {
    match self {
      Self::Null => String::new(),
      Self::Bool(value) => value.to_string(),
      Self::Int(value) => value.to_string(),
      Self::Float(value) => format_float(*value),
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

fn format_float(value: f64) -> String {
  let text = value.to_string();
  text.strip_suffix(".0").unwrap_or(&text).to_owned()
}

// Guard recursive JSON values before storing them in a cell.
fn check_depth_at(value: &Value, depth: usize) -> Result<()> {
  if depth > MAX_NESTING_DEPTH {
    return Err(Error::NestedTooDeep);
  }
  match value {
    Value::Array(values) => {
      for value in values {
        check_depth_at(value, depth + 1)?;
      }
    }
    Value::Object(values) => {
      for value in values.values() {
        check_depth_at(value, depth + 1)?;
      }
    }
    _ => {}
  }
  Ok(())
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
      assert_eq!(expected, Cell::from_json(value).unwrap());
    }
  }

  #[test]
  fn test_to_json() {
    for (cell, expected) in [
      (Cell::Null, Value::Null),
      (Cell::Bool(true), json!(true)),
      (Cell::Int(7), json!(7)),
      (Cell::Float(1.5), json!(1.5)),
      (Cell::Float(f64::INFINITY), Value::Null),
      (Cell::Text("alice".to_owned()), json!("alice")),
      (Cell::Json(json!({"a": 1})), json!({"a": 1})),
    ] {
      assert_eq!(expected, cell.to_json());
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

  #[test]
  fn test_from_json_allows_max_depth_nested_values() {
    let mut value = Value::Null;
    for _ in 0..MAX_NESTING_DEPTH {
      value = json!([value]);
    }
    assert!(Cell::from_json(value).is_ok());
  }

  #[test]
  fn test_from_json_rejects_too_deep_nested_values() {
    let mut value = Value::Null;
    for _ in 0..=MAX_NESTING_DEPTH {
      value = json!([value]);
    }
    assert!(matches!(Cell::from_json(value), Err(Error::NestedTooDeep)));
  }
}
