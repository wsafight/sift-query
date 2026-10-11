# Changelog

## 0.1.0 — 2026-10-11

- PostgreSQL typed and dynamic read-only queries, projections, grouping and summaries.
- Strict parameter budgets, server scopes, signed keyset cursors and optional snapshot execution.
- Parameter filters, registered HTTP operators, reusable mappings and lossless RPC encoding.
- Temporal/list codecs, named pools and routing from trusted context.
- Bounded scan/CSV, web adapters, page/label enrichment and observations.
- Bounded SQL template cache that always rebuilds current bind values.
- Opt-in typed PostgreSQL full-text/JSONB, bigint/text arrays, int8range and inet/cidr queries.
- Optional pg_trgm, ltree and PostGIS filters, explicit capability checks and bound similarity ordering.
- Public metadata/OpenAPI fragments, bounded RSQL subset and trusted transaction-local RLS with scan/CSV support.
- Explicit non-null cursor-key assertions enable index range seeks without weakening nullable-key ordering.
- Backend isolation/package checks, property tests and five scheduled fuzz targets; exact JSON number RPC regression fixed.
- Source modules grouped by responsibility with unchanged public import paths; shared checked budgets, ASCII name validation and lazy TTL/LRU cache algorithms.
- Added PostgreSQL `validate_schema` startup/CI checks backed by `EXPLAIN`, trusted-context `ScopeProvider` authorization entry points across all execution backends, and bounded PostgreSQL facets over server-declared read-model fields.
- Independent MySQL/MariaDB and SQLite backends with typed/map queries, summaries, cursor pages, bounded scans, CSV export and web adapters.
- Field access policies, server-declared EXISTS predicates, DISTINCT/facet semantics, metadata-based TypeScript query builders, schema result-type checks, query diagnostics and time/numeric buckets.
- Entity validation caches, precomputed codec bindings, portable SQL template caches, reduced parser/rendering allocations and reusable CSV buffers.
- Release archives and registry installation verified after PostgreSQL, MySQL, MariaDB and SQLite CI checks.

All eight workspace library crates are available on crates.io at 0.1.0. The public API is pre-1.0.
