#!/bin/sh
# STORIES.md §0.2 item 11: zero dependencies and all safe Rust.
#
# 1. Every crate in the resolved graph (normal, build and dev edges) must be a workspace
#    member, i.e. a path dependency inside this repository. `cargo tree` prints a path in
#    parentheses for path crates and nothing for registry or git crates.
# 2. Every Cargo manifest must forbid unsafe code, either directly or by inheriting the
#    workspace lint table.
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

for manifest in $(find . -name Cargo.toml -not -path './target/*'); do
  if ! grep -qE 'unsafe_code *= *"forbid"|lints\.workspace *= *true' "$manifest"; then
    echo "audit: $manifest does not forbid unsafe code" >&2
    status=1
  fi
done

if [ "$status" -eq 0 ]; then
  echo "audit: zero external crates, unsafe forbidden everywhere"
fi
exit "$status"
