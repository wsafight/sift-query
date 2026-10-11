use std::sync::Arc;
use std::time::Duration;

use sift_query_core::{QueryPlan, SearchEntity, SearchOptions};
use sqlx::pool::PoolConnection;
use sqlx::postgres::{PgArguments, PgConnection, PgRow};
use sqlx::{AssertSqlSafe, Connection, Decode, FromRow, Postgres, Row, Type};
use tokio::time::{Instant, timeout_at};

use crate::postgres::bind::build_arguments;
use crate::postgres::decode::{decode_map, decode_summaries};
use crate::{PgSearchError, RenderedPgQuery, RowMap, SummaryValue, TimeoutStage};

pub(crate) struct PreparedPgQuery {
    pub(crate) sql: Arc<str>,
    pub(crate) arguments: PgArguments,
    pub(crate) shape_id: crate::QueryShapeId,
}

impl PreparedPgQuery {
    /// The SQL text marked as safe to execute.
    ///
    /// `sqlx` 0.9 requires dynamic SQL to be wrapped in [`AssertSqlSafe`]. That
    /// assertion holds here because the text is produced by [`PgRenderer`]
    /// from static entity metadata:
    ///
    /// * Identifiers come from `FieldMeta::column` and `EntityMeta::from`,
    ///   which the derive rejects if they contain `;`, `--`, `/*`, or `$`.
    /// * Every user-supplied value becomes a bind parameter, never
    ///   interpolated text.
    /// * Custom operator and aggregate templates are checked for the same
    ///   forbidden tokens when they are declared.
    /// * Partition names are validated as SQL identifiers.
    ///
    /// A [`SqlInterceptor`] may return arbitrary SQL, which is why that trait
    /// is documented as trusted server-side code. Interceptor output reaches
    /// `sqlx` through this same assertion, so an interceptor is responsible for
    /// the safety of whatever it returns.
    ///
    /// [`PgRenderer`]: crate::PgRenderer
    /// [`SqlInterceptor`]: crate::SqlInterceptor
    pub(crate) fn sql(&self) -> AssertSqlSafe<Arc<str>> {
        AssertSqlSafe(Arc::clone(&self.sql))
    }
}

pub(crate) fn prepare(rendered: RenderedPgQuery) -> Result<PreparedPgQuery, PgSearchError> {
    let arguments = build_arguments(&rendered.binds)?;
    Ok(PreparedPgQuery {
        sql: rendered.sql,
        arguments,
        shape_id: rendered.shape_id,
    })
}

pub(crate) async fn fetch_typed<E>(
    connection: PoolConnection<Postgres>,
    prepared: PreparedPgQuery,
    deadline: Instant,
    stage: TimeoutStage,
) -> Result<(PoolConnection<Postgres>, Vec<E>), PgSearchError>
where
    E: Send + Unpin + for<'row> FromRow<'row, PgRow>,
{
    let mut connection = configure_statement_timeout(connection, deadline, stage).await?;
    tracing::debug!(
        query_shape_id = prepared.shape_id.0,
        "executing SiftQuery list"
    );
    let started = Instant::now();
    let result = timeout_at(
        deadline,
        sqlx::query_as_with::<Postgres, E, _>(prepared.sql(), prepared.arguments)
            .fetch_all(&mut *connection),
    )
    .await;
    let Ok(result) = result else {
        close_hard(connection).await;
        return Err(PgSearchError::Timeout(stage));
    };
    let (connection, rows) = finish_statement(connection, result, stage).await?;
    tracing::debug!(
        list_duration_us = elapsed_micros(started),
        rows_returned = rows.len(),
        "completed SiftQuery list"
    );
    Ok((connection, rows))
}

pub(crate) async fn fetch_maps<E: SearchEntity>(
    connection: PoolConnection<Postgres>,
    prepared: PreparedPgQuery,
    plan: &QueryPlan<E>,
    deadline: Instant,
) -> Result<(PoolConnection<Postgres>, Vec<RowMap>), PgSearchError> {
    let mut connection =
        configure_statement_timeout(connection, deadline, TimeoutStage::ListQuery).await?;
    tracing::debug!(
        query_shape_id = prepared.shape_id.0,
        "executing SiftQuery map list"
    );
    let started = Instant::now();
    let result = timeout_at(
        deadline,
        sqlx::query_with(prepared.sql(), prepared.arguments).fetch_all(&mut *connection),
    )
    .await;
    let Ok(result) = result else {
        close_hard(connection).await;
        return Err(PgSearchError::Timeout(TimeoutStage::ListQuery));
    };
    let (connection, rows) = finish_statement(connection, result, TimeoutStage::ListQuery).await?;
    let list_duration = elapsed_micros(started);
    let decode_started = Instant::now();
    let decoded = rows
        .iter()
        .map(|row| decode_map(row, plan))
        .collect::<Result<Vec<_>, _>>()
        .map_err(PgSearchError::Decode)?;
    tracing::debug!(
        list_duration_us = list_duration,
        decode_duration_us = elapsed_micros(decode_started),
        rows_returned = decoded.len(),
        "completed SiftQuery map list"
    );
    Ok((connection, decoded))
}

pub(crate) async fn fetch_cluster<E: SearchEntity>(
    connection: PoolConnection<Postgres>,
    prepared: PreparedPgQuery,
    options: Option<&SearchOptions<E>>,
    meta: &'static sift_query_core::EntityMeta,
    deadline: Instant,
) -> Result<(PoolConnection<Postgres>, (i64, Vec<SummaryValue>)), PgSearchError> {
    let mut connection =
        configure_statement_timeout(connection, deadline, TimeoutStage::ClusterQuery).await?;
    tracing::debug!(
        query_shape_id = prepared.shape_id.0,
        "executing SiftQuery cluster"
    );
    let started = Instant::now();
    let result = timeout_at(
        deadline,
        sqlx::query_with(prepared.sql(), prepared.arguments).fetch_one(&mut *connection),
    )
    .await;
    let Ok(result) = result else {
        close_hard(connection).await;
        return Err(PgSearchError::Timeout(TimeoutStage::ClusterQuery));
    };
    let (connection, row) =
        finish_statement(connection, result, TimeoutStage::ClusterQuery).await?;
    let cluster_duration = elapsed_micros(started);
    let decode_started = Instant::now();
    let total_count = row.try_get::<i64, _>("sq_count")?;
    let summaries = decode_summaries(&row, options, meta)?;
    tracing::debug!(
        cluster_duration_us = cluster_duration,
        decode_duration_us = elapsed_micros(decode_started),
        "completed SiftQuery cluster"
    );
    Ok((connection, (total_count, summaries)))
}

pub(crate) async fn fetch_sum<T>(
    connection: PoolConnection<Postgres>,
    prepared: PreparedPgQuery,
    deadline: Instant,
) -> Result<(PoolConnection<Postgres>, Option<T>), PgSearchError>
where
    T: Send + Unpin + for<'row> Decode<'row, Postgres> + Type<Postgres>,
{
    let mut connection =
        configure_statement_timeout(connection, deadline, TimeoutStage::ClusterQuery).await?;
    let started = Instant::now();
    let result = timeout_at(
        deadline,
        sqlx::query_scalar_with::<Postgres, Option<T>, _>(prepared.sql(), prepared.arguments)
            .fetch_one(&mut *connection),
    )
    .await;
    let Ok(result) = result else {
        close_hard(connection).await;
        return Err(PgSearchError::Timeout(TimeoutStage::ClusterQuery));
    };
    let result = finish_statement(connection, result, TimeoutStage::ClusterQuery).await;
    tracing::debug!(
        cluster_duration_us = elapsed_micros(started),
        "completed SiftQuery sum"
    );
    result
}

/// Executes a list query on a caller-owned PostgreSQL connection. The
/// connection is deliberately borrowed and is never committed, rolled back,
/// or returned to a pool by this crate.
pub(crate) async fn fetch_typed_external<E>(
    connection: &mut PgConnection,
    prepared: PreparedPgQuery,
    deadline: Instant,
    stage: TimeoutStage,
) -> Result<Vec<E>, PgSearchError>
where
    E: Send + Unpin + for<'row> FromRow<'row, PgRow>,
{
    let result = timeout_at(
        deadline,
        sqlx::query_as_with::<Postgres, E, _>(prepared.sql(), prepared.arguments)
            .fetch_all(&mut *connection),
    )
    .await;
    match result {
        Ok(Ok(rows)) => Ok(rows),
        Ok(Err(error)) => Err(external_sql_error(error, stage)),
        Err(_) => Err(PgSearchError::Timeout(stage)),
    }
}

pub(crate) async fn fetch_maps_external<E: SearchEntity>(
    connection: &mut PgConnection,
    prepared: PreparedPgQuery,
    plan: &QueryPlan<E>,
    deadline: Instant,
) -> Result<Vec<RowMap>, PgSearchError> {
    let result = timeout_at(
        deadline,
        sqlx::query_with(prepared.sql(), prepared.arguments).fetch_all(&mut *connection),
    )
    .await;
    let rows = match result {
        Ok(Ok(rows)) => rows,
        Ok(Err(error)) => return Err(external_sql_error(error, TimeoutStage::ListQuery)),
        Err(_) => return Err(PgSearchError::Timeout(TimeoutStage::ListQuery)),
    };
    rows.iter()
        .map(|row| decode_map(row, plan))
        .collect::<Result<Vec<_>, _>>()
        .map_err(PgSearchError::Decode)
}

pub(crate) async fn fetch_cluster_external<E: SearchEntity>(
    connection: &mut PgConnection,
    prepared: PreparedPgQuery,
    options: Option<&SearchOptions<E>>,
    meta: &'static sift_query_core::EntityMeta,
    deadline: Instant,
) -> Result<(i64, Vec<SummaryValue>), PgSearchError> {
    let result = timeout_at(
        deadline,
        sqlx::query_with(prepared.sql(), prepared.arguments).fetch_one(&mut *connection),
    )
    .await;
    let row = match result {
        Ok(Ok(row)) => row,
        Ok(Err(error)) => return Err(external_sql_error(error, TimeoutStage::ClusterQuery)),
        Err(_) => return Err(PgSearchError::Timeout(TimeoutStage::ClusterQuery)),
    };
    let total_count = row.try_get::<i64, _>("sq_count")?;
    let summaries = decode_summaries(&row, options, meta)?;
    Ok((total_count, summaries))
}

pub(crate) async fn configure_statement_timeout(
    mut connection: PoolConnection<Postgres>,
    deadline: Instant,
    stage: TimeoutStage,
) -> Result<PoolConnection<Postgres>, PgSearchError> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    let milliseconds = remaining.as_millis().saturating_sub(10).max(1);
    let milliseconds = u64::try_from(milliseconds).unwrap_or(u64::MAX);
    let result = timeout_at(
        deadline,
        sqlx::query_scalar::<_, String>("SELECT set_config('statement_timeout', $1, false)")
            .bind(milliseconds.to_string())
            .fetch_one(&mut *connection),
    )
    .await;
    let Ok(result) = result else {
        close_hard(connection).await;
        return Err(PgSearchError::Timeout(stage));
    };
    result.map_err(PgSearchError::Sql)?;
    Ok(connection)
}

pub(crate) async fn finish_statement<T>(
    mut connection: PoolConnection<Postgres>,
    result: Result<T, sqlx::Error>,
    stage: TimeoutStage,
) -> Result<(PoolConnection<Postgres>, T), PgSearchError> {
    // The statement timeout must be cleared before the connection goes back
    // to the pool. If the reset itself fails or times out, the connection is
    // closed rather than handed back with a live server-side timeout.
    let reset = tokio::time::timeout(
        Duration::from_secs(1),
        sqlx::query("SELECT set_config('statement_timeout', '0', false)").execute(&mut *connection),
    )
    .await;
    let reset_error = match reset {
        Ok(Ok(_)) => None,
        Ok(Err(error)) => Some(PgSearchError::Sql(error)),
        Err(_) => Some(PgSearchError::Timeout(stage)),
    };
    if let Some(error) = reset_error {
        close_hard(connection).await;
        return Err(error);
    }
    match result {
        Ok(value) => Ok((connection, value)),
        Err(error) if is_statement_timeout(&error) => Err(PgSearchError::Timeout(stage)),
        Err(error) => Err(PgSearchError::Sql(error)),
    }
}

fn is_statement_timeout(error: &sqlx::Error) -> bool {
    error
        .as_database_error()
        .and_then(sqlx::error::DatabaseError::code)
        .is_some_and(|code| code == "57014")
}

fn external_sql_error(error: sqlx::Error, stage: TimeoutStage) -> PgSearchError {
    if is_statement_timeout(&error) {
        PgSearchError::Timeout(stage)
    } else {
        PgSearchError::Sql(error)
    }
}

pub(crate) async fn close_hard(connection: PoolConnection<Postgres>) {
    if connection.detach().close_hard().await.is_err() {
        tracing::warn!(
            error_category = "connection_close",
            "failed to close timed-out PostgreSQL connection"
        );
    }
}

fn elapsed_micros(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX)
}
#[cfg(test)]
mod tests;
