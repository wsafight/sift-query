#!/usr/bin/env bash
set -euo pipefail

# Cargo's temporary workspace registry shares the target path as its identity.
# An unchanged draft version must not reuse a previously unpacked source cache.
package_target="$(mktemp -d "${TMPDIR:-/tmp/}sift-query-package.XXXXXX")"
printf 'Verifying package archives in %s\n' "$package_target"
cargo package --workspace --all-features --allow-dirty --target-dir "$package_target"
