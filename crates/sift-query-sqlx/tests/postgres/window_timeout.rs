use super::support::*;

#[derive(Debug, FromRow, SearchEntity)]
#[allow(dead_code)]
#[search_entity(
    from = "generate_series(1, 1) g(id) cross join lateral pg_sleep(2)",
    default_order = "id:asc",
    stable_order = "id:asc",
    timeout_ms = 100
)]
struct SlowWindow {
    #[search_field(column = "g.id", sortable)]
    id: i64,
}

/// Covers the window-count timeout path.
#[tokio::test]
#[ignore = "requires a disposable PostgreSQL instance in DATABASE_URL"]
async fn a_window_query_reports_a_timeout() -> Result<(), Box<dyn std::error::Error>> {
    let database_url = std::env::var("DATABASE_URL")?;
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&database_url)
        .await?;

    let search = PgSiftQuery::with_policy(
        pool.clone(),
        QueryPolicy::default(),
        PgExecutionPolicy {
            timeout: std::time::Duration::from_millis(100),
            count_strategy: CountStrategy::WindowWhenPossible,
            ..PgExecutionPolicy::default()
        },
    )?;
    let error = search
        .search_list::<SlowWindow>(&RawParams::parse("")?, &ServerScope::unrestricted())
        .await
        .unwrap_err();
    assert!(matches!(error, PgSearchError::Timeout(_)), "{error:?}");

    // The window path is used for `search` with the window count strategy.
    let error = search
        .search::<SlowWindow>(&RawParams::parse("")?, &ServerScope::unrestricted())
        .await
        .unwrap_err();
    assert!(matches!(error, PgSearchError::Timeout(_)), "{error:?}");

    let value: i32 = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        sqlx::query_scalar("SELECT 1").fetch_one(&pool),
    )
    .await??;
    assert_eq!(value, 1);
    Ok(())
}
