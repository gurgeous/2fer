//! Input format detection. Has two entrypoints, detect_by_sample and
//! detect_by_path. They both rely on the format registry to do the hard work.

use std::path::Path;

use super::registry;
use crate::{
  error::{Error, Result},
  formats::{self, Format},
  mishap,
  util::{self, SNIFF_BYTES},
};

//
// entry points
//

//
// Detect by examining a small prefix sample from the start of a file (<=4k).
// The specific formats rely on magic numbers, heuristics, etc. The list of
// formats we check is carefully ordered. Exact binary formats come first. JSON
// stays before YAML because YAML is broader. CSV/TSV stay fallbacks because
// they do not claim samples.
//

pub fn detect_by_sample(sample: &[u8]) -> Option<&'static dyn Format> {
  let sample = util::strip_utf8_bom(sample);
  registry::all().find(|format| format.detect_sample(sample))
}

//
// Detect by examining the path AND file contents. Goes like this:
//
// 1. try the path ext, if any
// 2. examine the first 4k of the input
// 3. examine the WHOLE FILE, which we need for things like xslx
// 4. before we fallback to csv, make sure this isn't a PDF or something weird
// 5. fallback to csv
//

pub fn detect_by_path(path: &Path) -> Result<&'static dyn Format> {
  // 1. ext?
  let ext = util::path_ext(path);
  if let Some(format) = formats::find(ext) {
    util::log_2fer(format_args!("  detect ext={ext} format={}", format.name()));
    return Ok(format);
  }

  // 2. sample?
  let sample = util::read_prefix(path, SNIFF_BYTES)?;
  if let Some(format) = detect_by_sample(&sample) {
    util::log_2fer(format_args!("  detect sample format={}", format.name()));
    return Ok(format);
  }

  // 3. whole file? xlsx is the only one that needs this today.
  if let Some(format) = registry::all().find(|format| format.detect_path(path)) {
    util::log_2fer(format_args!("  detect path format={}", format.name()));
    util::log_2fer("  detect mishap=false");
    return Ok(format);
  }

  // 4. Hm. We are about to fallback to CSV. Rule out some well known file types
  // that are easily detect but unsupported. We call these "mishaps"
  if let Some(kind) = mishap::mishap(&sample) {
    util::log_2fer(format_args!("  detect mishap={kind}"));
    return Err(Error::UnsupportedInputMishap(kind));
  }

  // 5. fallback to csv
  util::log_2fer("  detect mishap=false");
  util::log_2fer("  detect fallback format=csv");
  Ok(registry::find("csv").expect("csv format should be registered"))
}

#[cfg(test)]
mod tests {
  use std::{
    fs,
    io::{Cursor, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
  };

  use super::*;

  fn tmp_path(name: &str) -> PathBuf {
    std::env::temp_dir()
      .join(format!("2fer-detect-{name}-{}", SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()))
  }

  #[test]
  fn test_detect_by_sample() {
    for (bytes, expected) in [
      (&br#"[{"a":1}]"#[..], Some("json")),
      (&b"{\"a\":1}\n{\"a\":2}\n"[..], Some("jsonl")),
      (&b"- a: 1\n- a: 2\n"[..], Some("yml")),
      (&b"\n# comment\n---\n- a: 1\n- a: 2\n"[..], Some("yml")),
      (&b"| a | b |\n| --- | --- |\n| 1 | 2 |\n"[..], Some("md")),
      (&b"SQLite format 3\0rest"[..], Some("sqlite")),
      (&[0xd0, 0xcf, 0x11, 0xe0, 0x00][..], None),
      (&b"a,b\n1,2\n"[..], None),
      (&b"PK\x03\x04rest"[..], None),
      (&b"{\"a\":1}\n{\"a\":"[..], Some("jsonl")),
    ] {
      assert_eq!(expected, detect_by_sample(bytes).map(|format| format.name()), "{:?}", String::from_utf8_lossy(bytes));
    }
  }

  #[test]
  fn test_detect_by_path() {
    for (name, bytes, expected) in [
      ("json", &br#"[{"a":1}]"#[..], "json"),
      ("jsonl", &b"{\"a\":1}\n{\"a\":2}\n"[..], "jsonl"),
      ("yml", &b"- a: 1\n- a: 2\n"[..], "yml"),
      ("md", &b"| a | b |\n| --- | --- |\n| 1 | 2 |\n"[..], "md"),
      ("csv", &b"a\tb\n1\t2\n3\t4\n5\t6\n"[..], "csv"),
    ] {
      let path = tmp_path(name);
      fs::write(&path, bytes).unwrap();

      assert_eq!(expected, detect_by_path(&path).unwrap().name(), "{name}");

      fs::remove_file(path).unwrap();
    }
  }

  #[test]
  fn test_detect_by_path_ext_keeps_jsonl() {
    let path = tmp_path("input").with_extension("jsonl");
    fs::write(&path, b"{\"a\":1}\n{\"a\":2}\n").unwrap();

    assert_eq!("jsonl", detect_by_path(&path).unwrap().name());

    fs::remove_file(path).unwrap();
  }

  #[test]
  fn test_detect_by_path_xls_ext_does_not_reject_text() {
    let path = tmp_path("input").with_extension("xls");
    fs::write(&path, b"a,b\n1,2\n").unwrap();

    assert_eq!("csv", detect_by_path(&path).unwrap().name());

    fs::remove_file(path).unwrap();
  }

  #[test]
  fn test_detect_input_fixture_corpus() {
    for (path, expected) in [
      ("tests/test.csv", "csv"),
      ("tests/test.tsv", "tsv"),
      ("tests/test.json", "json"),
      ("tests/test.jsonl", "jsonl"),
      ("tests/test.yml", "yml"),
      ("tests/test.md", "md"),
      ("tests/test.sqlite", "sqlite"),
      ("tests/test.xlsx", "xlsx"),
    ] {
      assert_eq!(expected, detect_by_path(Path::new(path)).unwrap().name(), "{path}");
    }

    let extensionless_xlsx = tmp_path("fixture-xlsx");
    fs::copy("tests/test.xlsx", &extensionless_xlsx).unwrap();

    assert_eq!("xlsx", detect_by_path(&extensionless_xlsx).unwrap().name());

    fs::remove_file(extensionless_xlsx).unwrap();
  }

  #[test]
  fn test_plain_zip_is_rejected() {
    let mut bytes = Cursor::new(Vec::new());
    {
      let mut writer = zip::ZipWriter::new(&mut bytes);
      writer.start_file("[Content_Types].xml", zip::write::SimpleFileOptions::default()).unwrap();
      writer.write_all(b"ok").unwrap();
      writer.finish().unwrap();
    }
    let path = tmp_path("plain-zip");
    fs::write(&path, bytes.into_inner()).unwrap();

    assert!(matches!(detect_by_path(&path), Err(Error::UnsupportedInputMishap("zip"))));

    fs::remove_file(path).unwrap();
  }

  #[test]
  fn test_detect_by_path_rejects_mishaps() {
    for (bytes, expected) in [
      (&[0xd0, 0xcf, 0x11, 0xe0][..], "xls"),
      (&b"PAR1data"[..], "parquet"),
      (&b"%PDF-1.7"[..], "pdf"),
      (&b"\x89PNG\r\n\x1a\n"[..], "png"),
      (&b"\n<HTML>"[..], "html"),
      (&b"\0"[..], "binary"),
    ] {
      let path = tmp_path("mishap");
      fs::write(&path, bytes).unwrap();

      assert!(
        matches!(detect_by_path(&path), Err(Error::UnsupportedInputMishap(kind)) if kind == expected),
        "{path:?}"
      );

      fs::remove_file(path).unwrap();
    }
  }

  #[test]
  fn test_detect_mishap_fixture_corpus() {
    for (path, expected) in [
      ("tests/mishaps/arrow.arrow", "arrow"),
      ("tests/mishaps/avi.avi", "avi"),
      ("tests/mishaps/avro.avro", "avro"),
      ("tests/mishaps/null-byte.bin", "binary"),
      ("tests/mishaps/bzip2.bz2", "bzip2"),
      ("tests/mishaps/gif.gif", "gif"),
      ("tests/mishaps/gzip.gz", "gzip"),
      ("tests/mishaps/hdf5.h5", "hdf5"),
      ("tests/mishaps/html.html", "html"),
      ("tests/mishaps/jpeg.jpg", "jpeg"),
      ("tests/mishaps/parquet.parquet", "parquet"),
      ("tests/mishaps/pdf.pdf", "pdf"),
      ("tests/mishaps/png.png", "png"),
      ("tests/mishaps/riff.wav", "wav"),
      ("tests/mishaps/webp.webp", "webp"),
      ("tests/mishaps/xls.xls", "xls"),
      ("tests/mishaps/xml.xml", "xml"),
      ("tests/mishaps/xz.xz", "xz"),
      ("tests/mishaps/zip.zip", "zip"),
      ("tests/mishaps/zstd.zst", "zstd"),
    ] {
      assert!(
        matches!(detect_by_path(Path::new(path)), Err(Error::UnsupportedInputMishap(kind)) if kind == expected),
        "{path}"
      );
    }
  }
}
