#!/usr/bin/env bash
set -euo pipefail

check_tree() {
  local label="$1"
  local forbidden="$2"
  shift 2
  local dependency_tree
  dependency_tree="$(cargo tree --locked --edges normal --prefix none "$@")"
  if rg --quiet "$forbidden" <<<"$dependency_tree"; then
    printf 'Unexpected database dependency in %s\n' "$label" >&2
    rg "$forbidden" <<<"$dependency_tree" >&2
    return 1
  fi
  printf '%s dependency isolation passed\n' "$label"
}

check_tree core '^sqlx' -p sift-query-core --no-default-features
check_tree facade-core '^sqlx' -p sift-query --no-default-features
check_tree postgres '^sqlx-(mysql|sqlite)' -p sift-query --no-default-features --features postgres
check_tree mysql '^sqlx-(postgres|sqlite)' -p sift-query --no-default-features --features mysql
check_tree sqlite '^sqlx-(postgres|mysql)' -p sift-query --no-default-features --features sqlite
