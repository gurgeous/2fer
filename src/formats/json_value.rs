//! Shared halpers for json/jsonl

use serde::{
  Serialize, Serializer,
  ser::{SerializeMap, SerializeSeq},
};
use serde_json::Value;

use crate::{
  cell::Cell,
  error::{Error, Result},
  table::Table,
};

//
// owned values
//

pub(super) fn rows(table: &Table, compact: bool) -> Vec<Value> {
  let mut rows = table.json_rows();
  if compact {
    for row in &mut rows {
      compact_json(row);
    }
  }
  rows
}

pub(super) fn object_rows_to_table(
  rows: impl IntoIterator<Item = (usize, Value)>,
  object_error: impl Fn(usize) -> Error,
) -> Result<Table> {
  // Preserve first-seen object keys while keeping source-specific row errors.
  let mut records = Vec::new();
  for (line, row) in rows {
    let Value::Object(object) = row else {
      return Err(object_error(line));
    };
    let mut record = Vec::new();
    for (key, value) in object {
      record.push((key, Cell::from_json(value)?));
    }
    records.push(record);
  }
  Table::from_records(records)
}

//
// borrowed values
//

/// Borrowed table rows for streaming JSON serialization.
pub(super) struct Rows<'a> {
  table: &'a Table,
  compact: bool,
}

/// Borrowed table row for streaming JSON serialization.
pub(super) struct Row<'a> {
  headers: &'a [String],
  row: &'a [Cell],
  compact: bool,
}

/// Borrowed cell for streaming JSON serialization.
struct JsonCell<'a> {
  cell: &'a Cell,
  compact: bool,
}

pub(super) fn serializable_rows(table: &Table, compact: bool) -> Rows<'_> {
  Rows { table, compact }
}

pub(super) fn serializable_row<'a>(headers: &'a [String], row: &'a [Cell], compact: bool) -> Row<'a> {
  Row { headers, row, compact }
}

impl Serialize for Rows<'_> {
  fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
  where
    S: Serializer,
  {
    let mut seq = serializer.serialize_seq(Some(self.table.rows.len()))?;
    for row in &self.table.rows {
      seq.serialize_element(&serializable_row(&self.table.headers, row, self.compact))?;
    }
    seq.end()
  }
}

impl Serialize for Row<'_> {
  fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
  where
    S: Serializer,
  {
    let mut map = serializer.serialize_map(Some(row_len(self.headers, self.row, self.compact)))?;
    for (index, header) in self.headers.iter().enumerate() {
      let null = Cell::Null;
      let cell = self.row.get(index).unwrap_or(&null);
      if self.compact && matches!(cell, Cell::Null) {
        continue;
      }
      map.serialize_entry(header, &JsonCell { cell, compact: self.compact })?;
    }
    map.end()
  }
}

impl Serialize for JsonCell<'_> {
  fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
  where
    S: Serializer,
  {
    match self.cell {
      Cell::Null => serializer.serialize_none(),
      Cell::Bool(value) => serializer.serialize_bool(*value),
      Cell::Int(value) => serializer.serialize_i64(*value),
      Cell::Float(value) => {
        if value.is_finite() {
          serializer.serialize_f64(*value)
        } else {
          serializer.serialize_none()
        }
      }
      Cell::Text(value) => serializer.serialize_str(value),
      Cell::Json(value) if self.compact => {
        let mut value = value.clone();
        compact_json(&mut value);
        value.serialize(serializer)
      }
      Cell::Json(value) => value.serialize(serializer),
    }
  }
}

fn row_len(headers: &[String], row: &[Cell], compact: bool) -> usize {
  if compact {
    headers
      .iter()
      .enumerate()
      .filter(|(index, _)| !row.get(*index).is_none_or(|cell| matches!(cell, Cell::Null)))
      .count()
  } else {
    headers.len()
  }
}

//
// helpers
//

// Remove null-valued object fields while preserving array shape.
fn compact_json(value: &mut Value) {
  match value {
    Value::Array(values) => {
      for value in values {
        compact_json(value);
      }
    }
    Value::Object(values) => {
      values.retain(|_, value| {
        compact_json(value);
        !value.is_null()
      });
    }
    _ => {}
  }
}
