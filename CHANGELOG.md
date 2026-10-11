# Changelog

## 0.1.0 — Unreleased

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

This is an unreleased development version. Availability on crates.io is not implied.
