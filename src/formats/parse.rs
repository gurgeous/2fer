//! Low-level numeric/bool parsers use for inference

use std::borrow::Cow;

//
// types
//

/// Which numeric shape should be accepted?
#[derive(Clone, Copy)]
enum Numeric {
  Int,
  Float,
}

//
// parsers
//

pub(super) fn parse_bool(value: &str) -> Option<bool> {
  if value.eq_ignore_ascii_case("true") {
    Some(true)
  } else if value.eq_ignore_ascii_case("false") {
    Some(false)
  } else {
    None
  }
}

pub(super) fn parse_float(value: &str) -> Option<f64> {
  let value = parse_numeric(value, Numeric::Float)?.parse::<f64>().ok()?;
  value.is_finite().then_some(value)
}

pub(super) fn parse_int(value: &str) -> Option<i64> {
  parse_numeric(value, Numeric::Int)?.parse().ok()
}

//
// numeric
//

// Validate our small numeric grammar and strip thousands separators.
fn parse_numeric(value: &str, numeric: Numeric) -> Option<Cow<'_, str>> {
  let unsigned = value.strip_prefix('-').unwrap_or(value);
  let whole = match numeric {
    Numeric::Int => unsigned,
    Numeric::Float => {
      let (whole, fraction) = unsigned.split_once('.')?;
      if fraction.is_empty() || !fraction.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
      }
      whole
    }
  };
  if !valid_whole(whole) {
    return None;
  }

  if value.contains(',') { Some(Cow::Owned(value.replace(',', ""))) } else { Some(Cow::Borrowed(value)) }
}

//
// helpers
//

fn valid_whole(value: &str) -> bool {
  if value.contains(',') { valid_grouped_whole(value) } else { valid_plain_whole(value) }
}

fn valid_plain_whole(value: &str) -> bool {
  if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
    return false;
  }
  value == "0" || !value.starts_with('0')
}

fn valid_grouped_whole(value: &str) -> bool {
  let mut groups = value.split(',');
  let Some(first) = groups.next() else {
    return false;
  };
  if first.is_empty() || first.len() > 3 || first.starts_with('0') || !first.bytes().all(|byte| byte.is_ascii_digit()) {
    return false;
  }

  let mut saw_tail = false;
  for group in groups {
    if group.len() != 3 || !group.bytes().all(|byte| byte.is_ascii_digit()) {
      return false;
    }
    saw_tail = true;
  }
  saw_tail
}

//
// tests
//

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_parse_bool() {
    for (input, expected) in
      [("true", Some(true)), ("FALSE", Some(false)), (" true", None), ("false ", None), ("", None)]
    {
      assert_eq!(expected, parse_bool(input), "{input}");
    }
  }

  #[test]
  fn test_parse_float() {
    for (input, expected) in [
      ("1.5", Some(1.5)),
      ("-0.25", Some(-0.25)),
      ("1234.5", Some(1234.5)),
      ("1,234.50", Some(1234.5)),
      ("-1,234.50", Some(-1234.5)),
      ("123,456.7", Some(123456.7)),
      ("1,234,567.89", Some(1234567.89)),
      ("", None),
      ("1", None),
      ("1.", None),
      (".5", None),
      ("007.5", None),
      ("$1.5", None),
      ("1.5%", None),
      (" 1.5", None),
      ("1.0e999", None),
      ("0,000.1", None),
      ("12,34.50", None),
      ("1,23,456.50", None),
      ("1234,567.50", None),
    ] {
      assert_eq!(expected, parse_float(input), "{input}");
    }
  }

  #[test]
  fn test_parse_int() {
    for (input, expected) in [
      ("0", Some(0)),
      ("-12", Some(-12)),
      ("1234", Some(1234)),
      ("1,234", Some(1234)),
      ("-1,234", Some(-1234)),
      ("123,456", Some(123456)),
      ("1,234,567", Some(1234567)),
      ("", None),
      ("007", None),
      ("1.0", None),
      ("$1", None),
      ("1%", None),
      (" 1", None),
      ("0,000", None),
      ("12,34", None),
      ("1,23,456", None),
      ("1234,567", None),
      ("1,", None),
    ] {
      assert_eq!(expected, parse_int(input), "{input}");
    }
  }
}
