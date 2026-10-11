# Choose a backend

SiftQuery executes queries on PostgreSQL, MySQL/MariaDB and SQLite. The core query model does not depend on SQLx; execution uses separate backend crates.

## Select a driver

| Database | Facade feature | Executor | Backend crate |
| --- | --- | --- | --- |
| PostgreSQL | `postgres` | `PgSiftQuery` | `sift-query-sqlx` |
| MySQL / MariaDB | `mysql` | `MySqlSiftQuery` | `sift-query-mysql` |
| SQLite | `sqlite` | `SqliteSiftQuery` | `sift-query-sqlite` |

Disable default features and select the backend you need. MariaDB uses SQLx's MySQL driver. SiftQuery does not currently provide other database backends, even when another SQLx version or driver supports them.

## Compare capabilities

| Capability | PostgreSQL | MySQL / MariaDB | SQLite |
| --- | --- | --- | --- |
| Strict params, field/operator allowlists, scopes | Yes | Yes | Yes |
| Typed/map queries, count and summaries | Yes | Yes | Yes |
| Signed keyset cursor, scan and CSV | Yes | Yes | Yes |
| Caller-owned connections / transactions | Yes | Yes | Yes |
| Codec, labels, observers, web parameter adapters | Yes | Yes | Yes |
| `ScopeProvider` authorized entry points | Yes | Yes | Yes |
| Schema validation and facets | PostgreSQL API | — | — |
| Native JSONB, arrays/ranges/network, full text | Optional | — | — |
| pg_trgm / ltree / PostGIS / RLS context | Optional | — | — |

The capability matrix describes the library contract; it does not install database extensions or probe the database during ordinary requests. SQL dialects, aggregate output types and supported native values still vary by backend.

## Validation boundaries

Release validation includes PostgreSQL 17, MySQL 8.4, MariaDB 11.4 and SQLite. MySQL and MariaDB have independent service regression coverage and scan/export connection-reuse checks. Their full cancellation and output-failure matrix still requires additional work; deployment load and memory limits must be checked in the target environment.

Read [Release and backend isolation](release.md) and [PostgreSQL capabilities](postgres.md) for details. Start locally with the [SQLite example](quick-start.md).
