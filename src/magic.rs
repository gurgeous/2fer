//! File signature matching.

const MAGIC: &[(&[u8], &str)] = &[
  (b"SQLite format 3", "sqlite"),
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

/// Return the known file type implied by a byte signature.
pub(crate) fn find_magic(bytes: &[u8]) -> Option<&'static str> {
  for (magic, filetype) in MAGIC {
    if bytes.starts_with(magic) {
      return Some(filetype);
    }
  }
  match_riff(bytes)
}

fn match_riff(bytes: &[u8]) -> Option<&'static str> {
  if !bytes.starts_with(b"RIFF") || bytes.len() < 12 {
    return None;
  }
  match &bytes[8..12] {
    b"AVI " => Some("avi"),
    b"WAVE" => Some("wav"),
    b"WEBP" => Some("webp"),
    _ => None,
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_match_magic() {
    for (bytes, expected) in [
      // yes
      (&[0x28, 0xb5, 0x2f, 0xfd][..], Some("zstd")),
      (&[0xd0, 0xcf, 0x11, 0xe0][..], Some("xls")),
      (&b"%PDF-1.7"[..], Some("pdf")),
      (&b"\x1f\x8brest"[..], Some("gzip")),
      (&b"\x89HDF\r\n\x1a\n"[..], Some("hdf5")),
      (&b"\x89PNG\r\n\x1a\n"[..], Some("png")),
      (&b"\xfd7zXZ\0rest"[..], Some("xz")),
      (&b"\xff\xd8\xff"[..], Some("jpeg")),
      (&b"ARROW1"[..], Some("arrow")),
      (&b"BZhrest"[..], Some("bzip2")),
      (&b"FEA1"[..], Some("arrow")),
      (&b"GIF87a"[..], Some("gif")),
      (&b"GIF89a"[..], Some("gif")),
      (&b"Obj\x01data"[..], Some("avro")),
      (&b"PAR1data"[..], Some("parquet")),
      (&b"PK\x03\x04rest"[..], Some("zip")),
      (&b"RIFFxxxxAVI rest"[..], Some("avi")),
      (&b"RIFFxxxxWAVErest"[..], Some("wav")),
      (&b"RIFFxxxxWEBPrest"[..], Some("webp")),
      (&b"SQLite format 3\0rest"[..], Some("sqlite")),
      // no
      (&b"a,b\n1,2\n"[..], None),
    ] {
      assert_eq!(expected, find_magic(bytes), "{bytes:?}");
    }
  }
}
