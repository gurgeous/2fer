//! Format registry and lookup helpers.

use super::{Format, csv, json, jsonl, md, sqlite, xlsx, yml};

//
// ALL_FORMATS
//
// CSV and TSV need distinct registry entries because users can ask for them by
// --output file.tsv or 2tsv. Semicolon and pipe are supported, but we do not
// expose `2semi`/`2pipe` or matching output extensions.
//

static ALL_FORMATS: [&dyn Format; 8] = [&CSV, &JSON, &JSONL, &MD, &SQLITE, &TSV, &XLSX, &YML];

static CSV: csv::Csv = csv::Csv::new(&["csv"], b',', true);
static JSON: json::Json = json::Json;
static JSONL: jsonl::Jsonl = jsonl::Jsonl;
static MD: md::Md = md::Md;
static SQLITE: sqlite::Sqlite = sqlite::Sqlite;
static TSV: csv::Csv = csv::Csv::new(&["tsv"], b'\t', false);
static XLSX: xlsx::Xlsx = xlsx::Xlsx;
static YML: yml::Yml = yml::Yml;

//
// finders
//

/// Iterate formats in detection/lookup order.
pub(super) fn all() -> impl Iterator<Item = &'static dyn Format> {
  ALL_FORMATS.into_iter()
}

/// Find a format by canonical extension or extension alias.
pub(crate) fn find(key: &str) -> Option<&'static dyn Format> {
  all().find(|format| format.has_ext(key))
}

//
// tests
//

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_has_ext() {
    for (format, ext, expected) in [
      (find("json").unwrap(), "JSON", true),
      (find("jsonl").unwrap(), "ndjson", true),
      (find("yml").unwrap(), "yml", true),
      (find("csv").unwrap(), "txt", false),
    ] {
      assert_eq!(expected, format.has_ext(ext), "{ext}");
    }
  }

  #[test]
  fn test_find() {
    for (key, expected) in [
      ("CSV", "csv"),
      ("ndjson", "jsonl"),
      ("jsonl", "jsonl"),
      ("md", "md"),
      ("markdown", "md"),
      ("db", "sqlite"),
      ("sqlite3", "sqlite"),
      ("yml", "yml"),
      ("yaml", "yml"),
    ] {
      assert_eq!(expected, find(key).unwrap().name(), "{key}");
    }

    assert!(find("").is_none());
  }

  #[test]
  fn test_find_alias() {
    for (name, expected) in [
      ("json", "json"),
      ("ndjson", "jsonl"),
      ("markdown", "md"),
      ("md", "md"),
      ("db", "sqlite"),
      ("sqlite", "sqlite"),
      ("yml", "yml"),
    ] {
      assert_eq!(expected, find(name).unwrap().name(), "{name}");
    }
  }
}
