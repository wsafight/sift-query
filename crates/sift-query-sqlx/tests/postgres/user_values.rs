use super::support::*;
use super::wide_support::*;

/// Verifies the sqlx 0.9 `AssertSqlSafe` contract end to end: a payload that
/// would be destructive if interpolated must round-trip as data.
#[tokio::test]
#[ignore = "requires a disposable PostgreSQL instance in DATABASE_URL"]
async fn user_values_are_bound_and_never_executed_as_sql() -> Result<(), Box<dyn std::error::Error>>
{
    let database_url = std::env::var("DATABASE_URL")?;
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&database_url)
        .await?;
    let _lock = serialise(&pool).await?;
    create_wide_table(&pool).await?;
    let search = PgSiftQuery::new(pool.clone());
    let scope = ServerScope::<Wide>::unrestricted();

    // Store the payload as ordinary data.
    let payload = "'; DROP TABLE sift_query_test.wide; --";
    sqlx::query("UPDATE sift_query_test.wide SET title = $1 WHERE id = 1")
        .bind(payload)
        .execute(&pool)
        .await?;

    // Filtering by it must match exactly one row, not execute anything.
    let encoded = "%27%3B+DROP+TABLE+sift_query_test.wide%3B+--";
    let found = search
        .search_list::<Wide>(
            &RawParams::parse(&format!("title={encoded}&size=10"))?,
            &scope,
        )
        .await?;
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].title, payload);

    // LIKE patterns must treat the payload as a pattern, not as syntax.
    let matched = search
        .search_list::<Wide>(
            &RawParams::parse(&format!("title={encoded}&title-op=ct&size=10"))?,
            &scope,
        )
        .await?;
    assert_eq!(matched.len(), 1);

    // The table is still there and still holds two rows.
    let remaining = search
        .search_list::<Wide>(&RawParams::parse("size=10")?, &scope)
        .await?;
    assert_eq!(remaining.len(), 2);

    // Wildcards typed by a user match literal characters, not everything.
    sqlx::query("UPDATE sift_query_test.wide SET title = '100%' WHERE id = 1")
        .execute(&pool)
        .await?;
    sqlx::query("UPDATE sift_query_test.wide SET title = '100x' WHERE id = 2")
        .execute(&pool)
        .await?;
    let literal_percent = search
        .search_list::<Wide>(
            &RawParams::parse("title=100%25&title-op=ct&size=10")?,
            &scope,
        )
        .await?;
    assert_eq!(literal_percent.len(), 1);
    assert_eq!(literal_percent[0].title, "100%");

    sqlx::query("DROP TABLE sift_query_test.wide")
        .execute(&pool)
        .await?;
    Ok(())
}
