#!/usr/bin/env bash
set -euo pipefail

# Store a timestamped report directory below the supplied base path. The
# DATABASE_URL workload is intentionally opt-in because it creates and drops a
# dedicated sift_benchmark schema in that database.
base_dir="${1:-${SIFT_QUERY_BASELINE_DIR:-artifacts/performance-baseline}}"
timestamp="$(date -u +%Y%m%dT%H%M%SZ)"
report_dir="$base_dir/$timestamp"
mkdir -p "$report_dir"

if git_revision="$(git rev-parse --verify HEAD 2>/dev/null)"; then
  :
else
  git_revision=unavailable
fi

optional() {
  if "$@" 2>/dev/null; then
    return 0
  fi
  printf 'unavailable'
}

{
  printf 'collected_at_utc=%s\n' "$timestamp"
  printf 'repository=%s\n' "$(pwd)"
  printf 'git_revision=%s\n' "$git_revision"
  printf 'os=%s\n' "$(uname -s)"
  printf 'kernel=%s\n' "$(uname -r)"
  printf 'architecture=%s\n' "$(uname -m)"
  printf 'rustc=%s\n' "$(rustc --version)"
  printf 'cargo=%s\n' "$(cargo --version)"
  case "$(uname -s)" in
    Darwin)
      printf 'cpu_model=%s\n' "$(optional sysctl -n hw.model)"
      printf 'cpu_count=%s\n' "$(optional sysctl -n hw.ncpu)"
      printf 'memory_bytes=%s\n' "$(optional sysctl -n hw.memsize)"
      ;;
    Linux)
      printf 'cpu_model=%s\n' "$(optional awk -F: '/model name/{gsub(/^ +/, \"\", $2); print $2; exit}' /proc/cpuinfo)"
      printf 'cpu_count=%s\n' "$(optional getconf _NPROCESSORS_ONLN)"
      printf 'memory_kb=%s\n' "$(optional awk '/MemTotal:/{print $2; exit}' /proc/meminfo)"
      ;;
    *)
      printf 'cpu_model=unavailable\n'
      printf 'cpu_count=unavailable\n'
      printf 'memory=unavailable\n'
      ;;
  esac
  if [[ -n "${DATABASE_URL:-}" ]]; then
    printf 'postgres_workload=enabled\n'
    printf 'database_url=provided via environment (value omitted)\n'
  else
    printf 'postgres_workload=skipped (set DATABASE_URL to enable)\n'
  fi
} > "$report_dir/environment.txt"

printf 'Writing performance baseline to %s\n' "$report_dir"

if ! CARGO_TERM_COLOR=never cargo bench -p sift-query-sqlx --bench core_pipeline \
  -- --sample-size 20 --warm-up-time 1 --measurement-time 2 \
  > "$report_dir/core_pipeline.txt" 2>&1; then
  printf 'core_pipeline benchmark failed; see %s/core_pipeline.txt\n' "$report_dir" >&2
  exit 1
fi

if [[ -n "${DATABASE_URL:-}" ]]; then
  if ! CARGO_TERM_COLOR=never cargo bench -p sift-query-sqlx --bench postgres_workload \
    > "$report_dir/postgres_workload.txt" 2>&1; then
    printf 'postgres_workload benchmark failed; see %s/postgres_workload.txt\n' "$report_dir" >&2
    exit 1
  fi
else
  printf 'DATABASE_URL is unset; PostgreSQL workload was not run.\n' \
    > "$report_dir/postgres_workload.txt"
fi

printf 'Performance baseline completed: %s\n' "$report_dir"
