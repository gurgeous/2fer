//! Simple heuristics for detecting "mishaps", which are well-known file types
//! that are not supported. For example, PNG. CSV is our fallback, but we don't
//! want to go there if the input file happens to be a PNG or whatever.

use crate::util;

const MISHAP_MAGIC: &[(&[u8], &str)] = &[
  (&[0x28, 0xb5, 0x2f, 0xfd], "zstd"),
  (&[0xd0, 0xcf, 0x11, 0xe0], "xls"),
  (b"%PDF-", "pdf"),
  (b"\x1f\x8b", "gzip"),
  (b"\x89HDF\r\n\x1a\n", "hdf5"),
  (b"\x89PNG\r\n\x1a\n", "png"),
  (b"\xfd7zXZ\0", "xz"),
  (b"\xff\xd8\xff", "jpeg"),
  (b"ARROW1", "arrow"),
  (b"BZh", "bzip2"),
  (b"FEA1", "arrow"),
  (b"GIF87a", "gif"),
  (b"GIF89a", "gif"),
  (b"Obj\x01", "avro"),
  (b"PAR1", "parquet"),
  (b"PK\x03\x04", "zip"),
  (b"PK\x05\x06", "zip"),
  (b"PK\x07\x08", "zip"),
];

const MISHAP_TEXT: &[(&str, &str)] = &[("<!doctype html", "html"), ("<?xml", "xml"), ("<html", "html")];

pub(crate) fn mishap(bytes: &[u8]) -> Option<&'static str> {
  for &(magic, kind) in MISHAP_MAGIC {
    if bytes.starts_with(magic) {
      return Some(kind);
    }
  }
  if is_riff(bytes) {
    return Some("riff");
  }
  if bytes.contains(&0) {
    return Some("binary");
  }

  let text = String::from_utf8_lossy(util::strip_utf8_bom(bytes).trim_ascii_start());
  MISHAP_TEXT.iter().find_map(|(prefix, kind)| util::starts_with_ignore_ascii_case(&text, prefix).then_some(*kind))
}

// "riff" container fro AVI, WAV, WEBP
fn is_riff(bytes: &[u8]) -> bool {
  bytes.starts_with(b"RIFF") && bytes.len() >= 12 && matches!(&bytes[8..12], b"AVI " | b"WAVE" | b"WEBP")
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_mishap() {
    for (bytes, expected) in [
      (&[0x28, 0xb5, 0x2f, 0xfd][..], Some("zstd")),
      (&[0xd0, 0xcf, 0x11, 0xe0][..], Some("xls")),
      (&b"  <HTML>"[..], Some("html")),
      (&b"%PDF-1.7"[..], Some("pdf")),
      (&b"\n<?xml version=\"1.0\"?>"[..], Some("xml")),
      (&b"\x1f\x8brest"[..], Some("gzip")),
      (&b"\x89HDF\r\n\x1a\n"[..], Some("hdf5")),
      (&b"\x89PNG\r\n\x1a\n"[..], Some("png")),
      (&b"\xef\xbb\xbf  <!DOCTYPE html>"[..], Some("html")),
      (&b"\xfd7zXZ\0rest"[..], Some("xz")),
      (&b"\xff\xd8\xff"[..], Some("jpeg")),
      (&b"a\0b"[..], Some("binary")),
      (&b"ARROW1"[..], Some("arrow")),
      (&b"BZhrest"[..], Some("bzip2")),
      (&b"FEA1"[..], Some("arrow")),
      (&b"GIF87a"[..], Some("gif")),
      (&b"GIF89a"[..], Some("gif")),
      (&b"Obj\x01data"[..], Some("avro")),
      (&b"PAR1data"[..], Some("parquet")),
      (&b"PK\x03\x04rest"[..], Some("zip")),
      (&b"RIFFxxxxWEBPrest"[..], Some("riff")),
      (&b"HTTP/1.1 404 Not Found"[..], None),
      (&b"a,b\n1,2\n"[..], None),
      (&b"\x01a,b\n1,2\n"[..], None),
    ] {
      assert_eq!(expected, mishap(bytes), "{bytes:?}");
    }
  }
}
