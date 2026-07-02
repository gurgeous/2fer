## Project

- Rust CLI for converting tabular data between CSV, TSV, JSON, JSONL/NDJSON, Markdown, YML, SQLite input, XLS/XLSX input, and XLSX output.

## Important

- Branch names: `^[a-z_]+$`
- COMMIT: include all current changes by default.
- PR bodies: 1-2 bullets max, use `--body-file`, no backticks.
- PR merge: do not wait for CI unless asked.
- PR: include `Fixes #N` when applicable.
- CHANGELOG: match style, reference issues, credit issue authors.
- Justfile is at `.justfile`, no need to search. Always prefer `just` but ask before creating new tasks.

## Rust Style

- Use idiomatic Rust: `rustfmt`, `clippy`, `Result`, clear ownership, small modules.
- Keep APIs small; avoid one-use wrappers unless they clarify behavior.
- Inline trivial one-use wrappers; keep a helper only when it names real behavior, hides real complexity, or has multiple callers.
- Treat long arg lists as a smell; options structs must model real domain values.
- Prefer `build_xxx` over `resolve_xxx` when constructing derived values from args or config.
- Avoid test-only dependency injection; prefer real value objects or direct code.
- Comment any API split that exists only for tests or other non-primary paths.
- Prefer table-based tests and tiny helpers over repeated setup.
- Name Rust unit tests `test_fn_name`; use `test_fn_name_case` only when one function needs multiple tests.
- Comments explain intent, jargon, tradeoffs, algorithms, or invariants; skip name/type restatements.
- Every Rust source file, struct, and enum has a succinct role comment.
- Add succinct comments to complicated structs, fields, and functions.
- Use short section comments (`//`, `// format`, `// helpers`, `// tests`) to break up longer Rust files; keep them one or two words and avoid restating obvious code.
- Preserve helpful comments during refactors. Do not remove comments without asking. Don't add stupid comments.
- Import sibling modules/items at top; avoid repeated inline `crate::foo::bar`.
- Use Rust doc comments idiomatically: `//!` for module docs and `///` for public API docs.
- Do not map our own errors except at real layer boundaries; add context at the source.
- Silently ignore non-actionable stdout/pager write failures.
- Keep crate-sensitive behavior behind local modules.

## Tests

- Use `just` tasks. Do not run `cargo fmt` directly; `just llm` uses nightly rustfmt.
- After each code change, run `just llm`.
- Keep tests deterministic.
- Prefer table-based tests for parser and infer case matrices.
- Do not force flags in every parity test; cover defaults explicitly.
- Every runtime/system probe needs a Rust equivalent and test.

## Style

- Keep files/APIs small and direct.
- Bin scripts and important files should start with a short top-level comment.
- Prefer simple values and explicit ownership.
