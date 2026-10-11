use super::support::*;

#[derive(Debug, FromRow, SearchEntity)]
#[allow(dead_code)]
#[search_entity(
    from = "generate_series(1, 1) g(id) cross join lateral pg_sleep(2)",
    default_order = "id:asc",
    stable_order = "id:asc",
    timeout_ms = 100
)]
struct SlowRow {
    #[search_field(column = "g.id", sortable)]
    id: i64,
}

/// Verifies that timing out a query does not poison the pool.
#[tokio::test]
#[ignore = "requires a disposable PostgreSQL instance in DATABASE_URL"]
async fn timeout_keeps_the_pool_usable() -> Result<(), Box<dyn std::error::Error>> {
    let database_url = std::env::var("DATABASE_URL")?;
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&database_url)
        .await?;
    let search = PgSiftQuery::with_policy(
        pool.clone(),
        sift_query::QueryPolicy::default(),
        PgExecutionPolicy {
            timeout: std::time::Duration::from_millis(100),
            ..PgExecutionPolicy::default()
        },
    )?;
    let raw = RawParams::parse("")?;
    let error = search
        .search_list::<SlowRow>(&raw, &ServerScope::unrestricted())
        .await
        .unwrap_err();
    assert!(matches!(error, PgSearchError::Timeout(_)));

    let value: i32 = tokio::time::timeout(
        std::time::Duration::from_secs(1),
        sqlx::query_scalar("SELECT 1").fetch_one(&pool),
    )
    .await??;
    assert_eq!(value, 1);
    let mut sleeping_queries = 1_i64;
    for _ in 0..20 {
        sleeping_queries = sqlx::query_scalar(
            "SELECT count(*) FROM pg_stat_activity \
             WHERE state = 'active' AND query LIKE '%pg_sleep(2)%' AND pid <> pg_backend_pid()",
        )
        .fetch_one(&pool)
        .await?;
        if sleeping_queries == 0 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert_eq!(sleeping_queries, 0);
    Ok(())
}
