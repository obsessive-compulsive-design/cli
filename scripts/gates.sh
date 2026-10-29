#!/bin/sh
# Runs every STORIES.md §0.2 gate locally, in the same form CI runs them.
# Needs cargo-llvm-cov and cargo-mutants on PATH (external tools, not crate dependencies).
set -eu
cd "$(dirname "$0")/.."

cargo fmt --all --check
cargo clippy --workspace -- -D warnings -W clippy::pedantic -D clippy::unwrap_used -D clippy::expect_used
cargo clippy --workspace --all-targets -- -D warnings -W clippy::pedantic
sh scripts/audit.sh
SJS_PROP_CASES="${SJS_PROP_CASES:-4096}" SJS_CORPUS_PROGRAMS="${SJS_CORPUS_PROGRAMS:-10000}" cargo test --workspace --all-targets
cargo test --workspace --doc
cargo llvm-cov --workspace --all-targets --fail-under-lines 100 --fail-under-regions 100 --show-missing-lines
SJS_FUZZ_SECONDS="${SJS_FUZZ_SECONDS:-60}" cargo test --workspace -- fuzz_
git diff "${GATES_BASE:-HEAD}" > target/changes.diff
if grep -q '^+++ .*\.rs$' target/changes.diff; then
  cargo mutants --in-diff target/changes.diff --workspace --timeout-multiplier 3
else
  echo "No Rust files changed; nothing to mutate."
fi
