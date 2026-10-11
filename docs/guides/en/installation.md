# Installation

SiftQuery 0.1.0 is published on [crates.io](https://crates.io/crates/sift-query). Use Rust **1.99.0 or newer**. The SQLx integration uses **0.9**.

## Install for SQLite

Create a binary project with `cargo new sift-demo`, then add these dependencies to `Cargo.toml`:

```toml
[dependencies]
sift-query = { version = "0.1.0", default-features = false, features = ["sqlite", "serde"] }
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
| `serde` | Serialization support |
| `chrono`, `json`, `uuid` | Optional value types, subject to backend support |
| `decimal` | Decimal support for PostgreSQL and MySQL |
| `axum`, `actix-web`, `poem` | PostgreSQL web integration |
| `mysql-axum`, `sqlite-axum` | Backend-specific Axum integration |
| `metadata`, `rsql` | Query metadata and bounded RSQL parsing |
| `postgres-search`, `postgres-types` | Native PostgreSQL search and values |
| `pg-trgm`, `ltree`, `postgis`, `rls` | Optional PostgreSQL extensions and RLS context |

The corresponding `mysql-actix-web`, `sqlite-actix-web`, `mysql-poem` and `sqlite-poem` features are also available. Check the [backend guide](backend-selection.md) before selecting native capabilities.

## API documentation

Use [docs.rs](https://docs.rs/sift-query) for generated Rust API documentation. The [Rust API guide](api.md) explains the query contracts and entry points. The library is licensed under Apache-2.0.
