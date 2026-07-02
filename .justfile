default:
  just --list

archive target asset:
  just build --release --target {{target}}
  just banner "archive {{target}} {{asset}}..."
  bin/archive {{target}} {{asset}}

build *ARGS:
  just banner "build {{ARGS}}..."
  cargo build --quiet {{ARGS}}

build-release: (build "--release")
  ls -lh target/release/2fer

build-small: (build "--profile small")
  ls -lh target/small/2fer

clean:
  cargo clean
  rm -rf tmp && mkdir tmp

run *ARGS:
  cargo run -- {{ARGS}}

#
# check/llm
#

[windows]
check: build test bats
  just banner "✓ check ✓"

[unix]
check: build lint test bats
  just banner "✓ check ✓"

llm:
  LLM=1 just fmt check

bats *ARGS:
  just banner "bats..."
  mkdir -p tmp
  if [ -n "${LLM:-}" ]; then \
    bats {{ARGS}} tests/smoke.bats > tmp/bats.out 2>&1 || { cat tmp/bats.out; exit 1; } ; \
  else \
    bats {{ARGS}} --print-output-on-failure tests/smoke.bats ; \
  fi

coverage:
  rm -rf tmp/coverage && mkdir -p tmp/coverage
  mise x cargo:cargo-llvm-cov -- \
    cargo llvm-cov --all-targets --all-features --workspace --html --output-dir tmp/coverage
  just banner "✓ coverage -> tmp/coverage/html/index.html ✓"

fmt:
  just banner "fmt..."
  cargo +nightly fmt --all

install: build
  mkdir -p ~/.local/bin
  cp target/debug/2fer ~/.local/bin/2fer
  cd ~/.local/bin && for name in 2csv 2json 2jsonl 2md 2sqlite 2tsv 2xlsx 2yml; do ln -sf 2fer "$name"; done
  just banner "installed ~/.local/bin/2fer and symlinks"

lint:
  just banner "lint..."
  rustup --quiet component add --toolchain nightly rustfmt
  rustup --quiet component add clippy
  cargo +nightly fmt --all --check
  cargo clippy --quiet --all-targets --all-features -- -D warnings

test *ARGS:
  just banner "test {{ARGS}}..."
  cargo test --quiet {{ARGS}}

test-watch:
  watchexec --clear=clear --stop-timeout=0 just test

#
# banner
#

set quiet

banner msg bg="64;160;43":
  if [ -z "${LLM:-}" ]; then \
    printf "\e[1;38;5;231;48;2;%sm[%s] %-72s\e[0m\n" "{{bg}}" $(date +"%H:%M:%S") "{{msg}}" ; \
  fi
warning +msg: (banner msg "251;100;11")
fatal +msg: (banner msg "210;15;57")
  exit 1
