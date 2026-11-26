# Changelog

## 0.2.0 — 2026-11-26

### Breaking Changes

- Unified shared backend types (`Page`, `CursorPage`, `RowMap`, `SummaryValue`, `ScanPolicy`, `TimeoutStage`, `QueryObservation`, `QueryObserver`, `ExtensionError`, `LabelEnricher`, `PageEnricher`, `TypedPageEnricher`, `CsvColumn`, `CsvExportPolicy`, `ExportLimiter`, and web extractors) in `sift-query-sqlx-common` and re-exported them directly from `sift-query` and `sift-query-sqlx`, removing duplicate backend-prefixed aliases such as `MySqlPage`, `SqliteScanPolicy`, and `mysql_web`.
- Simplified `sift-query` feature flags by removing redundant `mysql-*` and `sqlite-*` combination features (`axum`, `actix-web`, `poem`, `chrono`, `decimal`, `json`, `serde`, and `uuid` now automatically propagate to any enabled backend).
- Changed `ScanPolicy::validate` to return `Result<Self, ParamError>` and marked `TimeoutStage` as `#[non_exhaustive]`.

### Fixes & Improvements

- **Core**: Enforced expanded query complexity budgets across nested boolean groups, fixed `ServerScope` + `OR` semantics, and tightened cursor and bucket validation.
- **SQL rendering**: Fixed cursor `NULL` bind leaks, `ORDER BY` null-ordering alignment, and `LIKE` escaping drift across PostgreSQL, MySQL/MariaDB, and SQLite.
- **PostgreSQL**: Preserved original query errors when statement timeout reset fails, enforced field-access policies on buckets and facets, and respected scan row/time budgets under RLS.
- **Deduplication**: Shared CSV export and web extractor implementations across all three sqlx backends via `sift-query-sqlx-common`.
- **Docs**: Added a bilingual Astro documentation site.

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
