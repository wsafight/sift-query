#![cfg(all(feature = "decimal", feature = "chrono"))]

pub(super) use std::sync::Arc;

pub(super) use std::sync::atomic::{AtomicBool, Ordering};

pub(super) use futures_util::TryStreamExt;
pub(super) use sift_query_derive::SearchEntity;

pub(super) use sift_query::{
    CursorCodec, CustomAggregate, QueryPolicy, RawParams, RowValue, SearchOptions, ServerScope,
    ValueType,
};

pub(super) use sift_query_sqlx::{
    CountStrategy, ExecutionStrategy, ExtensionError, PartitionName, PartitionResolver,
    PgExecutionPolicy, PgSearchError, PgSiftQuery, QueryContext, ResultContext, ResultFilter,
    ResultFilterDecision, SnapshotIsolation, SqlContext, SqlInterceptor, TimeoutStage,
};

pub(super) use sqlx::FromRow;

pub(super) use sqlx::postgres::PgPoolOptions;

#[derive(Debug, FromRow, SearchEntity)]
#[allow(dead_code)]
#[search_entity(
    from = "sift_query_test.users u",
    base_where = "u.deleted = false",
    default_order = "id:asc",
    stable_order = "id:asc"
)]
pub(super) struct User {
    #[search_field(
        column = "u.id",
        sortable,
        operators = [Eq, InList, NotIn, NotBetween, AlwaysTrue, AlwaysFalse]
    )]
    pub(super) id: i64,
    #[search_field(
        column = "u.name",
        operators = [
            Eq,
            StartWith,
            EndWith,
            Contain,
            OrLike,
            NotLike,
            Empty,
            NotEmpty
        ]
    )]
    pub(super) name: String,
    #[search_field(column = "u.status", operators = [Eq, InList, NotIn])]
    pub(super) status: i32,
    #[search_field(column = "u.age", operators = [Eq, Ge, Le, Between, NotBetween])]
    pub(super) age: i32,
}

#[derive(Debug, FromRow, sift_query_derive::SearchProjection)]
#[allow(dead_code)]
#[search_projection(entity = User)]
pub(super) struct UserSummary {
    pub(super) id: i64,
    #[search_field(source = "name")]
    pub(super) username: String,
}

#[derive(Debug, FromRow, SearchEntity)]
#[allow(dead_code)]
#[search_entity(
    from = "sift_query_test.users u",
    distinct,
    default_order = "status:asc",
    stable_order = "status:asc"
)]
pub(super) struct DistinctStatus {
    #[search_field(column = "u.status", sortable)]
    pub(super) status: i32,
}

#[derive(Debug, FromRow, SearchEntity)]
#[allow(dead_code)]
#[search_entity(
    from = "sift_query_test.scores s",
    default_order = "courseId:asc",
    stable_order = "courseId:asc"
)]
pub(super) struct CourseTotal {
    #[search_field(
        name = "courseId",
        column = "s.course_id",
        kind = GroupKey,
        sortable,
        operators = [Eq, InList]
    )]
    pub(super) course_id: i64,
    #[search_field(
        column = "sum(s.score)",
        kind = Aggregate,
        operators = [Eq, Ge, Le, Between]
    )]
    pub(super) total: i64,
}

#[derive(Debug, FromRow, SearchEntity)]
#[allow(dead_code)]
#[search_entity(
    from = "sift_query_test.pg_types t",
    default_order = "id:asc",
    stable_order = "id:asc"
)]
pub(super) struct PgTypes {
    #[search_field(column = "t.id", sortable)]
    pub(super) id: i64,
    pub(super) flag: bool,
    pub(super) ratio: f64,
    pub(super) title: String,
    pub(super) data: Vec<u8>,
    pub(super) event_date: chrono::NaiveDate,
    pub(super) event_time: chrono::NaiveTime,
    pub(super) local_at: chrono::NaiveDateTime,
    pub(super) observed_at: chrono::DateTime<chrono::Utc>,
    pub(super) external_id: uuid::Uuid,
    pub(super) amount: rust_decimal::Decimal,
    #[search_field(filterable = false)]
    pub(super) payload: serde_json::Value,
}

pub(super) struct UsersPartition;

impl PartitionResolver for UsersPartition {
    fn resolve(
        &self,
        _context: &QueryContext<'_>,
    ) -> Result<Option<PartitionName>, ExtensionError> {
        Ok(Some(
            PartitionName::qualified("sift_query_test", "users")?.with_alias("u")?,
        ))
    }
}

pub(super) struct CommentInterceptor(pub(super) Arc<AtomicBool>);

impl SqlInterceptor for CommentInterceptor {
    fn intercept(
        &self,
        _context: &SqlContext,
        sql: &str,
    ) -> Result<Option<String>, ExtensionError> {
        self.0.store(true, Ordering::Relaxed);
        Ok(Some(format!("{sql} /* sift-query-m2 */")))
    }
}

pub(super) struct RemoveName;

impl ResultFilter for RemoveName {
    fn filter(
        &self,
        _context: &ResultContext,
        _row_index: usize,
        row: &mut sift_query_sqlx::RowMap,
    ) -> Result<ResultFilterDecision, ExtensionError> {
        row.shift_remove("name");
        Ok(ResultFilterDecision::Keep)
    }
}

/// Serialises the PostgreSQL tests in this binary.
///
/// Each test creates and drops shared schema objects, so they take turns on a
/// session-scoped advisory lock instead of racing. The returned connection
/// must stay alive for the duration of the test: releasing it releases the
/// lock.
pub(super) async fn serialise(
    pool: &sqlx::PgPool,
) -> Result<sqlx::pool::PoolConnection<sqlx::Postgres>, Box<dyn std::error::Error>> {
    let mut connection = pool.acquire().await?;
    sqlx::query("SELECT pg_advisory_lock(918273645)")
        .execute(&mut *connection)
        .await?;
    Ok(connection)
}
