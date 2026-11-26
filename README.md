English | [简体中文](README.zh-CN.md)

# SiftQuery

SiftQuery is a typed, parameter-driven, read-only query library for Rust and SQLx. Your application declares allowed fields, operators, projections, and server-owned scopes; SiftQuery validates request parameters, enforces query budgets, and renders parameterized SQL across PostgreSQL, MySQL/MariaDB, and SQLite.

## Why SiftQuery

- **Declared boundaries**: Derive `SearchEntity` to define fields, operator whitelists, projections, and stable ordering. Invalid parameters are rejected before touching the database.
- **Server-owned authorization**: Supply `ServerScope` (or implement `ScopeProvider`) from trusted identity and authorization context. Client filters never bypass server scopes or inject raw SQL, table names, or column names.
- **Three isolated database backends**: PostgreSQL, MySQL/MariaDB, and SQLite live in separate crates and Cargo features. `sift-query-core` does not depend on `sqlx`, and choosing one backend does not pull in other database drivers.
- **Production query pipeline**: Typed rows and dynamic maps, `SUM`/`AVG`/`MIN`/`MAX` summaries, signed keyset cursors (with or without `COUNT`), bounded scans and CSV exports, field access policies, server-declared `EXISTS`/`NOT EXISTS`, facets, time/numeric buckets, web extractors (`axum`, `actix-web`, `poem`), codecs, label enrichment, observers, and a bounded SQL template cache that always rebinds current values.
- **Optional PostgreSQL capabilities**: Opt-in full-text search, JSONB, native array/range/network types, `pg_trgm`, `ltree`, PostGIS, `validate_schema::<E>()` via `EXPLAIN`, query diagnostics, safe metadata/OpenAPI fragments, bounded RSQL, and transaction-local RLS.

## Installation

Version **0.2.0** is published on [crates.io](https://crates.io/crates/sift-query) (MSRV **Rust 1.95.0**, SQLx **0.9**, pre-1.0 API).

```toml
[dependencies]
sift-query = { version = "0.2.0", default-features = false, features = ["postgres", "serde"] }
sqlx = { version = "0.9", default-features = false, features = ["postgres", "runtime-tokio", "macros"] }
```

Disable default features and replace `postgres` with `mysql` or `sqlite` to select another backend. Integration features (`axum`, `actix-web`, `poem`, `chrono`, `decimal`, `json`, `serde`, `uuid`) automatically apply to any enabled backend.

## Quick Example

```rust
use sift_query::{RawParams, SearchEntity, ServerScope, SqliteSiftQuery};

#[derive(Debug, sqlx::FromRow, SearchEntity)]
#[search_entity(from = "users", default_order = "id:asc", stable_order = "id:asc")]
struct User {
    #[search_field(sortable, cursor_non_null, operators = [Eq, InList])]
    id: i64,
    #[search_field(operators = [Eq, StartWith])]
    name: String,
}

// Parse URL params: name=A&name-op=sw&size=10
let raw = RawParams::parse("name=A&name-op=sw&size=10")?;
let query = search.parse::<User>(&raw)?;
let page = search.search(&query, &ServerScope::unrestricted()).await?;
```

## Documentation

- **Documentation site (EN / 中文)**: See [site/README.md](site/README.md) for local preview and GitHub Pages deployment.
- **Getting started**: [Overview](docs/guides/en/overview.md) · [Installation](docs/guides/en/installation.md) · [Quick start](docs/guides/en/quick-start.md) · [Choose a backend](docs/guides/en/backend-selection.md)
- **Reference**: [API docs on docs.rs](https://docs.rs/sift-query) · [Rust API Guide](docs/guides/en/api.md) · [Changelog](CHANGELOG.md) · [Security](SECURITY.md)
- **中文文档**: [README.zh-CN.md](README.zh-CN.md) · [文档索引](docs/README.md)

## License

Licensed under the [Apache-2.0](LICENSE) license.
