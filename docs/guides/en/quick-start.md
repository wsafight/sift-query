# Quick start

This example runs against an in-memory SQLite database. Follow [Installation](installation.md), then replace `src/main.rs` with the following complete program.

## Declare and execute a query

```rust
use sift_query::{RawParams, SearchEntity, ServerScope, SqliteSiftQuery};
use sqlx::sqlite::SqlitePoolOptions;

#[derive(Debug, sqlx::FromRow, SearchEntity)]
#[search_entity(from = "users", default_order = "id:asc", stable_order = "id:asc")]
struct User {
    #[search_field(sortable, cursor_non_null, operators = [Eq, InList])]
    id: i64,
    #[search_field(operators = [Eq, StartWith])]
    name: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await?;

    sqlx::query("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT NOT NULL)")
        .execute(&pool)
        .await?;
    sqlx::query("INSERT INTO users (id, name) VALUES (?, ?), (?, ?), (?, ?)")
        .bind(1_i64).bind("Alice")
        .bind(2_i64).bind("Bob")
        .bind(3_i64).bind("Ari")
        .execute(&pool)
        .await?;

    let search = SqliteSiftQuery::new(pool);
    let raw = RawParams::parse("name=A&name-op=sw&size=10")?;
    let query = search.parse::<User>(&raw)?;
    let page = search.search(&query, &ServerScope::unrestricted()).await?;

    assert_eq!(page.total_count, 2);
    for user in page.data {
        println!("{}: {}", user.id, user.name);
    }
    Ok(())
}
```

The table is public demonstration data, so the example uses `ServerScope::unrestricted()`. For private application data, build a scope from authenticated identity and authorization before executing the query.

## Run it

```bash
cargo run
```

Expected output:

```text
1: Alice
3: Ari
```

`name-op=sw` means a literal prefix match; `size=10` limits the page size. `search` returns a page with rows and a total count. The stable primary-key order makes paging deterministic. The SQLite pool uses one connection so that this in-memory example shares one database.

## Understand the declaration

`SearchEntity` provides static field metadata; `sqlx::FromRow` decodes result rows. The `id` field allows equality, list membership and sorting, while `name` allows equality and prefix matching. Requests cannot add other operators to this declaration.

`cursor_non_null` asserts that the ordering expression is non-null in the database. This example has an unmodified primary key. Do not infer this assertion from a Rust field type or use it on a nullable outer-join expression.

## Continue

Try the [query cookbook](query-cookbook.md), compare [database backends](backend-selection.md), or read the [Rust API](api.md). Your application supplies schema migrations, connection pools and authorization; SiftQuery supplies the read-only query pipeline.
