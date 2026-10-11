use super::*;
use sqlx::postgres::PgPoolOptions;
use std::time::Duration;

/// These exercise connection bookkeeping, so they need a real server.
async fn live_pool() -> sqlx::PgPool {
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    PgPoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
        .expect("database must be reachable")
}

/// The `sqlx` 0.9 `SqlSafeStr` assertion is satisfied by routing every
/// dynamic query through `PreparedPgQuery::sql`, so this pins the contract:
/// the value handed to `sqlx` is always an owned `AssertSqlSafe`, never a
/// borrowed reference that could outlive the prepared query.
#[test]
fn prepared_queries_hand_sqlx_an_owned_safe_sql_string() {
    let prepared = prepare(RenderedPgQuery {
        sql: Arc::from("SELECT 1"),
        binds: Vec::new(),
        shape_id: crate::QueryShapeId(7),
    })
    .unwrap();
    let safe = prepared.sql();
    assert_eq!(safe.0.as_ref(), "SELECT 1");
    // The assertion shares the original allocation rather than copying it.
    assert!(Arc::ptr_eq(&safe.0, &prepared.sql));
    assert_eq!(prepared.shape_id, crate::QueryShapeId(7));
}

/// `SqlSafeStr` is implemented for the owned wrappers only, so the assertion
/// must be an owned `AssertSqlSafe<Arc<str>>` rather than a borrow of the
/// prepared query. This keeps the `Query` values returned by
/// `query_with`/`query_as_with` free of any borrow from `PreparedPgQuery`,
/// which is what lets them be moved into `timeout_at` and into the stream.
#[test]
fn the_sql_assertion_does_not_borrow_the_prepared_query() {
    fn assert_owned<T: sqlx::SqlSafeStr>(value: T) -> T {
        value
    }
    let prepared = prepare(RenderedPgQuery {
        sql: Arc::from("SELECT $1"),
        binds: Vec::new(),
        shape_id: crate::QueryShapeId(8),
    })
    .unwrap();
    let sql = assert_owned(prepared.sql());
    // Dropping the prepared query must not invalidate the assertion.
    drop(prepared);
    assert_eq!(sql.0.as_ref(), "SELECT $1");
}

#[tokio::test]
#[ignore = "requires a disposable PostgreSQL instance in DATABASE_URL"]
async fn configure_statement_timeout_reports_an_expired_deadline() {
    let pool = live_pool().await;
    // An already-expired deadline makes the setup query time out, which is
    // the branch that closes the connection rather than returning it.
    let deadline = Instant::now() - Duration::from_secs(1);
    let connection = pool.acquire().await.expect("pool hands out slots");
    let error = configure_statement_timeout(connection, deadline, TimeoutStage::ListQuery)
        .await
        .unwrap_err();
    assert!(
        matches!(error, PgSearchError::Timeout(TimeoutStage::ListQuery)),
        "{error:?}"
    );
}

#[tokio::test]
#[ignore = "requires a disposable PostgreSQL instance in DATABASE_URL"]
async fn configure_statement_timeout_returns_a_usable_connection() {
    let pool = live_pool().await;
    let deadline = Instant::now() + Duration::from_secs(5);
    let connection = pool.acquire().await.expect("pool hands out slots");
    let connection = configure_statement_timeout(connection, deadline, TimeoutStage::ListQuery)
        .await
        .expect("a live server accepts the timeout setting");
    // `finish_statement` resets it and hands the connection back.
    let (mut connection, ()) = finish_statement(connection, Ok(()), TimeoutStage::ListQuery)
        .await
        .expect("reset succeeds");
    connection.return_to_pool().await;
}

#[tokio::test]
#[ignore = "requires a disposable PostgreSQL instance in DATABASE_URL"]
async fn finish_statement_reports_a_failed_query() {
    let pool = live_pool().await;
    let connection = pool.acquire().await.expect("pool hands out slots");
    let failure: Result<(), sqlx::Error> = Err(sqlx::Error::RowNotFound);
    let error = finish_statement(connection, failure, TimeoutStage::ClusterQuery)
        .await
        .unwrap_err();
    assert!(matches!(error, PgSearchError::Sql(_)), "{error:?}");
}

#[tokio::test]
#[ignore = "requires a disposable PostgreSQL instance in DATABASE_URL"]
async fn finish_statement_maps_a_statement_timeout_to_a_timeout_error() {
    let pool = live_pool().await;
    let connection = pool.acquire().await.expect("pool hands out slots");
    // 57014 is the PostgreSQL statement-timeout SQLSTATE.
    let failure: Result<(), sqlx::Error> = Err(sqlx::Error::Database(Box::new(TestDatabaseError(
        "57014".to_owned(),
    ))));
    let error = finish_statement(connection, failure, TimeoutStage::ListQuery)
        .await
        .unwrap_err();
    assert!(
        matches!(error, PgSearchError::Timeout(TimeoutStage::ListQuery)),
        "{error:?}"
    );
}

#[tokio::test]
#[ignore = "requires a disposable PostgreSQL instance in DATABASE_URL"]
async fn close_hard_releases_the_connection() {
    let pool = live_pool().await;
    let connection = pool.acquire().await.expect("pool hands out slots");
    close_hard(connection).await;
    // The pool is still usable after a hard close.
    let value: i32 = sqlx::query_scalar("SELECT 1")
        .fetch_one(&pool)
        .await
        .expect("pool remains usable");
    assert_eq!(value, 1);
}

#[test]
fn elapsed_micros_is_monotonic() {
    let started = Instant::now();
    let first = elapsed_micros(started);
    let second = elapsed_micros(started);
    assert!(second >= first);
}

#[derive(Debug)]
struct TestDatabaseError(String);

impl std::fmt::Display for TestDatabaseError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("test database error")
    }
}

impl std::error::Error for TestDatabaseError {}

impl sqlx::error::DatabaseError for TestDatabaseError {
    fn message(&self) -> &'static str {
        "canceling statement due to statement timeout"
    }

    fn kind(&self) -> sqlx::error::ErrorKind {
        sqlx::error::ErrorKind::Other
    }

    fn as_error(&self) -> &(dyn std::error::Error + Send + Sync + 'static) {
        self
    }

    fn as_error_mut(&mut self) -> &mut (dyn std::error::Error + Send + Sync + 'static) {
        self
    }

    fn into_error(self: Box<Self>) -> Box<dyn std::error::Error + Send + Sync + 'static> {
        self
    }

    fn code(&self) -> Option<std::borrow::Cow<'_, str>> {
        Some(std::borrow::Cow::Borrowed(&self.0))
    }
}
