//! YML format support.

use serde_json::{Map, Number, Value as JsonValue};
use serde_yaml::Value as YamlValue;

use super::{Format, json_value};
use crate::{
  app::App,
  cell::Cell,
  error::{Error, Result},
  table::Table,
};

//
// format
//

/// YML format entry.
#[derive(Debug)]
pub(super) struct Yml;

impl Format for Yml {
  fn exts(&self) -> &'static [&'static str] {
    &["yml", "yaml"]
  }

  fn detect_sample(&self, bytes: &[u8]) -> bool {
    is_yml(bytes)
  }

  fn read_from_bytes(&self, _app: &App, bytes: &[u8]) -> Result<Table> {
    read_yml(bytes)
  }

  fn write_to_bytes(&self, app: &App, table: &Table) -> Result<Vec<u8>> {
    let mut text = serde_yaml::to_string(&json_value::serializable_rows(table, app.args.compact))?;
    if !text.ends_with('\n') {
      text.push('\n');
    }
    Ok(text.into_bytes())
  }
}

//
// helpers
//

// Skip blank lines, comments, and `---` before checking for sequence mappings.
pub fn is_yml(bytes: &[u8]) -> bool {
  let text = String::from_utf8_lossy(bytes);
  let mut lines = yml_content_lines(&text);
  let Some(first) = lines.next() else { return false };
  if let Some(rest) = first.strip_prefix("- ") {
    return looks_like_mapping(rest);
  }
  first == "-" && lines.next().is_some_and(looks_like_mapping)
}

/// Read a YAML sequence of mappings.
fn read_yml(bytes: &[u8]) -> Result<Table> {
  let value: YamlValue = serde_yaml::from_slice(bytes)?;
  let YamlValue::Sequence(rows) = value else {
    return Err(Error::YmlShape("expected a sequence of mappings".to_owned()));
  };

  let mut records = Vec::new();
  for row in rows {
    let YamlValue::Mapping(mapping) = row else {
      return Err(Error::YmlShape("expected every row to be a mapping".to_owned()));
    };
    let mut record = Vec::new();
    for (key, value) in mapping {
      let YamlValue::String(key) = key else {
        return Err(Error::YmlShape("mapping keys must be strings".to_owned()));
      };
      record.push((key, Cell::from_json(yml_to_json(value)?)));
    }
    records.push(record);
  }
  Table::from_records(records)
}

// Normalize YAML values through the JSON-backed cell model.
fn yml_to_json(value: YamlValue) -> Result<JsonValue> {
  Ok(match value {
    YamlValue::Null => JsonValue::Null,
    YamlValue::Bool(value) => JsonValue::Bool(value),
    YamlValue::Number(value) => yml_number(value)?,
    YamlValue::String(value) => JsonValue::String(value),
    YamlValue::Sequence(values) => JsonValue::Array(values.into_iter().map(yml_to_json).collect::<Result<Vec<_>>>()?),
    YamlValue::Mapping(values) => {
      let mut object = Map::new();
      for (key, value) in values {
        let YamlValue::String(key) = key else {
          return Err(Error::YmlShape("nested mapping keys must be strings".to_owned()));
        };
        object.insert(key, yml_to_json(value)?);
      }
      JsonValue::Object(object)
    }
    YamlValue::Tagged(value) => yml_to_json(value.value)?,
  })
}

fn yml_number(value: serde_yaml::Number) -> Result<JsonValue> {
  if let Some(value) = value.as_i64() {
    Ok(JsonValue::Number(Number::from(value)))
  } else if let Some(value) = value.as_f64() {
    Ok(Number::from_f64(value).map(JsonValue::Number).unwrap_or(JsonValue::Null))
  } else {
    Err(Error::YmlShape("unsupported number".to_owned()))
  }
}

fn yml_content_lines(text: &str) -> impl Iterator<Item = &str> {
  text.lines().map(str::trim_start).filter(|line| !line.is_empty() && !line.starts_with('#') && *line != "---")
}

fn looks_like_mapping(line: &str) -> bool {
  let line = line.trim_start();
  if line.starts_with(['[', '{']) {
    return false;
  }
  let Some(index) = line.find(':') else {
    return false;
  };
  index > 0 && line.as_bytes().get(index + 1).is_none_or(u8::is_ascii_whitespace)
}

#[cfg(test)]
mod tests {
  use std::{fs, path::PathBuf};

  use serde_json::json;

  use super::*;
  use crate::cell::Cell;

  fn app(compact: bool) -> App {
    let args = crate::args::Args {
      output: None,
      as_format: Some("yml".to_owned()),
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
  fn test_read_sequence_of_mappings() {
    let table = read_yml(b"- name: alice\n  meta:\n    ok: true\n- score: 2\n").unwrap();
    assert_eq!(["name", "meta", "score"], table.headers.as_slice());
    assert_eq!(Cell::Json(json!({"ok": true})), table.rows[0][1]);
    assert_eq!(Cell::Null, table.rows[1][0]);
  }

  #[test]
  fn test_reject_shapes() {
    assert!(read_yml(b"name: alice\n").is_err());
    assert!(read_yml(b"- [alice]\n").is_err());
  }

  #[test]
  fn test_is_yml_corpus() {
    for (name, expected) in [
      ("ansible-playbook.yml", true),
      ("circleci.yml", false),
      ("docker-compose.yml", false),
      ("docker-role-tasks.yml", true),
      ("github-workflow.yml", false),
      ("gitlab-ci.yml", false),
      ("k8s-deployment.yml", false),
      ("k8s-service.yml", false),
      ("nginx-role-tasks.yml", true),
      ("nodejs-role-tasks.yml", true),
      ("pre-commit.yml", true),
    ] {
      let path = PathBuf::from("tests/yml").join(name);
      let bytes = fs::read(&path).unwrap();
      assert_eq!(expected, is_yml(&bytes), "{name}");
    }
  }

  #[test]
  fn test_write_yml() {
    let table = Table {
      headers: vec!["name".to_owned(), "score".to_owned()],
      rows: vec![vec![Cell::Text("alice".to_owned()), Cell::Int(1)]],
    };
    let out = String::from_utf8(Yml.write_to_bytes(&app(false), &table).unwrap()).unwrap();
    assert_eq!("- name: alice\n  score: 1\n", out);
  }

  #[test]
  fn test_compact() {
    let table = Table {
      headers: vec!["name".to_owned(), "score".to_owned(), "meta".to_owned()],
      rows: vec![vec![Cell::Text("alice".to_owned()), Cell::Null, Cell::Json(json!({"ok": true, "extra": null}))]],
    };
    let yml = String::from_utf8(Yml.write_to_bytes(&app(true), &table).unwrap()).unwrap();
    assert!(!yml.contains("score:"));
    assert!(!yml.contains("extra:"));
    assert!(yml.contains("ok: true"));
  }
}
