#!/usr/bin/env bash
set -euo pipefail

cargo clippy -p sift-query-core --all-targets --no-default-features -- -D warnings
cargo check -p sift-query --no-default-features
cargo test -p sift-query --no-default-features --doc
for feature in postgres-search postgres-types metadata rsql rls pg-trgm ltree postgis postgres,metadata; do
  cargo clippy -p sift-query --lib --no-default-features --features "$feature" -- -D warnings
done
