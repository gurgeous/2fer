//! Shared helpers. Note to LLMs - each one should have a test.

use std::{
  fmt,
  fs::File,
  io::{self, Read, Write},
  path::Path,
  time::Duration,
};

use tempfile::NamedTempFile;

use crate::error::{Error, Result};

//
// debug logging
//

/// Print a debug line when TWOFER_DEBUG is enabled.
pub(crate) fn log_2fer(message: impl fmt::Display) {
  if std::env::var_os("TWOFER_DEBUG").is_some() {
    let _ = writeln!(io::stderr(), "2fer: {message}");
  }
}

//
// string helpers
//

pub(crate) fn format_elapsed(duration: Duration) -> String {
  let ms = duration.as_secs_f64() * 1000.0;
  if ms < 10.0 { format!("{ms:.1}ms") } else { format!("{ms:.0}ms") }
}

/// Return a quoted, escaped display string for one byte.
pub(crate) fn inspect_byte(byte: u8) -> String {
  match byte {
    b'\t' => r#""\t""#.to_owned(),
    b'\n' => r#""\n""#.to_owned(),
    b'\r' => r#""\r""#.to_owned(),
    b'"' => r#""\"""#.to_owned(),
    b'\\' => r#""\\""#.to_owned(),
    b' '..=b'~' => format!(r#""{}""#, byte as char),
    _ => format!(r#""\x{byte:02x}""#),
  }
}

/// Case-insensitive ASCII version of `str::starts_with`.
#[allow(dead_code)]
pub(crate) fn starts_with_ignore_ascii_case(value: &str, prefix: &str) -> bool {
  value.get(..prefix.len()).is_some_and(|head| head.eq_ignore_ascii_case(prefix))
}

/// Case-insensitive ASCII version of `str::ends_with`.
#[allow(dead_code)]
pub(crate) fn ends_with_ignore_ascii_case(value: &str, suffix: &str) -> bool {
  let Some(split) = value.len().checked_sub(suffix.len()) else {
    return false;
  };
  value.get(split..).is_some_and(|tail| tail.eq_ignore_ascii_case(suffix))
}

/// Case-insensitive ASCII version of `str::strip_prefix`.
#[allow(dead_code)]
pub(crate) fn strip_prefix_ignore_ascii_case<'a>(value: &'a str, prefix: &str) -> Option<&'a str> {
  if starts_with_ignore_ascii_case(value, prefix) { value.get(prefix.len()..) } else { None }
}

/// Case-insensitive ASCII version of `str::strip_suffix`.
#[allow(dead_code)]
pub(crate) fn strip_suffix_ignore_ascii_case<'a>(value: &'a str, suffix: &str) -> Option<&'a str> {
  let split = value.len().checked_sub(suffix.len())?;
  if ends_with_ignore_ascii_case(value, suffix) { value.get(..split) } else { None }
}

//
// ReplayReader
//

/// Read wrapper that replays peeked bytes before continuing the stream.
#[derive(Debug)]
pub(crate) struct ReplayReader<R> {
  inner: R,
  buf: Vec<u8>,
  pos: usize,
}

impl<R: Read> ReplayReader<R> {
  /// Build a reader with an initially empty replay buffer.
  pub(crate) fn new(inner: R) -> Self {
    Self { inner, buf: Vec::new(), pos: 0 }
  }

  /// Fill and return up to `limit` bytes without advancing the read position.
  pub(crate) fn peek(&mut self, limit: usize) -> io::Result<&[u8]> {
    while self.buf.len() < limit {
      let len = limit - self.buf.len();
      let mut chunk = vec![0; len];
      let n = self.inner.read(&mut chunk)?;
      if n == 0 {
        break;
      }
      self.buf.extend_from_slice(&chunk[..n]);
    }
    Ok(&self.buf)
  }
}

impl<R: Read> Read for ReplayReader<R> {
  fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
    if self.pos < self.buf.len() {
      let n = (self.buf.len() - self.pos).min(buf.len());
      buf[..n].copy_from_slice(&self.buf[self.pos..self.pos + n]);
      self.pos += n;
      return Ok(n);
    }

    self.inner.read(buf)
  }
}

//
// file/io helpers
//

/// Number of leading bytes used for cheap content sniffing.
pub(crate) const SNIFF_BYTES: usize = 4096;

/// Return a path's UTF-8 extension, or empty string when it has none.
pub(crate) fn path_ext(path: &Path) -> &str {
  path.extension().and_then(|ext| ext.to_str()).unwrap_or("")
}

/// Read up to `limit` bytes from a file for cheap content sniffing.
pub(crate) fn read_prefix(path: &Path, limit: usize) -> Result<Vec<u8>> {
  let mut file = File::open(path).map_err(|error| Error::FileRead { path: path.to_owned(), error })?;
  let mut bytes = vec![0; limit];
  let read = file.read(&mut bytes).map_err(|error| Error::FileRead { path: path.to_owned(), error })?;
  bytes.truncate(read);
  Ok(bytes)
}

/// Create a named temp file that deletes itself on drop.
pub(crate) fn temp_file(label: &str, ext: &str) -> Result<NamedTempFile> {
  let suffix = if ext.is_empty() { String::new() } else { format!(".{ext}") };
  tempfile::Builder::new()
    .prefix(&format!("2fer-{label}-"))
    .suffix(&suffix)
    .tempfile()
    .map_err(|error| Error::WriteFile { path: std::env::temp_dir(), error })
}

/// Remove a UTF-8 BOM when present.
pub(crate) fn strip_utf8_bom(bytes: &[u8]) -> &[u8] {
  bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(bytes)
}

//
// tests
//

#[cfg(test)]
mod tests {
  use std::{
    fs,
    io::{Cursor, Read},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
  };

  use super::*;

  fn tmp_path(name: &str) -> PathBuf {
    std::env::temp_dir()
      .join(format!("2fer-util-{name}-{}", SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()))
  }

  #[test]
  fn test_peek() {
    for (limit, expected) in [(0, &b""[..]), (3, &b"abc"[..]), (10, &b"abcdef"[..])] {
      let mut reader = ReplayReader::new(Cursor::new(b"abcdef"));

      assert_eq!(expected, reader.peek(limit).unwrap(), "limit={limit}");
    }
  }

  #[test]
  fn test_read_replays_peeked_bytes() {
    let mut reader = ReplayReader::new(Cursor::new(b"abcdef"));

    assert_eq!(b"abc", reader.peek(3).unwrap());

    let mut out = Vec::new();
    reader.read_to_end(&mut out).unwrap();
    assert_eq!(b"abcdef", out.as_slice());
  }

  #[test]
  fn test_read_after_partial_replay() {
    let mut reader = ReplayReader::new(Cursor::new(b"abcdef"));
    let mut first = [0; 2];
    let mut rest = Vec::new();

    reader.peek(4).unwrap();
    reader.read_exact(&mut first).unwrap();
    reader.read_to_end(&mut rest).unwrap();

    assert_eq!(b"ab", &first);
    assert_eq!(b"cdef", rest.as_slice());
  }

  #[test]
  fn test_path_ext() {
    for (path, expected) in [("data.csv", "csv"), ("DATA.JSON", "JSON"), ("archive.tar.gz", "gz"), ("README", "")] {
      assert_eq!(expected, path_ext(Path::new(path)), "{path}");
    }
  }

  #[test]
  fn test_format_elapsed() {
    assert_eq!("1.5ms", format_elapsed(Duration::from_micros(1500)));
    assert_eq!("12ms", format_elapsed(Duration::from_millis(12)));
  }

  #[test]
  fn test_inspect_byte() {
    for (byte, expected) in [
      (b',', r#"",""#),
      (b'\t', r#""\t""#),
      (b' ', r#"" ""#),
      (b'"', r#""\"""#),
      (b'\\', r#""\\""#),
      (b':', r#"":""#),
      (0x1f, r#""\x1f""#),
    ] {
      assert_eq!(expected, inspect_byte(byte), "byte={byte}");
    }
  }

  #[test]
  fn test_starts_with_ignore_ascii_case() {
    for (value, prefix, expected) in [
      ("2FER", "2f", true),
      ("hello", "HELLO", true),
      ("hello", "hellos", false),
      ("écho", "É", false),
      ("écho", "é", true),
    ] {
      assert_eq!(expected, starts_with_ignore_ascii_case(value, prefix), "{value:?} {prefix:?}");
    }
  }

  #[test]
  fn test_ends_with_ignore_ascii_case() {
    for (value, suffix, expected) in [
      ("2csv.EXE", ".exe", true),
      ("hello", "ELLO", true),
      ("hello", "xhello", false),
      ("café", "FÉ", false),
      ("café", "fé", true),
    ] {
      assert_eq!(expected, ends_with_ignore_ascii_case(value, suffix), "{value:?} {suffix:?}");
    }
  }

  #[test]
  fn test_strip_prefix_ignore_ascii_case() {
    for (value, prefix, expected) in
      [("2JSON", "2j", Some("SON")), ("hello", "HEL", Some("lo")), ("hello", "x", None), ("écho", "é", Some("cho"))]
    {
      assert_eq!(expected, strip_prefix_ignore_ascii_case(value, prefix), "{value:?} {prefix:?}");
    }
  }

  #[test]
  fn test_strip_suffix_ignore_ascii_case() {
    for (value, suffix, expected) in [
      ("2json.EXE", ".exe", Some("2json")),
      ("hello", "LLO", Some("he")),
      ("hello", "x", None),
      ("café", "fé", Some("ca")),
    ] {
      assert_eq!(expected, strip_suffix_ignore_ascii_case(value, suffix), "{value:?} {suffix:?}");
    }
  }

  #[test]
  fn test_read_prefix() {
    let path = tmp_path("prefix");
    fs::write(&path, b"abcdef").unwrap();

    assert_eq!(b"abc", read_prefix(&path, 3).unwrap().as_slice());
    assert_eq!(b"abcdef", read_prefix(&path, 99).unwrap().as_slice());
    assert_eq!(b"", read_prefix(&path, 0).unwrap().as_slice());

    fs::remove_file(path).unwrap();
  }

  #[test]
  fn test_read_prefix_missing_file() {
    let path = tmp_path("missing");

    assert!(matches!(read_prefix(&path, 3), Err(Error::FileRead { .. })));
  }

  #[test]
  fn test_temp_file() {
    let path = {
      let file = temp_file("util", "tmp").unwrap();
      let path = file.path().to_owned();

      assert!(path.exists());
      assert_eq!("tmp", path_ext(&path));

      path
    };

    assert!(!path.exists());
  }

  #[test]
  fn test_strip_utf8_bom() {
    for (bytes, expected) in [(&b"\xef\xbb\xbfabc"[..], &b"abc"[..]), (&b"abc"[..], &b"abc"[..]), (&b""[..], &b""[..])]
    {
      assert_eq!(expected, strip_utf8_bom(bytes));
    }
  }
}
