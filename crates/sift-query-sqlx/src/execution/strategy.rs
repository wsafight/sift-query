use sift_query_core::{QueryPlan, SearchEntity, SearchOptions};
use sqlx::FromRow;
use sqlx::Postgres;
use sqlx::pool::PoolConnection;
use sqlx::postgres::PgRow;
use tokio::time::{Instant, timeout_at};

use crate::execution::io::{PreparedPgQuery, close_hard, fetch_cluster, fetch_maps, fetch_typed};
use crate::{Page, PgSearchError, PgSiftQuery, RowMap, TimeoutStage};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ExecutionStrategy {
    #[default]
    SequentialBestEffort,
    ParallelBestEffort,
    Snapshot(SnapshotIsolation),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SnapshotIsolation {
    #[default]
    RepeatableRead,
    Serializable,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CountStrategy {
    #[default]
    Separate,
    WindowWhenPossible,
}

impl PgSiftQuery {
    pub(crate) async fn execute_typed_pair<E, R>(
        &self,
        list: Option<PreparedPgQuery>,
        cluster: PreparedPgQuery,
        options: Option<&SearchOptions<E>>,
        deadline: Instant,
    ) -> Result<Page<R>, PgSearchError>
    where
        E: SearchEntity,
        R: Send + Unpin + for<'row> FromRow<'row, PgRow>,
    {
        match self.execution.strategy {
            ExecutionStrategy::SequentialBestEffort => {
                let connection = self.acquire(deadline).await?;
                let (connection, data) = match list {
                    Some(rendered) => {
                        fetch_typed::<R>(connection, rendered, deadline, TimeoutStage::ListQuery)
                            .await?
                    }
                    None => (connection, Vec::new()),
                };
                let (_, (total_count, summaries)) =
                    fetch_cluster(connection, cluster, options, E::META, deadline).await?;
                Ok(Page {
                    data,
                    total_count,
                    summaries,
                    next_cursor: None,
                })
            }
            ExecutionStrategy::ParallelBestEffort if list.is_some() => {
                let permit = timeout_at(deadline, self.parallel_gate.acquire())
                    .await
                    .map_err(|_| PgSearchError::Timeout(TimeoutStage::PoolAcquire))?
                    .map_err(|_| PgSearchError::ParallelGateClosed)?;
                let list = list.expect("checked above");
                let list_future = async {
                    let connection = self.acquire(deadline).await?;
                    fetch_typed::<R>(connection, list, deadline, TimeoutStage::ListQuery).await
                };
                let cluster_future = async {
                    let connection = self.acquire(deadline).await?;
                    fetch_cluster(connection, cluster, options, E::META, deadline).await
                };
                let (list_result, cluster_result) =
                    futures_util::future::join(list_future, cluster_future).await;
                drop(permit);
                let (_, data) = list_result?;
                let (_, (total_count, summaries)) = cluster_result?;
                Ok(Page {
                    data,
                    total_count,
                    summaries,
                    next_cursor: None,
                })
            }
            ExecutionStrategy::ParallelBestEffort => {
                let connection = self.acquire(deadline).await?;
                let (_, (total_count, summaries)) =
                    fetch_cluster(connection, cluster, options, E::META, deadline).await?;
                Ok(Page {
                    data: Vec::new(),
                    total_count,
                    summaries,
                    next_cursor: None,
                })
            }
            ExecutionStrategy::Snapshot(isolation) => {
                let mut connection = self.acquire(deadline).await?;
                connection.close_on_drop();
                let connection = begin_snapshot(connection, isolation, deadline).await?;
                let (connection, data) = match list {
                    Some(rendered) => {
                        fetch_typed::<R>(connection, rendered, deadline, TimeoutStage::ListQuery)
                            .await?
                    }
                    None => (connection, Vec::new()),
                };
                let (connection, (total_count, summaries)) =
                    fetch_cluster(connection, cluster, options, E::META, deadline).await?;
                let mut connection = commit(connection, deadline).await?;
                connection.return_to_pool().await;
                Ok(Page {
                    data,
                    total_count,
                    summaries,
                    next_cursor: None,
                })
            }
        }
    }

    pub(crate) async fn execute_map_pair<E: SearchEntity>(
        &self,
        plan: &QueryPlan<E>,
        list: Option<PreparedPgQuery>,
        cluster: PreparedPgQuery,
        options: Option<&SearchOptions<E>>,
        deadline: Instant,
    ) -> Result<Page<RowMap>, PgSearchError> {
        match self.execution.strategy {
            ExecutionStrategy::SequentialBestEffort => {
                let connection = self.acquire(deadline).await?;
                let (connection, data) = match list {
                    Some(rendered) => fetch_maps(connection, rendered, plan, deadline).await?,
                    None => (connection, Vec::new()),
                };
                let (_, (total_count, summaries)) =
                    fetch_cluster(connection, cluster, options, E::META, deadline).await?;
                Ok(Page {
                    data,
                    total_count,
                    summaries,
                    next_cursor: None,
                })
            }
            ExecutionStrategy::ParallelBestEffort if list.is_some() => {
                let permit = timeout_at(deadline, self.parallel_gate.acquire())
                    .await
                    .map_err(|_| PgSearchError::Timeout(TimeoutStage::PoolAcquire))?
                    .map_err(|_| PgSearchError::ParallelGateClosed)?;
                let list = list.expect("checked above");
                let list_future = async {
                    let connection = self.acquire(deadline).await?;
                    fetch_maps(connection, list, plan, deadline).await
                };
                let cluster_future = async {
                    let connection = self.acquire(deadline).await?;
                    fetch_cluster(connection, cluster, options, E::META, deadline).await
                };
                let (list_result, cluster_result) =
                    futures_util::future::join(list_future, cluster_future).await;
                drop(permit);
                let (_, data) = list_result?;
                let (_, (total_count, summaries)) = cluster_result?;
                Ok(Page {
                    data,
                    total_count,
                    summaries,
                    next_cursor: None,
                })
            }
            ExecutionStrategy::ParallelBestEffort => {
                let connection = self.acquire(deadline).await?;
                let (_, (total_count, summaries)) =
                    fetch_cluster(connection, cluster, options, E::META, deadline).await?;
                Ok(Page {
                    data: Vec::new(),
                    total_count,
                    summaries,
                    next_cursor: None,
                })
            }
            ExecutionStrategy::Snapshot(isolation) => {
                let mut connection = self.acquire(deadline).await?;
                connection.close_on_drop();
                let connection = begin_snapshot(connection, isolation, deadline).await?;
                let (connection, data) = match list {
                    Some(rendered) => fetch_maps(connection, rendered, plan, deadline).await?,
                    None => (connection, Vec::new()),
                };
                let (connection, (total_count, summaries)) =
                    fetch_cluster(connection, cluster, options, E::META, deadline).await?;
                let mut connection = commit(connection, deadline).await?;
                connection.return_to_pool().await;
                Ok(Page {
                    data,
                    total_count,
                    summaries,
                    next_cursor: None,
                })
            }
        }
    }
}

async fn begin_snapshot(
    connection: PoolConnection<Postgres>,
    isolation: SnapshotIsolation,
    deadline: Instant,
) -> Result<PoolConnection<Postgres>, PgSearchError> {
    let sql = match isolation {
        SnapshotIsolation::RepeatableRead => "BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY",
        SnapshotIsolation::Serializable => "BEGIN ISOLATION LEVEL SERIALIZABLE READ ONLY",
    };
    execute_control(connection, sql, deadline).await
}

async fn commit(
    connection: PoolConnection<Postgres>,
    deadline: Instant,
) -> Result<PoolConnection<Postgres>, PgSearchError> {
    execute_control(connection, "COMMIT", deadline).await
}

async fn execute_control(
    mut connection: PoolConnection<Postgres>,
    sql: &'static str,
    deadline: Instant,
) -> Result<PoolConnection<Postgres>, PgSearchError> {
    let result = timeout_at(deadline, sqlx::query(sql).execute(&mut *connection)).await;
    let Ok(result) = result else {
        close_hard(connection).await;
        return Err(PgSearchError::Timeout(TimeoutStage::ClusterQuery));
    };
    match result {
        Ok(_) => Ok(connection),
        Err(error) => {
            close_hard(connection).await;
            Err(PgSearchError::Sql(error))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    #[ignore = "requires a disposable PostgreSQL instance in DATABASE_URL"]
    async fn snapshot_control_faults_discard_connection_and_release_pool_slot() {
        let url = std::env::var("DATABASE_URL").unwrap();
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(1)
            .connect(&url)
            .await
            .unwrap();
        let admin = sqlx::postgres::PgPoolOptions::new()
            .max_connections(1)
            .connect(&url)
            .await
            .unwrap();
        for control in ["COMMIT", "ROLLBACK"] {
            let mut connection = pool.acquire().await.unwrap();
            connection.close_on_drop();
            let mut connection = begin_snapshot(
                connection,
                SnapshotIsolation::RepeatableRead,
                Instant::now() + Duration::from_secs(5),
            )
            .await
            .unwrap();
            let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
                .fetch_one(&mut *connection)
                .await
                .unwrap();
            // Terminate only the connection acquired by this test.
            assert!(
                sqlx::query_scalar::<_, bool>("SELECT pg_terminate_backend($1)")
                    .bind(pid)
                    .fetch_one(&admin)
                    .await
                    .unwrap()
            );
            assert!(
                execute_control(connection, control, Instant::now() + Duration::from_secs(5))
                    .await
                    .is_err()
            );
            assert_clean(&pool).await;
        }
        let mut connection = pool.acquire().await.unwrap();
        connection.close_on_drop();
        let connection = begin_snapshot(
            connection,
            SnapshotIsolation::Serializable,
            Instant::now() + Duration::from_secs(5),
        )
        .await
        .unwrap();
        assert!(matches!(
            commit(connection, Instant::now() - Duration::from_secs(1)).await,
            Err(PgSearchError::Timeout(_))
        ));
        assert_clean(&pool).await;
        pool.close().await;
        admin.close().await;
    }

    async fn assert_clean(pool: &sqlx::PgPool) {
        let read_only: String = tokio::time::timeout(
            Duration::from_secs(5),
            sqlx::query_scalar("SHOW transaction_read_only").fetch_one(pool),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(read_only, "off");
    }
}
