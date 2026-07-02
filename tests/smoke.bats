#!/usr/bin/env bats

# Bats is for process-level behavior that Rust unit tests do not cover well:
# real CLI IO, file output, symlink behavior, and subprocesses.

setup() {
  ROOT="$BATS_TEST_DIRNAME/.."
  BIN="$ROOT/target/debug/2fer"
}

run_ok() {
  run "$BIN" "$@"
  [ "$status" -eq 0 ]
}

run_tty() {
  command -v script >/dev/null || skip "script not installed"

  if [[ "$(uname)" == Darwin ]]; then
    run script -q /dev/null bash -lc "$1"
  else
    run script -qfec "$1" /dev/null
  fi
}

assert_output_matches() {
  [ "$(tr -d '\r' <<<"$output")" = "$(tr -d '\r' <<<"$1")" ]
}

#
# CLI basics
#

@test "requires an output format signal" {
  run bash -lc "printf 'name,score\nalice,1\n' | '$BIN'"
  [ "$status" -eq 1 ]
  [[ "$output" == *"2fer:"* ]]
}

@test "naked invocation prints help hint" {
  run_tty "'$BIN'"
  [ "$status" -eq 0 ]
  [[ "$output" == *"2fer: try '2fer --help' for more information"* ]]

  local link="$BATS_TEST_TMPDIR/2csv"
  ln -s "$BIN" "$link"
  run_tty "'$link'"
  [ "$status" -eq 0 ]
  [[ "$output" == *"2csv: try '2csv --help' for more information"* ]]

  run_tty "'$BIN' --as json"
  [ "$status" -eq 1 ]
  [[ "$output" == *"2fer: Could not read from stdin"* ]]
}

@test "bad args and flags" {
  run "$BIN" --bogus
  [ "$status" -eq 1 ]
  [[ "$output" == *"unexpected argument '--bogus'"* ]]
  [[ "$output" == *"2fer: try '2fer --help' for more information"* ]]
  [[ "$output" != *"Usage: 2fer"* ]]

  run "$BIN" --as
  [ "$status" -eq 1 ]
  [[ "$output" == *"a value is required for '--as <format>'"* ]]
  [[ "$output" == *"2fer: try '2fer --help' for more information"* ]]

  run "$BIN" --to json
  [ "$status" -eq 1 ]
  [[ "$output" == *"unexpected argument '--to'"* ]]

  run "$BIN" --completion fish
  [ "$status" -eq 1 ]
  [[ "$output" == *"invalid value 'fish' for '--completion <shell>'"* ]]

  local link="$BATS_TEST_TMPDIR/2csv"
  ln -s "$BIN" "$link"
  run "$link" --bogus
  [ "$status" -eq 1 ]
  [[ "$output" == *"2csv: unexpected argument '--bogus'"* ]]
  [[ "$output" == *"2csv: try '2csv --help' for more information"* ]]
}

@test "stdin csv to json stdout" {
  run bash -lc "printf 'name,score\nalice,1\n' | '$BIN' --as json"
  [ "$status" -eq 0 ]
  assert_output_matches $'[\n  {\n    "name": "alice",\n    "score": 1\n  }\n]'
}

@test "stdin semicolon csv without trailing newline" {
  run bash -lc "printf 'name;score\nalice;1\nbob;2' | '$BIN' --as json"
  [ "$status" -eq 0 ]
  assert_output_matches $'[\n  {\n    "name": "alice",\n    "score": 1\n  },\n  {\n    "name": "bob",\n    "score": 2\n  }\n]'
}

@test "broken pipe exits quietly" {
  run bash -lc "awk 'BEGIN { print \"a,b\"; for (i = 0; i < 100000; i++) print i \",\" i }' | '$BIN' --as json | head -3"
  [ "$status" -eq 0 ]
  [[ "$output" != *"Could not write to stdout"* ]]
}

@test "--help and --version" {
  run_ok --help
  [[ "$output" == *"Usage: 2fer [OPTIONS] [file]"* ]]
  [[ "$output" == *"Output:"* ]]
  [[ "$output" == *"Input:"* ]]
  [[ "$output" == *"Other:"* ]]
  [[ "$output" == *"--as <format>"* ]]

  run_ok --version
  [[ "$output" == 2fer:* ]]
  [[ "$output" == *"("*")" ]]

  local link="$BATS_TEST_TMPDIR/2csv"
  ln -s "$BIN" "$link"
  run "$link" --help
  [ "$status" -eq 0 ]
  [[ "$output" == *"Usage: 2csv [OPTIONS] [file]"* ]]
  [[ "$output" == *'Note: You ran `2csv`'* ]]

  run "$link" --version
  [ "$status" -eq 0 ]
  [[ "$output" == 2fer:* ]]
  [[ "$output" == *"(invoked as '2csv')"* ]]
}

@test "--completion" {
  run_ok --completion bash
  [[ "$output" == *"complete"* ]]
  [[ "$output" == *"2fer"* ]]
  [[ "$output" == *"--as"* ]]
  run bash -lc "set -o pipefail; '$BIN' --completion bash | bash -n"
  [ "$status" -eq 0 ]

  run_ok --completion zsh
  [[ "$output" == *"#compdef 2fer"* ]]
  [[ "$output" == *"--as"* ]]

  command -v zsh >/dev/null || skip "zsh not installed"
  run bash -lc "set -o pipefail; '$BIN' --completion zsh | zsh -n"
  [ "$status" -eq 0 ]
}

@test "symlink output command" {
  local link="$BATS_TEST_TMPDIR/2json"
  ln -s "$BIN" "$link"

  run bash -lc "printf 'name,score\nalice,1\n' | '$link'"
  [ "$status" -eq 0 ]
  [[ "$output" == *'"alice"'* ]]
}

@test "positional csv to json output file" {
  local input="$BATS_TEST_TMPDIR/input.csv"
  local outfile="$BATS_TEST_TMPDIR/output.json"
  printf 'name,score\nalice,1\n' >"$input"

  run_ok "$input" --output "$outfile"
  [ -s "$outfile" ]
  grep -q '"alice"' "$outfile"
}

@test "output format disagreement fails" {
  local input="$BATS_TEST_TMPDIR/input.csv"
  printf 'name,score\nalice,1\n' >"$input"

  run "$BIN" "$input" --as json --output "$BATS_TEST_TMPDIR/output.tsv"
  [ "$status" -eq 1 ]
  [[ "$output" == *"2fer:"* ]]
}

@test "--from is not supported" {
  run "$BIN" --from csv --as json
  [ "$status" -ne 0 ]
  [[ "$output" == *"unexpected argument '--from'"* ]]
}

@test "--delim overrides csv-like input" {
  run bash -lc "printf 'name;score\nalice;1\n' | '$BIN' --delim ';' --as json"
  [ "$status" -eq 0 ]
  assert_output_matches $'[\n  {\n    "name": "alice",\n    "score": 1\n  }\n]'
}

@test "--vanilla disables text infer" {
  run bash -lc "printf 'flag,score\ntrue,1.5\n' | '$BIN' --vanilla --as json"
  [ "$status" -eq 0 ]
  assert_output_matches $'[\n  {\n    "flag": "true",\n    "score": "1.5"\n  }\n]'
}

@test "stdin workbook detection" {
  local xlsx="$BATS_TEST_TMPDIR/input.xlsx"
  printf '[{"left":3,"right":4}]\n' | "$BIN" --as xlsx --output "$xlsx"

  run bash -lc "cat '$xlsx' | '$BIN' --as csv"
  [ "$status" -eq 0 ]
  [[ "$output" == *"3,4"* ]]
}

@test "sqlite stdin to json" {
  command -v sqlite3 >/dev/null || skip "sqlite3 not installed"

  local db="$BATS_TEST_TMPDIR/input.db"
  sqlite3 "$db" "create table players(name text); insert into players values ('alice');"

  run bash -lc "cat '$db' | '$BIN' --table players --as json"
  [ "$status" -eq 0 ]
  [[ "$output" == *'"alice"'* ]]
}

#
# Smoke conversions
#

@test "jsonl to xlsx" {
  local input="$BATS_TEST_TMPDIR/input.jsonl"
  local outfile="$BATS_TEST_TMPDIR/output.xlsx"
  printf '{"name":"alice","score":1}\n{"name":"bob","score":2}\n' >"$input"

  run_ok "$input" --output "$outfile"
  [ -s "$outfile" ]
}

@test "xlsx to csv" {
  local xlsx="$BATS_TEST_TMPDIR/input.xlsx"
  local csv="$BATS_TEST_TMPDIR/output.csv"
  printf '[{"name":"alice","score":1}]\n' | "$BIN" --as xlsx --output "$xlsx"

  run_ok "$xlsx" --output "$csv"
  grep -q "alice" "$csv"
}

@test "binary stdout allows redirection" {
  local input="$BATS_TEST_TMPDIR/input.csv"
  local xlsx="$BATS_TEST_TMPDIR/output.xlsx"
  printf 'name,score\nalice,1\n' >"$input"

  run bash -lc "'$BIN' '$input' --as xlsx > '$xlsx'"
  [ "$status" -eq 0 ]
  [ -s "$xlsx" ]
}

@test "binary stdout refuses terminal" {
  command -v script >/dev/null || skip "script not installed"

  local input="$BATS_TEST_TMPDIR/input.csv"
  printf 'name,score\nalice,1\n' >"$input"

  run_tty "'$BIN' '$input' --as xlsx"
  [ "$status" -eq 1 ]
  [[ "$output" == *"Refusing to write xlsx bytes to your terminal"* ]]
}

@test "xls input is rejected" {
  local xls="$BATS_TEST_TMPDIR/input.xls"
  printf '\xD0\xCF\x11\xE0' >"$xls"

  run "$BIN" "$xls" --as csv
  [ "$status" -eq 1 ]
  [[ "$output" == *"appears to be xls"* ]]
}

@test "sqlite to json" {
  command -v sqlite3 >/dev/null || skip "sqlite3 not installed"

  local db="$BATS_TEST_TMPDIR/input.db"
  sqlite3 "$db" "create table players(name text, score integer); insert into players values ('alice', 1);"

  run_ok "$db" --table players --as json
  [[ "$output" == *'"alice"'* ]]
}

@test "jsonl to sqlite" {
  command -v sqlite3 >/dev/null || skip "sqlite3 not installed"

  local db="$BATS_TEST_TMPDIR/output.db"
  printf '{"name":"alice","score":1}\n{"name":"bob","score":2}\n' | "$BIN" --table players --as sqlite --output "$db"

  run sqlite3 "$db" "select name || ':' || score from players where name = 'alice';"
  [ "$status" -eq 0 ]
  assert_output_matches "alice:1"
}

@test "sqlite extensionless path detection" {
  command -v sqlite3 >/dev/null || skip "sqlite3 not installed"

  local db="$BATS_TEST_TMPDIR/input"
  sqlite3 "$db" "create table players(name text, score integer); insert into players values ('alice', 1);"

  run_ok "$db" --table players --as json
  [[ "$output" == *'"alice"'* ]]
}

@test "md to json" {
  run bash -lc "printf '| name | score |\n| --- | ---: |\n| alice | 1 |\n' | '$BIN' --as json"
  [ "$status" -eq 0 ]
  [[ "$output" == *'"name": "alice"'* ]]
  [[ "$output" == *'"score": 1'* ]]
}

@test "jsonl to md" {
  run bash -lc "printf '{\"name\":\"alice\",\"note\":\"a|b\"}\n{\"name\":\"bob\",\"note\":\"c\"}\n' | '$BIN' --as markdown"
  [ "$status" -eq 0 ]
  assert_output_matches $'| name  | note |\n| ----- | ---- |\n| alice | a\\|b |\n| bob   | c    |'
}

@test "csv to json" {
  run bash -lc "printf 'name,score\nalice,1\n' | '$BIN' --as json"
  [ "$status" -eq 0 ]
  assert_output_matches $'[\n  {\n    "name": "alice",\n    "score": 1\n  }\n]'
}

@test "yaml to tsv" {
  run bash -lc "printf -- '- name: alice\n  score: 1\n' | '$BIN' --as tsv"
  [ "$status" -eq 0 ]
  assert_output_matches $'name\tscore\nalice\t1'
}
