#!/bin/sh
# STORIES.md §0.2 item 11: zero dependencies and all safe Rust.
#
# 1. Every crate in the resolved graph (normal, build and dev edges) must be a workspace
#    member, i.e. a path dependency inside this repository. `cargo tree` prints a path in
#    parentheses for path crates and nothing for registry or git crates.
# 2. Every Cargo manifest must forbid unsafe code: either a `[lints.rust]` /
#    `[workspace.lints.rust]` table containing `unsafe_code = "forbid"`, or a `[lints]`
#    table containing `workspace = true` (inheriting the root's forbid). The check is
#    structural, by TOML table, because the same text under `[package]` is silently
#    ignored by cargo.
set -eu
cd "$(dirname "$0")/.."

status=0

tree=$(cargo tree --workspace -e normal,build,dev --prefix none --no-dedupe) || {
  echo "audit: cargo tree failed" >&2
  exit 2
}
external=$(printf '%s\n' "$tree" | grep -v '^$' | grep -v ' (/' || true)
if [ -n "$external" ]; then
  echo "audit: external crates found in the dependency graph:" >&2
  echo "$external" | sort -u >&2
  status=1
fi

forbids_unsafe() {
  awk '
    /^[[:space:]]*\[/ { section = $0; gsub(/[][[:space:]]/, "", section); next }
    (section == "lints.rust" || section == "workspace.lints.rust") &&
      $0 ~ /^[[:space:]]*unsafe_code[[:space:]]*=[[:space:]]*"forbid"/ { ok = 1 }
    section == "lints" && $0 ~ /^[[:space:]]*workspace[[:space:]]*=[[:space:]]*true/ { ok = 1 }
    END { exit ok ? 0 : 1 }
  ' "$1"
}

for manifest in $(find . -name Cargo.toml -not -path './target/*'); do
  if ! forbids_unsafe "$manifest"; then
    echo "audit: $manifest does not forbid unsafe code" >&2
    status=1
  fi
done

if cargo check --workspace --all-targets 2>&1 | grep -q 'unused manifest key'; then
  echo "audit: a manifest has a key cargo ignores (see cargo check warnings)" >&2
  status=1
fi

if [ "$status" -eq 0 ]; then
  echo "audit: zero external crates, unsafe forbidden everywhere"
fi
exit "$status"
