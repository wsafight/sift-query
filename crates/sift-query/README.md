# SiftQuery

A typed, parameter-driven, read-only query library for Rust and sqlx.

SiftQuery validates fields, operators, budgets, ordering and paging before executing parameterized SQL. Application authorization is supplied separately through `ServerScope` and `GroupScope`.

```no_run
# #[cfg(feature = "postgres")]
# mod example {
use sift_query::{PgSiftQuery, RawParams, SearchEntity, ServerScope};

#[derive(Debug, sqlx::FromRow, SearchEntity)]
#[search_entity(from = "app_user", stable_order = "id:asc")]
struct User {
    #[search_field(sortable, cursor_non_null, operators = [Eq, InList])]
    id: i64,
    #[search_field(operators = [Eq, Contain])]
    name: String,
}

# async fn example(pool: sqlx::PgPool) -> Result<(), Box<dyn std::error::Error>> {
let search = PgSiftQuery::new(pool);
let params = RawParams::parse("name=alice&name-op=ct&size=20")?;
let page = search.search::<User>(&params, &ServerScope::unrestricted()).await?;
assert!(page.data.len() <= 20);
# Ok(())
# }
# }
```

Features include nested conditions, typed queries, dynamic and static projections, summaries, signed cursors, bounded scan/CSV, codecs, named pools, web adapters, result enrichment and SQL template caching. PostgreSQL additionally exposes `validate_schema::<E>()` for `EXPLAIN` based startup/CI checks, `ScopeProvider` based authorized search entry points, and bounded server declared facets. Ordinary queries do not require a transaction. The cache stores SQL templates, never query results.

`cursor_non_null` is a server assertion for ordering expressions guaranteed non-null by the database (such as an unmodified primary key). It enables a range predicate without NULL branches; do not infer it solely from a Rust field type or use it on a nullable outer-join expression. Changing the assertion changes the schema fingerprint and invalidates old cursors.

PostgreSQL, MySQL/MariaDB and SQLite execution are separate backend crates. `default-features = false` provides the backend-independent model and derive macros. Enable exactly one of `postgres`, `mysql` or `sqlite` for execution; `axum`, `actix-web` and `poem` add PostgreSQL web integration. PostgreSQL-specific extensions remain optional, and a PostgreSQL installation does not acquire MySQL or SQLite drivers.

For MySQL/MariaDB use `MySqlSiftQuery`; for SQLite use `SqliteSiftQuery`. Both expose `search`, `search_map`, `search_list`, `count`, `SearchOptions` summaries, caller-owned connection/transaction entry points, bounded scan streams, CSV export, and the portable Web parameter adapters. PostgreSQL-only RLS, native types, full-text search, and extension features remain on the PostgreSQL backend.

Rust 1.99.0 or newer is required. The API is currently pre-1.0. Licensed under Apache-2.0.

Optional PostgreSQL search (`postgres-search`) example:

```no_run
# #[cfg(feature = "postgres-search")]
# mod example {
use sift_query::{postgres::{PgCapabilities, TextConfig}, *};

#[derive(Debug, sqlx::FromRow, SearchEntity)]
#[search_entity(from = "article", stable_order = "id:asc")]
struct Article {
    #[search_field(sortable)]
    id: i64,
    #[sqlx(skip)]
    #[search_field(column="document", kind=FilterOnly, selectable=false,
        custom_operators=["fts_plain","fts_phrase","fts_web","fts_query"])]
    document: FilterOnly<String>,
}

async fn search(pool: sqlx::PgPool) -> Result<(), Box<dyn std::error::Error>> {
    let capabilities = PgCapabilities::default().with_full_text(TextConfig::English);
    capabilities.check(&pool).await?; // explicit application initialization
    let search = PgSiftQuery::with_policy(pool, capabilities.apply(QueryPolicy::default())?, PgExecutionPolicy::default())?;
    let params = RawParams::parse("document=rust&document-op=fts_plain")?;
    let _page = search.search::<Article>(&params, &ServerScope::unrestricted()).await?;
    Ok(())
}
# }
```

Opt-in trusted RLS context (`rls`) keeps settings inside one transaction:

```no_run
# #[cfg(feature = "rls")]
# async fn example(search: sift_query::PgSiftQuery) -> Result<(), Box<dyn std::error::Error>> {
use sift_query::RlsConfig;
let config = RlsConfig::new(&["app.tenant"], &["app_reader"])?;
let context = config.context(&[("app.tenant", "trusted-tenant")], Some("app_reader"))?;
let transaction = search.begin_rls(&context, std::time::Duration::from_secs(5)).await?;
// Use transaction.search/search_map with application queries and ServerScope.
transaction.finish().await?;
# Ok(())
# }
```

`metadata` exports safe query descriptions/OpenAPI fragments, `rsql` parses a bounded RSQL subset, `postgres-types` adds native array/range/network values, and `pg-trgm`/`ltree`/`postgis` enable optional extensions. These features are disabled by default.
