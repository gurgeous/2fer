//! CSV delimiter sniffing.
//!
//! It is a simple heuristic. Look at sample, reject tiny/blank/trailing lines,
//! then test candidates comma, tab, semicolon, and pipe. For each delim, count
//! columns on every row. Delim wins when all rows have the same column count
//! and at least two columns. If several delims win, the one with the most
//! columns wins.

const CANDIDATES: &[u8] = b",\t;|";

/// Guess a delimiter from consistently shaped sample rows.
pub fn sniff(sample: &str) -> Option<u8> {
  let lines = split_lines(sample);
  if lines.len() < 3 || lines.iter().any(|line| line.is_empty()) {
    return None;
  }

  let mut best_count = 0;
  let mut best_delimiter = None;
  for &delimiter in CANDIDATES {
    let count = count_columns(&lines, delimiter);
    if count > best_count {
      best_count = count;
      best_delimiter = Some(delimiter);
    }
  }
  best_delimiter
}

//
// helpers
//

// Drop the final line because streaming samples may end mid-row.
fn split_lines(sample: &str) -> Vec<&str> {
  let mut lines: Vec<_> = sample.split('\n').take(11).map(|line| line.strip_suffix('\r').unwrap_or(line)).collect();
  lines.pop();
  lines
}

fn count_columns(lines: &[&str], delimiter: u8) -> usize {
  let mut expected = 0;
  for line in lines {
    let n = count_columns_for_line(line, delimiter);
    if expected == 0 {
      expected = n;
    }
    if n != expected {
      return 0;
    }
  }
  if expected < 2 { 0 } else { expected }
}

fn count_columns_for_line(line: &str, delimiter: u8) -> usize {
  let mut n = 1;
  let mut in_quotes = false;
  let mut iter = line.as_bytes().iter().peekable();
  while let Some(&ch) = iter.next() {
    if ch == b'"' {
      if in_quotes && iter.peek() == Some(&&b'"') {
        iter.next();
      } else {
        in_quotes = !in_quotes;
      }
    } else if !in_quotes && ch == delimiter {
      n += 1;
    }
  }
  n
}

//
// tests
//

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_sniff() {
    assert_eq!(Some(b','), sniff("a,b,c\n1,2,3\n4,5,6\n7,8"));
    assert_eq!(Some(b';'), sniff("a;b;c\n1;2;3\n4;5;6\n7;8"));
    assert_eq!(Some(b'\t'), sniff("a\tb\tc\n1\t2\t3\n4\t5\t6\n7\t8"));
    assert_eq!(Some(b'|'), sniff("a|b|c\n1|2|3\n4|5|6\n7|8"));
    assert_eq!(None, sniff("a,b\n1,2\n"));
    assert_eq!(None, sniff("a,b,c\n1,2\n3,4,5\n"));
  }
}
