//! Simple heuristics for detecting "mishaps", which are well-known file types
//! that are not supported. For example, PNG. CSV is our fallback, but we don't
//! want to go there if the input file happens to be a PNG or whatever.

use crate::{magic, util};

const MISHAP_TEXT: &[(&str, &str)] = &[("<!doctype html", "html"), ("<?xml", "xml"), ("<html", "html")];

pub(crate) fn mishap(bytes: &[u8]) -> Option<&'static str> {
  if let Some(kind) = magic::find_magic(bytes) {
    return if kind == "sqlite" { None } else { Some(kind) };
  }

  if bytes.contains(&0) {
    return Some("binary");
  }

  let text = String::from_utf8_lossy(util::strip_utf8_bom(bytes).trim_ascii_start());
  MISHAP_TEXT.iter().find_map(|(prefix, kind)| util::starts_with_ignore_ascii_case(&text, prefix).then_some(*kind))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_mishap() {
    for (bytes, expected) in [
      (&b"%PDF-1.7"[..], Some("pdf")),
      (&b"SQLite format 3\0rest"[..], None),
      (&b"a\0b"[..], Some("binary")),
      (&b"  <HTML>"[..], Some("html")),
      (&b"\n<?xml version=\"1.0\"?>"[..], Some("xml")),
      (&b"\xef\xbb\xbf  <!DOCTYPE html>"[..], Some("html")),
      (&b"HTTP/1.1 404 Not Found"[..], None),
      (&b"a,b\n1,2\n"[..], None),
      (&b"\x01a,b\n1,2\n"[..], None),
    ] {
      assert_eq!(expected, mishap(bytes), "{bytes:?}");
    }
  }
}
