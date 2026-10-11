use sift_query_core::{QueryPlan, SearchEntity};
use sqlx::postgres::PgRow;
use sqlx::{FromRow, Postgres, Row};
use tokio::time::{Instant, timeout_at};

use crate::execution::io::{
    PreparedPgQuery, close_hard, configure_statement_timeout, fetch_cluster, finish_statement,
};
use crate::postgres::decode::decode_map;
use crate::{Page, PgSearchError, PgSiftQuery, RowMap, TimeoutStage};

impl PgSiftQuery {
    pub(crate) async fn execute_window_typed<E, R>(
        &self,
        window: PreparedPgQuery,
        fallback: PreparedPgQuery,
        deadline: Instant,
    ) -> Result<Page<R>, PgSearchError>
    where
        E: SearchEntity,
        R: Send + Unpin + for<'row> FromRow<'row, PgRow>,
    {
        let connection = self.acquire(deadline).await?;
        let (connection, rows) = fetch_window_rows(connection, window, deadline).await?;
        let total_count = rows
            .first()
            .map(|row| row.try_get::<i64, _>("sq_total_count"))
            .transpose()?;
        let data = rows
            .iter()
            .map(R::from_row)
            .collect::<Result<Vec<_>, _>>()?;
        let total_count = match total_count {
            Some(count) => count,
            None => {
                fetch_cluster::<E>(connection, fallback, None, E::META, deadline)
                    .await?
                    .1
                    .0
            }
        };
        Ok(Page {
            data,
            total_count,
            summaries: Vec::new(),
            next_cursor: None,
        })
    }

    pub(crate) async fn execute_window_map<E: SearchEntity>(
        &self,
        plan: &QueryPlan<E>,
        window: PreparedPgQuery,
        fallback: PreparedPgQuery,
        deadline: Instant,
    ) -> Result<Page<RowMap>, PgSearchError> {
        let connection = self.acquire(deadline).await?;
        let (connection, rows) = fetch_window_rows(connection, window, deadline).await?;
        let total_count = rows
            .first()
            .map(|row| row.try_get::<i64, _>("sq_total_count"))
            .transpose()?;
        let data = rows
            .iter()
            .map(|row| decode_map(row, plan))
            .collect::<Result<Vec<_>, _>>()?;
        let total_count = match total_count {
            Some(count) => count,
            None => {
                fetch_cluster::<E>(connection, fallback, None, E::META, deadline)
                    .await?
                    .1
                    .0
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

async fn fetch_window_rows(
    connection: sqlx::pool::PoolConnection<Postgres>,
    prepared: PreparedPgQuery,
    deadline: Instant,
) -> Result<(sqlx::pool::PoolConnection<Postgres>, Vec<PgRow>), PgSearchError> {
    let mut connection =
        configure_statement_timeout(connection, deadline, TimeoutStage::ListQuery).await?;
    let result = timeout_at(
        deadline,
        sqlx::query_with(prepared.sql(), prepared.arguments).fetch_all(&mut *connection),
    )
    .await;
    let Ok(result) = result else {
        close_hard(connection).await;
        return Err(PgSearchError::Timeout(TimeoutStage::ListQuery));
    };
    finish_statement(connection, result, TimeoutStage::ListQuery).await
}
