use sift_query_core::{QueryPlan, SearchEntity};
use sqlx::postgres::PgRow;
use sqlx::{FromRow, Postgres, Row};
use tokio::time::{Instant, timeout_at};

use crate::execution::io::{
    PreparedPgQuery, close_hard, configure_statement_timeout, execute_cluster_raw,
    finish_statement, finish_statement_err,
};
use crate::postgres::decode::decode_map;
use crate::{Page, PgSearchError, PgSiftQuery, RowMap, TimeoutStage};

impl PgSiftQuery {
    pub(crate) async fn execute_window_typed<E, R, F>(
        &self,
        window: PreparedPgQuery,
        fallback: F,
        deadline: Instant,
    ) -> Result<Page<R>, PgSearchError>
    where
        E: SearchEntity,
        R: Send + Unpin + for<'row> FromRow<'row, PgRow>,
        F: FnOnce() -> Result<PreparedPgQuery, PgSearchError>,
    {
        let connection = self.acquire(deadline).await?;
        let connection =
            configure_statement_timeout(connection, deadline, TimeoutStage::ListQuery).await?;
        let (connection, rows) = match fetch_window_rows_raw(connection, window, deadline).await? {
            (connection, Ok(rows)) => (connection, rows),
            (connection, Err(error)) => {
                finish_statement::<()>(connection, Err(error), TimeoutStage::ListQuery).await?;
                unreachable!()
            }
        };
        let decoded = rows
            .first()
            .map(|row| row.try_get::<i64, _>("sq_total_count"))
            .transpose()
            .and_then(|total_count| {
                let data = rows
                    .iter()
                    .map(R::from_row)
                    .collect::<Result<Vec<_>, _>>()?;
                Ok((data, total_count))
            });
        let (data, total_count) = match decoded {
            Ok((data, Some(count))) => {
                finish_statement(connection, Ok(()), TimeoutStage::ListQuery).await?;
                (data, count)
            }
            Ok((data, None)) => {
                let fallback = match fallback() {
                    Ok(prepared) => prepared,
                    Err(error) => {
                        finish_statement_err::<()>(
                            connection,
                            Err(error),
                            TimeoutStage::ClusterQuery,
                        )
                        .await?;
                        unreachable!()
                    }
                };
                let (connection, cluster) =
                    execute_cluster_raw::<E>(connection, fallback, None, E::META, deadline).await?;
                let (_, (count, _)) =
                    finish_statement_err(connection, cluster, TimeoutStage::ClusterQuery).await?;
                (data, count)
            }
            Err(error) => {
                finish_statement::<()>(connection, Err(error), TimeoutStage::ListQuery).await?;
                unreachable!()
            }
        };
        Ok(Page {
            data,
            total_count,
            summaries: Vec::new(),
            next_cursor: None,
        })
    }

    pub(crate) async fn execute_window_map<E: SearchEntity, F>(
        &self,
        plan: &QueryPlan<E>,
        window: PreparedPgQuery,
        fallback: F,
        deadline: Instant,
    ) -> Result<Page<RowMap>, PgSearchError>
    where
        F: FnOnce() -> Result<PreparedPgQuery, PgSearchError>,
    {
        let connection = self.acquire(deadline).await?;
        let connection =
            configure_statement_timeout(connection, deadline, TimeoutStage::ListQuery).await?;
        let (connection, rows) = match fetch_window_rows_raw(connection, window, deadline).await? {
            (connection, Ok(rows)) => (connection, rows),
            (connection, Err(error)) => {
                finish_statement::<()>(connection, Err(error), TimeoutStage::ListQuery).await?;
                unreachable!()
            }
        };
        let decoded: Result<(Vec<RowMap>, Option<i64>), PgSearchError> = rows
            .first()
            .map(|row| row.try_get::<i64, _>("sq_total_count"))
            .transpose()
            .map_err(PgSearchError::Sql)
            .and_then(|total_count| {
                let data = rows
                    .iter()
                    .map(|row| decode_map(row, plan))
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(PgSearchError::Decode)?;
                Ok((data, total_count))
            });
        let (data, total_count) = match decoded {
            Ok((data, Some(count))) => {
                finish_statement(connection, Ok(()), TimeoutStage::ListQuery).await?;
                (data, count)
            }
            Ok((data, None)) => {
                let fallback = match fallback() {
                    Ok(prepared) => prepared,
                    Err(error) => {
                        finish_statement_err::<()>(
                            connection,
                            Err(error),
                            TimeoutStage::ClusterQuery,
                        )
                        .await?;
                        unreachable!()
                    }
                };
                let (connection, cluster) =
                    execute_cluster_raw::<E>(connection, fallback, None, E::META, deadline).await?;
                let (_, (count, _)) =
                    finish_statement_err(connection, cluster, TimeoutStage::ClusterQuery).await?;
                (data, count)
            }
            Err(error) => {
                finish_statement_err::<()>(connection, Err(error), TimeoutStage::ListQuery).await?;
                unreachable!()
            }
        };
        Ok(Page {
            data,
            total_count,
            summaries: Vec::new(),
            next_cursor: None,
        })
    }
}

async fn fetch_window_rows_raw(
    mut connection: sqlx::pool::PoolConnection<Postgres>,
    prepared: PreparedPgQuery,
    deadline: Instant,
) -> Result<(sqlx::pool::PoolConnection<Postgres>, Result<Vec<PgRow>, sqlx::Error>), PgSearchError> {
    let result = timeout_at(
        deadline,
        sqlx::query_with(prepared.sql(), prepared.arguments).fetch_all(&mut *connection),
    )
    .await;
    let Ok(result) = result else {
        close_hard(connection).await;
        return Err(PgSearchError::Timeout(TimeoutStage::ListQuery));
    };
    Ok((connection, result))
}
