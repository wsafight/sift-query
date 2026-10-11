use async_stream::try_stream;
use futures_util::{StreamExt, TryStreamExt};
use sift_query_core::{GroupScope, QueryInput, QueryPlan, QueryTarget, SearchEntity, ServerScope};
use sift_query_sqlx_common::{Backend, ScanFuture, ScanPolicy, SearchStream, scan_plan};
use sqlx::sqlite::{Sqlite, SqliteRow};
use sqlx::{AssertSqlSafe, FromRow};
use tokio::time::{Instant, timeout_at};

use crate::{RowMap, SearchError, SqliteBackend, SqliteSiftQuery};

impl SqliteSiftQuery {
    pub fn scan<E>(
        &self,
        query: impl QueryInput<E>,
        scope: &ServerScope<E>,
        policy: ScanPolicy,
    ) -> Result<ScanFuture<E>, SearchError>
    where
        E: SearchEntity + Send + Unpin + 'static + for<'r> FromRow<'r, SqliteRow>,
    {
        let stream = self.scan_stream(query, scope, policy)?;
        Ok(Box::pin(stream.try_collect()))
    }

    pub fn scan_stream<E>(
        &self,
        query: impl QueryInput<E>,
        scope: &ServerScope<E>,
        policy: ScanPolicy,
    ) -> Result<SearchStream<E>, SearchError>
    where
        E: SearchEntity + Send + Unpin + 'static + for<'r> FromRow<'r, SqliteRow>,
    {
        self.observers
            .observe_stream(E::META.entity_name, "scan", || {
                let policy = policy.validate()?;
                let plan = scan_plan(self.plan(query, scope, QueryTarget::TypedList)?, policy)?;
                let stream = self.typed_scan_stream(plan, policy)?;
                Ok(self.bound_scan(stream, policy))
            })
    }

    pub fn scan_map<E>(
        &self,
        query: impl QueryInput<E>,
        scope: &ServerScope<E>,
        policy: ScanPolicy,
    ) -> Result<ScanFuture<RowMap>, SearchError>
    where
        E: SearchEntity + Send + Unpin + 'static,
    {
        let stream = self.scan_map_stream(query, scope, policy)?;
        Ok(Box::pin(stream.try_collect()))
    }

    pub fn scan_map_stream<E>(
        &self,
        query: impl QueryInput<E>,
        scope: &ServerScope<E>,
        policy: ScanPolicy,
    ) -> Result<SearchStream<RowMap>, SearchError>
    where
        E: SearchEntity + Send + Unpin + 'static,
    {
        self.scan_map_stream_with_scopes(query, scope, &GroupScope::unrestricted(), policy)
    }

    pub fn scan_map_stream_with_scopes<E>(
        &self,
        query: impl QueryInput<E>,
        scope: &ServerScope<E>,
        group_scope: &GroupScope<E>,
        policy: ScanPolicy,
    ) -> Result<SearchStream<RowMap>, SearchError>
    where
        E: SearchEntity + Send + Unpin + 'static,
    {
        self.observers
            .observe_stream(E::META.entity_name, "scan_map", || {
                let policy = policy.validate()?;
                let plan = scan_plan(
                    self.plan_with_scopes(query, scope, group_scope, QueryTarget::MapPage)?,
                    policy,
                )?;
                let stream = self.map_scan_stream(plan, policy)?;
                Ok(self.bound_scan(stream, policy))
            })
    }

    #[allow(clippy::needless_pass_by_value)]
    fn typed_scan_stream<E>(
        &self,
        plan: QueryPlan<E>,
        policy: ScanPolicy,
    ) -> Result<SearchStream<E>, SearchError>
    where
        E: SearchEntity + Send + Unpin + 'static + for<'r> FromRow<'r, SqliteRow>,
    {
        if plan.force_empty || plan.paging.size == 0 {
            return Ok(Box::pin(futures_util::stream::empty()));
        }
        let rendered = self.renderer.render_list(&plan)?;
        let args = SqliteBackend::arguments(&rendered.binds)?;
        let pool = self.pool.clone();
        Ok(Box::pin(try_stream! {
            let deadline = Instant::now().checked_add(policy.timeout).unwrap_or_else(Instant::now);
            let mut connection = timeout_at(deadline, pool.acquire())
                .await
                .map_err(|_| SearchError::Timeout(sift_query_sqlx_common::TimeoutStage::PoolAcquire))??;
            connection.close_on_drop();
            let mut rows = sqlx::query_as_with::<Sqlite, E, _>(AssertSqlSafe(rendered.sql), args)
                .fetch(&mut *connection);
            while let Some(row) = rows.try_next().await? {
                yield row;
            }
            drop(rows);
            let _ = timeout_at(deadline, connection.return_to_pool()).await;
        }))
    }

    #[allow(clippy::needless_pass_by_value)]
    fn map_scan_stream<E>(
        &self,
        plan: QueryPlan<E>,
        policy: ScanPolicy,
    ) -> Result<SearchStream<RowMap>, SearchError>
    where
        E: SearchEntity + Send + Unpin + 'static,
    {
        if plan.force_empty || plan.paging.size == 0 {
            return Ok(Box::pin(futures_util::stream::empty()));
        }
        let rendered = self.renderer.render_list(&plan)?;
        let args = SqliteBackend::arguments(&rendered.binds)?;
        let pool = self.pool.clone();
        let executor = self.clone();
        Ok(Box::pin(try_stream! {
            let deadline = Instant::now().checked_add(policy.timeout).unwrap_or_else(Instant::now);
            let mut connection = timeout_at(deadline, pool.acquire())
                .await
                .map_err(|_| SearchError::Timeout(sift_query_sqlx_common::TimeoutStage::PoolAcquire))??;
            connection.close_on_drop();
            let mut rows = sqlx::query_with(AssertSqlSafe(rendered.sql), args).fetch(&mut *connection);
            while let Some(row) = rows.try_next().await? {
                let mut row = crate::decode_map::<E>(&row, &plan)?;
                executor.format_map_rows::<E>(std::slice::from_mut(&mut row))?;
                yield row;
            }
            drop(rows);
            let _ = timeout_at(deadline, connection.return_to_pool()).await;
        }))
    }

    pub(crate) fn bound_scan<T: Send + 'static>(
        &self,
        mut stream: SearchStream<T>,
        policy: ScanPolicy,
    ) -> SearchStream<T> {
        let active = Arc::clone(&self.scan_gate);
        let waiting = Arc::clone(&self.scan_wait_gate);
        Box::pin(try_stream! {
            let deadline = Instant::now().checked_add(policy.timeout).unwrap_or_else(Instant::now);
            let _permit = if let Ok(permit) = Arc::clone(&active).try_acquire_owned() {
                permit
            } else {
                let waiter = match waiting.try_acquire_owned() {
                    Ok(permit) => permit,
                    Err(_) => Err::<tokio::sync::OwnedSemaphorePermit, _>(SearchError::ScanQueueFull)?,
                };
                let permit = timeout_at(deadline, active.acquire_owned())
                    .await
                    .map_err(|_| SearchError::Timeout(sift_query_sqlx_common::TimeoutStage::Scan))?
                    .map_err(|_| SearchError::ScanQueueFull)?;
                drop(waiter);
                permit
            };
            let mut count = 0_u32;
            while let Some(row) = timeout_at(deadline, stream.next())
                .await
                .map_err(|_| SearchError::Timeout(sift_query_sqlx_common::TimeoutStage::Scan))?
            {
                let row = row?;
                if count == policy.max_rows {
                    Err(SearchError::ScanLimitExceeded { max_rows: policy.max_rows })?;
                }
                count += 1;
                yield row;
                if count.is_multiple_of(policy.batch_size) {
                    tokio::task::yield_now().await;
                }
            }
        })
    }
}

use std::sync::Arc;
