//! Binary entry point.

use std::{
  ffi::{OsStr, OsString},
  io::{self, IsTerminal, Write},
  process::ExitCode,
};

use clap::CommandFactory;
use clap_complete::{Shell, generate};

mod app;
mod args;
mod cell;
mod error;
mod formats;
mod mishap;
mod table;
mod util;

use crate::{app::App, error::Result};

//
// simple main that wraps App
//

fn main() -> ExitCode {
  let argv = std::env::args_os().collect::<Vec<_>>();
  let symlink = build_symlink(argv.first().map(OsString::as_os_str));

  match main0(argv, &symlink) {
    Ok(()) => ExitCode::SUCCESS,
    Err(error) => {
      eprint!("{}", crate::error::format(&error, &symlink));
      ExitCode::FAILURE
    }
  }
}

fn main0(argv: Vec<OsString>, symlink: &str) -> Result<()> {
  let args = args::parse_from(argv)?;

  //
  // early exits
  //

  if let Some(shell) = args.completion {
    write_completion(shell, &mut io::stdout());
    return Ok(());
  }
  if args.help {
    let _ = write!(io::stdout(), "{}", args::help(symlink));
    return Ok(());
  }
  if args.version {
    let _ = writeln!(io::stdout(), "{}", version_line(symlink));
    return Ok(());
  }

  //
  // run
  //

  if symlink != "2fer" {
    util::log_2fer(format_args!("invoked as `{symlink}`"));
  }
  if args.input.is_none() && io::stdin().is_terminal() && !args.argv_had_args {
    let _ = io::stdout().write_all(crate::error::usage_hint(symlink).as_bytes());
    return Ok(());
  }

  let app = App::build(args, Some(symlink.to_owned()))?;

  app.main()
}

//
// helpers
//

/// how are we invoked? we typically call this "symlink" even though technically
/// it might be the string "2fer", our main bin
fn build_symlink(argv0: Option<&OsStr>) -> String {
  let name = argv0
    .and_then(OsStr::to_str)
    .and_then(|argv0| argv0.rsplit(['/', '\\']).next())
    .filter(|name| !name.is_empty())
    .unwrap_or("2fer")
    .to_owned();
  util::strip_suffix_ignore_ascii_case(&name, ".exe").unwrap_or(&name).to_owned()
}

fn clap_shell(shell: args::CompletionShell) -> Shell {
  match shell {
    args::CompletionShell::Bash => Shell::Bash,
    args::CompletionShell::Zsh => Shell::Zsh,
  }
}

const COMPLETION_NAMES: &[&str] = &["2fer", "2csv", "2json", "2jsonl", "2md", "2sqlite", "2tsv", "2xlsx", "2yml"];

fn write_completion(shell: args::CompletionShell, out: &mut dyn Write) {
  let mut command = args::Args::command();
  let mut bytes = Vec::new();
  generate(clap_shell(shell), &mut command, "2fer", &mut bytes);

  let mut text = String::from_utf8(bytes).expect("completion output should be UTF-8");
  add_completion_aliases(shell, &mut text);
  let _ = out.write_all(text.as_bytes());
}

fn add_completion_aliases(shell: args::CompletionShell, text: &mut String) {
  match shell {
    args::CompletionShell::Bash => add_bash_completion_aliases(text),
    args::CompletionShell::Zsh => add_zsh_completion_aliases(text),
  }
}

fn add_bash_completion_aliases(text: &mut String) {
  let aliases = COMPLETION_NAMES[1..].join(" ");
  text.push_str("\n# Handy 2fer symlinks use the same arguments.\n");
  text.push_str(
    "if [[ \"${BASH_VERSINFO[0]}\" -eq 4 && \"${BASH_VERSINFO[1]}\" -ge 4 || \"${BASH_VERSINFO[0]}\" -gt 4 ]]; then\n",
  );
  text.push_str(&format!("    complete -F _2fer -o nosort -o bashdefault -o default {aliases}\n"));
  text.push_str("else\n");
  text.push_str(&format!("    complete -F _2fer -o bashdefault -o default {aliases}\n"));
  text.push_str("fi\n");
}

fn add_zsh_completion_aliases(text: &mut String) {
  let names = COMPLETION_NAMES.join(" ");
  *text = text.replacen("#compdef 2fer", &format!("#compdef {names}"), 1);
  *text = text.replace("compdef _2fer 2fer", &format!("compdef _2fer {names}"));
}

fn version_line(symlink: &str) -> String {
  let sha = option_env!("TWOFER_GIT_SHA");
  let sha = sha.filter(|sha| !sha.is_empty()).unwrap_or("unknown sha");
  let mut line = format!("2fer: {} ({sha})", env!("CARGO_PKG_VERSION"));
  if symlink == "2fer" {
    return line;
  }
  line.push_str(&format!(" (invoked as '{symlink}')"));
  line
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_version_line() {
    assert!(version_line("2fer").starts_with(&format!("2fer: {} (", env!("CARGO_PKG_VERSION"))));
    assert!(!version_line("2fer").contains("invoked as"));
    assert!(version_line("2csv").ends_with(" (invoked as '2csv')"));
  }

  #[test]
  fn test_symlink() {
    assert_eq!("2fer", build_symlink(None));
    assert_eq!("2fer", build_symlink(Some(OsStr::new("/usr/bin/2fer"))));
    assert_eq!("2csv", build_symlink(Some(OsStr::new("/tmp/2csv"))));
    assert_eq!("2csv", build_symlink(Some(OsStr::new(r"C:\bin\2csv.exe"))));
    assert_eq!("2json", build_symlink(Some(OsStr::new("2json.EXE"))));
    assert_eq!("2tsv", build_symlink(Some(OsStr::new("2tsv.ExE"))));
  }

  #[test]
  fn test_write_completion() {
    let mut bash = Vec::new();
    write_completion(args::CompletionShell::Bash, &mut bash);
    let bash = String::from_utf8(bash).unwrap();
    assert!(bash.contains("complete"));
    assert!(bash.contains("2fer"));
    assert!(bash.contains("2csv 2json 2jsonl 2md 2sqlite 2tsv 2xlsx 2yml"));

    let mut zsh = Vec::new();
    write_completion(args::CompletionShell::Zsh, &mut zsh);
    let zsh = String::from_utf8(zsh).unwrap();
    assert!(zsh.contains("#compdef 2fer 2csv 2json 2jsonl 2md 2sqlite 2tsv 2xlsx 2yml"));
    assert!(zsh.contains("compdef _2fer 2fer 2csv 2json 2jsonl 2md 2sqlite 2tsv 2xlsx 2yml"));
    assert!(zsh.contains("--as"));
  }
}
