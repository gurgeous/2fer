//! Cli args for Clap

use std::{ffi::OsString, path::PathBuf};

use clap::{CommandFactory, Error as ClapError, Parser, ValueEnum};

use crate::error::{Error, Result};

//
// args
//

#[derive(Debug, Parser)]
#[command(name = "2fer", disable_help_flag = true, disable_version_flag = true)]
pub struct Args {
  /// Set the input delimiter when reading CSVs
  #[arg(help_heading = INPUT, short = 'd', long = "delim", alias = "delimiter", value_name = "char", value_parser = parse_delim)]
  pub delim: Option<u8>,

  /// Pick the sqlite table for read or write
  #[arg(help_heading = INPUT, long, value_name = "name")]
  pub table: Option<String>,

  /// Disable numeric formatting (for csv => json and similar)
  #[arg(help_heading = INPUT, long)]
  pub vanilla: bool,

  /// Write output to this file, otherwise we use stdout
  #[arg(help_heading = OUTPUT, short = 'o', long, value_name = "file")]
  pub output: Option<PathBuf>,

  /// Select output format
  #[arg(help_heading = OUTPUT, long = "as", value_name = "format")]
  pub as_format: Option<String>,

  /// Leave null fields out of json, jsonl, and yml
  #[arg(help_heading = OUTPUT, long)]
  pub compact: bool,

  /// Print shell completion (bash|zsh)
  #[arg(help_heading = OTHER, hide = true, long, value_name = "shell")]
  pub completion: Option<CompletionShell>,

  /// Get help
  #[arg(help_heading = OTHER, short = 'h', long)]
  pub help: bool,

  /// Show version number and exit
  #[arg(help_heading = OTHER, short = 'v', long)]
  pub version: bool,

  /// Read from this file, otherwise use stdin
  #[arg(value_name = "file")]
  pub input: Option<PathBuf>,

  /// True when any argv was provided, not just the binary name.
  #[arg(skip)]
  pub argv_had_args: bool,
}

//
// help text
//

const INPUT: &str = "Input";
const OTHER: &str = "Other";
const OUTPUT: &str = "Output";

pub fn help(symlink: &str) -> String {
  // note: help.txt is our about text
  let mut cmd = Args::command()
    .about(include_str!("help.txt"))
    .name(symlink.to_owned())
    .bin_name(symlink.to_owned())
    .hide_possible_values(true)
    .help_template("{usage-heading} {usage}\n\n{about}\n\n{all-args}")
    .max_term_width(80);

  let help = cmd.render_help().to_string();
  let hint = if symlink == "2fer" {
    String::new()
  } else {
    let fmt = symlink.strip_prefix('2').unwrap_or(symlink);
    format!("\n\nNote: You ran `{symlink}`, which converts the input to {fmt}.")
  };
  help.replace("HINT", &hint)
}

//
// parse cli args with Clap
//

pub fn parse_from<I, T>(args: I) -> Result<Args>
where
  I: IntoIterator<Item = T>,
  T: Into<OsString>,
{
  fn fmt_err(error: ClapError) -> Error {
    let s = error.to_string().lines().next().unwrap_or("invalid arguments").trim_start_matches("error: ").to_owned();
    Error::Usage(s)
  }

  let args = args.into_iter().map(Into::into).collect::<Vec<_>>();
  let argv_had_args = args.len() > 1;
  let mut args = Args::try_parse_from(args).map_err(fmt_err)?;
  args.argv_had_args = argv_had_args;
  Ok(args)
}

//
// completion
//

//
// helpers
//

/// Shells supported by --completion.
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
#[value(rename_all = "kebab-case")]
pub enum CompletionShell {
  Bash,
  Zsh,
}

// Parse --delim <xxx>.
fn parse_delim(delim: &str) -> std::result::Result<u8, String> {
  match delim {
    "tab" | "\\t" | "\t" => Ok(b'\t'),
    _ if delim.len() == 1 && delim.as_bytes()[0].is_ascii_graphic() => Ok(delim.as_bytes()[0]),
    " " => Ok(b' '),
    _ => Err("delimiter must be one printable ASCII char or tab".to_owned()),
  }
}

//
// tests
//

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_parse_delim() {
    assert_eq!(Ok(b';'), parse_delim(";"));
    assert_eq!(Ok(b'\t'), parse_delim("tab"));
    assert_eq!(Ok(b'\t'), parse_delim("\\t"));
    assert_eq!(Ok(b' '), parse_delim(" "));
    assert!(parse_delim(";;").is_err());
    assert!(parse_delim("é").is_err());
  }

  fn parse(args: &[&str]) -> Result<Args> {
    parse_from(std::iter::once("2fer").chain(args.iter().copied()))
  }

  #[test]
  fn test_parse_args() {
    let args =
      parse(&["--as", "json", "--compact", "--delim", "tab", "--table", "players", "--vanilla", "input.csv"]).unwrap();

    assert_eq!(Some("json"), args.as_format.as_deref());
    assert!(args.compact);
    assert_eq!(Some(b'\t'), args.delim);
    assert_eq!(Some("players"), args.table.as_deref());
    assert!(args.vanilla);
    assert_eq!(Some(PathBuf::from("input.csv")), args.input);
    assert!(args.argv_had_args);
  }

  #[test]
  fn test_parse_early_exits() {
    assert!(parse(&["--help"]).unwrap().help);
    assert!(parse(&["--version"]).unwrap().version);
    assert_eq!(Some(CompletionShell::Bash), parse(&["--completion", "bash"]).unwrap().completion);
  }

  #[test]
  fn test_parse_rejects_bad_args() {
    assert!(
      matches!(parse(&["--bogus"]), Err(Error::Usage(message)) if message == "unexpected argument '--bogus' found")
    );
    assert!(matches!(parse(&["--as"]), Err(Error::Usage(message)) if message.contains("a value is required")));
    assert!(
      matches!(parse(&["--to", "json"]), Err(Error::Usage(message)) if message.contains("unexpected argument '--to'"))
    );
    assert!(
      matches!(parse(&["--delim", ";;"]), Err(Error::Usage(message)) if message.contains("delimiter must be one printable ASCII char or tab"))
    );
  }

  #[test]
  fn test_help() {
    let out = help("2fer");

    assert!(out.contains("Usage: 2fer"));
    assert!(out.contains("Output:"));
    assert!(out.contains("Input:"));
    assert!(out.contains("Other:"));
    assert!(out.contains("--as <format>"));
    assert!(!out.contains("HINT"));
    assert!(!out.contains("output by default"));
  }

  #[test]
  fn test_help_symlink() {
    let out = help("2csv");
    assert!(out.contains("Usage: 2csv"));
    assert!(out.contains("Note: You ran `2csv`"));
    assert!(!out.contains("HINT"));
  }
}
