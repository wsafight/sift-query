use super::support::*;
use super::wide_support::*;

/// Covers the parallel and snapshot execution strategies with a list query.
#[tokio::test]
#[ignore = "requires a disposable PostgreSQL instance in DATABASE_URL"]
async fn parallel_and_snapshot_strategies_run_both_queries()
-> Result<(), Box<dyn std::error::Error>> {
    let database_url = std::env::var("DATABASE_URL")?;
    let pool = PgPoolOptions::new()
        .max_connections(6)
        .connect(&database_url)
        .await?;
    let _lock = serialise(&pool).await?;
    create_wide_table(&pool).await?;
    let scope = ServerScope::<Wide>::unrestricted();
    let cursor_scope = ServerScope::<WideCursor>::unrestricted();
    let raw = RawParams::parse("size=10")?;

    for strategy in [
        ExecutionStrategy::ParallelBestEffort,
        ExecutionStrategy::Snapshot(SnapshotIsolation::RepeatableRead),
        ExecutionStrategy::Snapshot(SnapshotIsolation::Serializable),
    ] {
        let search = PgSiftQuery::with_policy(
            pool.clone(),
            QueryPolicy::default(),
            PgExecutionPolicy {
                strategy,
                max_parallel_queries: 2,
                ..PgExecutionPolicy::default()
            },
        )?;

        let page = search.search::<Wide>(&raw, &scope).await?;
        assert_eq!(page.data.len(), 2, "{strategy:?}");
        assert_eq!(page.total_count, 2, "{strategy:?}");

        let map_page = search.search_map::<Wide>(&raw, &scope).await?;
        assert_eq!(map_page.data.len(), 2, "{strategy:?}");
        assert_eq!(map_page.total_count, 2, "{strategy:?}");

        let projected = search
            .search_as::<WideCursor, WideProjection>(&raw, &cursor_scope)
            .await?;
        assert_eq!(projected.data.len(), 2, "{strategy:?}");
        assert_eq!(projected.data[0].heading, "alpha", "{strategy:?}");
    }

    sqlx::query("DROP TABLE sift_query_test.wide")
        .execute(&pool)
        .await?;
    Ok(())
}
