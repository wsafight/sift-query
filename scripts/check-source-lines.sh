#!/usr/bin/env bash
set -euo pipefail

max_lines=600
violations=0

while IFS= read -r -d '' file; do
  lines=$(wc -l < "$file")
  if (( lines > max_lines )); then
    printf '%s: %s lines (maximum %s)\n' "$file" "$lines" "$max_lines" >&2
    violations=1
  fi
done < <(find crates examples -type f -name '*.rs' -print0)

if (( violations != 0 )); then
  exit 1
fi
