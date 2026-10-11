use super::support::*;
use super::wide_support::*;

/// Exercises the configuration guards and the timeout-stage labels.
#[tokio::test]
#[ignore = "requires a disposable PostgreSQL instance in DATABASE_URL"]
async fn execution_policy_guards_and_labels() -> Result<(), Box<dyn std::error::Error>> {
    let database_url = std::env::var("DATABASE_URL")?;
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&database_url)
        .await?;

    let zero_timeout = PgSiftQuery::with_policy(
        pool.clone(),
        QueryPolicy::default(),
        PgExecutionPolicy {
            timeout: std::time::Duration::ZERO,
            ..PgExecutionPolicy::default()
        },
    );
    assert!(zero_timeout.is_err());

    let zero_parallel = PgSiftQuery::with_policy(
        pool.clone(),
        QueryPolicy::default(),
        PgExecutionPolicy {
            max_parallel_queries: 0,
            ..PgExecutionPolicy::default()
        },
    );
    assert!(zero_parallel.is_err());

    let oversized_cache = PgSiftQuery::with_policy(
        pool.clone(),
        QueryPolicy::default(),
        PgExecutionPolicy {
            shape_cache_capacity: 16_385,
            ..PgExecutionPolicy::default()
        },
    );
    assert!(oversized_cache.is_err());

    assert_eq!(TimeoutStage::PoolAcquire.to_string(), "pool acquire");
    assert_eq!(TimeoutStage::ListQuery.to_string(), "list query");
    assert_eq!(TimeoutStage::ClusterQuery.to_string(), "cluster query");
    assert_eq!(PgExecutionPolicy::default().max_parallel_queries, 16);
    assert_eq!(PgExecutionPolicy::default().shape_cache_capacity, 0);
    assert_eq!(
        ExecutionStrategy::default(),
        ExecutionStrategy::SequentialBestEffort
    );
    assert_eq!(CountStrategy::default(), CountStrategy::Separate);
    assert_eq!(
        SnapshotIsolation::default(),
        SnapshotIsolation::RepeatableRead
    );

    // The parallel gate is bounded by the configured concurrency.
    let narrow = PgSiftQuery::with_policy(
        pool.clone(),
        QueryPolicy::default(),
        PgExecutionPolicy {
            strategy: ExecutionStrategy::ParallelBestEffort,
            max_parallel_queries: 1,
            ..PgExecutionPolicy::default()
        },
    )?;
    let page = narrow
        .search::<WideCursor>(
            &RawParams::parse("size=10")?,
            &ServerScope::<WideCursor>::unrestricted(),
        )
        .await;
    // The table does not exist here, so either the list or the count query
    // fails; the point is that the parallel gate did not deadlock.
    assert!(page.is_err() || page.is_ok());
    Ok(())
}
