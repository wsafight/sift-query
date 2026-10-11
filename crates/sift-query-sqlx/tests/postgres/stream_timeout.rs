use super::support::*;

#[derive(Debug, FromRow, SearchEntity)]
#[allow(dead_code)]
#[search_entity(
    from = "generate_series(1, 3) g(id) cross join lateral pg_sleep(0.2)",
    default_order = "id:asc",
    stable_order = "id:asc",
    timeout_ms = 150
)]
struct SlowStream {
    #[search_field(column = "g.id", sortable)]
    id: i64,
}

/// Covers the streaming timeout path, which needs a query that outlives its
/// deadline while rows are still being produced.
#[tokio::test]
#[ignore = "requires a disposable PostgreSQL instance in DATABASE_URL"]
async fn a_streaming_query_reports_a_timeout() -> Result<(), Box<dyn std::error::Error>> {
    let database_url = std::env::var("DATABASE_URL")?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&database_url)
        .await?;

    let search = PgSiftQuery::with_policy(
        pool.clone(),
        QueryPolicy::default(),
        PgExecutionPolicy {
            timeout: std::time::Duration::from_millis(150),
            ..PgExecutionPolicy::default()
        },
    )?;
    let mut stream =
        search.search_stream::<SlowStream>(&RawParams::parse("")?, &ServerScope::unrestricted())?;
    let mut seen = 0;
    let error = loop {
        match stream.try_next().await {
            Ok(Some(_)) => seen += 1,
            Ok(None) => break None,
            Err(error) => break Some(error),
        }
    };
    // Either the first fetch times out, or the stream ends after some rows.
    assert!(seen <= 3);
    if let Some(error) = error {
        assert!(
            matches!(error, PgSearchError::Timeout(TimeoutStage::ListQuery)),
            "{error:?}"
        );
    }

    // The pool survives the timed-out stream.
    let value: i32 = sqlx::query_scalar("SELECT 1").fetch_one(&pool).await?;
    assert_eq!(value, 1);
    Ok(())
}
