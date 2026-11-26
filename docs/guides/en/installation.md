# Installation

SiftQuery 0.2.0 is published on [crates.io](https://crates.io/crates/sift-query). Use Rust **1.95.0 or newer**. The SQLx integration uses **0.9**.

## Install for SQLite

Create a binary project with `cargo new sift-demo`, then add these dependencies to `Cargo.toml`:

```toml
[dependencies]
sift-query = { version = "0.2.0", default-features = false, features = ["sqlite", "serde"] }
sqlx = { version = "0.9", default-features = false, features = ["sqlite", "runtime-tokio", "macros"] }
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

The [quick start](quick-start.md) uses this exact configuration.

## Select PostgreSQL or MySQL

For PostgreSQL, replace both `sqlite` features with `postgres`. For MySQL or MariaDB, replace both with `mysql`. Use `PgSiftQuery`, `MySqlSiftQuery` or `SqliteSiftQuery` respectively. MariaDB uses the SQLx MySQL driver.

SiftQuery defaults to PostgreSQL plus `chrono`, `decimal`, `json`, `serde` and `uuid`. Disable default features to select another backend explicitly. Other dependencies can still enable additional SQLx drivers through Cargo feature unification.

## Optional integrations

| Feature | Purpose |
| --- | --- |
| `serde` | Serialization support across enabled backends |
| `chrono`, `json`, `uuid` | Optional value types, automatically forwarded to enabled backends |
| `decimal` | Decimal support for PostgreSQL and MySQL |
| `axum`, `actix-web`, `poem` | Web extractors and error adapters for any enabled backend |
| `metadata`, `rsql` | Query metadata and bounded RSQL parsing |
| `postgres-search`, `postgres-types` | Native PostgreSQL search and values |
| `pg-trgm`, `ltree`, `postgis`, `rls` | Optional PostgreSQL extensions and RLS context |

Starting in 0.2.0, `axum`, `actix-web`, `poem`, and value-type features propagate automatically to whichever database backends are enabled (backend-prefixed features like `sqlite-axum` are no longer needed). Check the [backend guide](backend-selection.md) before selecting native capabilities.

## API documentation

Use [docs.rs](https://docs.rs/sift-query) for generated Rust API documentation. The [Rust API guide](api.md) explains the query contracts and entry points. The library is licensed under Apache-2.0.
