use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use async_stream::try_stream;
use futures_core::Stream;
use futures_util::{StreamExt, TryStreamExt};
use sift_query_core::{
    GroupScope, PagingMode, QueryInput, QueryPlan, QueryTarget, SearchEntity, ServerScope,
};
use sqlx::FromRow;
use sqlx::postgres::PgRow;
use tokio::time::{Instant, timeout_at};

use crate::cache::RenderRequest;
use crate::execution::io::{close_hard, configure_statement_timeout, finish_statement, prepare};
use crate::postgres::decode::decode_map;
use crate::{PgSearchError, PgSiftQuery, RowMap, TimeoutStage};

enum Outcome {
    Done,
    Failed(sqlx::Error),
    TimedOut,
}

pub type SearchStream<T> = Pin<Box<dyn Stream<Item = Result<T, PgSearchError>> + Send + 'static>>;
pub type ScanFuture<T> =
    Pin<Box<dyn std::future::Future<Output = Result<Vec<T>, PgSearchError>> + Send>>;

/// Server-owned single-statement scan budget. PostgreSQL provides one statement
/// snapshot; `batch_size` controls cooperative handoff, not cursor pagination.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScanPolicy {
    pub max_rows: u32,
    pub batch_size: u32,
    pub timeout: Duration,
}
impl Default for ScanPolicy {
    fn default() -> Self {
        Self {
            max_rows: 10_000,
            batch_size: 256,
            timeout: Duration::from_secs(60),
        }
    }
}
impl ScanPolicy {
    pub fn validate(self) -> Result<Self, PgSearchError> {
        if self.max_rows == 0
            || self.max_rows == u32::MAX
            || self.batch_size == 0
            || self.batch_size > 16_384
            || self.timeout.is_zero()
        {
            return Err(sift_query_core::ParamError::InvalidValue {
                field: "scan".to_owned(),
                reason: "scan requires finite row, batch and time budgets".to_owned(),
            }
            .into());
        }
        Ok(self)
    }
}

impl PgSiftQuery {
    /// Streams a page without count. Early drop closes the checked-out
    /// connection so an active statement or timeout cannot return to the pool.
    pub fn search_stream<E>(
        &self,
        query: impl QueryInput<E>,
        scope: &ServerScope<E>,
    ) -> Result<SearchStream<E>, PgSearchError>
    where
        E: SearchEntity + Send + Unpin + 'static + for<'row> FromRow<'row, PgRow>,
    {
        self.observe_stream::<E, _>("search_stream", || {
            let plan = self.resolve_plan(query, scope, QueryTarget::TypedList, "search_stream")?;
            self.stream_plan(plan, "search_stream")
        })
    }

    pub(crate) fn stream_plan<E>(
        &self,
        plan: QueryPlan<E>,
        operation: &'static str,
    ) -> Result<SearchStream<E>, PgSearchError>
    where
        E: SearchEntity + Send + Unpin + 'static + for<'row> FromRow<'row, PgRow>,
    {
        self.typed_stream(plan, operation, None)
    }

    #[allow(clippy::needless_pass_by_value)]
    fn typed_stream<E>(
        &self,
        plan: QueryPlan<E>,
        operation: &'static str,
        scan_timeout: Option<Duration>,
    ) -> Result<SearchStream<E>, PgSearchError>
    where
        E: SearchEntity + Send + Unpin + 'static + for<'row> FromRow<'row, PgRow>,
    {
        if plan.force_empty || plan.paging.size == 0 {
            return Ok(Box::pin(futures_util::stream::empty()));
        }
        let partition = self.extensions.resolve_partition(&plan, operation)?;
        let prepared = prepare(self.render(
            &plan,
            operation,
            "list",
            partition.as_deref(),
            RenderRequest::List,
        )?)?;
        let executor = self.clone();
        Ok(Box::pin(try_stream! {
            let deadline = scan_timeout.map_or_else(|| executor.deadline::<E>(), |duration| Instant::now() + duration);
            let connection = executor.acquire(deadline).await?;
            let mut connection = configure_statement_timeout(connection, deadline, TimeoutStage::ListQuery).await?;
            connection.close_on_drop();
            let mut rows = sqlx::query_as_with::<_, E, _>(prepared.sql(), prepared.arguments).fetch(&mut *connection);
            let outcome = loop {
                match timeout_at(deadline, rows.try_next()).await {
                    Ok(Ok(Some(row))) => yield row,
                    Ok(Ok(None)) => break Outcome::Done,
                    Ok(Err(error)) => break Outcome::Failed(error),
                    Err(_) => break Outcome::TimedOut,
                }
            };
            drop(rows);
            finish_stream(connection, outcome).await?;
        }))
    }

    pub fn search_map_stream<E>(
        &self,
        query: impl QueryInput<E>,
        scope: &ServerScope<E>,
    ) -> Result<SearchStream<RowMap>, PgSearchError>
    where
        E: SearchEntity + Send + Unpin + 'static,
    {
        self.observe_stream::<E, _>("search_map_stream", || {
            let plan =
                self.resolve_plan(query, scope, QueryTarget::MapPage, "search_map_stream")?;
            self.map_stream(plan, "search_map_stream", None)
        })
    }

    fn map_stream<E>(
        &self,
        plan: QueryPlan<E>,
        operation: &'static str,
        scan_timeout: Option<Duration>,
    ) -> Result<SearchStream<RowMap>, PgSearchError>
    where
        E: SearchEntity + Send + Unpin + 'static,
    {
        if plan.force_empty || plan.paging.size == 0 {
            return Ok(Box::pin(futures_util::stream::empty()));
        }
        let partition = self.extensions.resolve_partition(&plan, operation)?;
        let prepared = prepare(self.render(
            &plan,
            operation,
            "list",
            partition.as_deref(),
            RenderRequest::List,
        )?)?;
        let executor = self.clone();
        Ok(Box::pin(try_stream! {
            let deadline = scan_timeout.map_or_else(|| executor.deadline::<E>(), |duration| Instant::now() + duration);
            let connection = executor.acquire(deadline).await?;
            let mut connection = configure_statement_timeout(connection, deadline, TimeoutStage::ListQuery).await?;
            connection.close_on_drop();
            let mut rows = sqlx::query_with(prepared.sql(), prepared.arguments).fetch(&mut *connection);
            let outcome = loop {
                match timeout_at(deadline, rows.try_next()).await {
                    Ok(Ok(Some(row))) => {
                        let mut row = decode_map(&row, &plan)?;
                        executor.format_map_rows::<E>(std::slice::from_mut(&mut row))?;
                        yield row;
                    }
                    Ok(Ok(None)) => break Outcome::Done,
                    Ok(Err(error)) => break Outcome::Failed(error),
                    Err(_) => break Outcome::TimedOut,
                }
            };
            drop(rows);
            finish_stream(connection, outcome).await?;
        }))
    }

    /// Explicit collecting convenience; prefer `scan_stream` for constant-memory use.
    pub fn scan<E>(
        &self,
        query: impl QueryInput<E>,
        scope: &ServerScope<E>,
        policy: ScanPolicy,
    ) -> Result<ScanFuture<E>, PgSearchError>
    where
        E: SearchEntity + Send + Unpin + 'static + for<'row> FromRow<'row, PgRow>,
    {
        let stream = self.scan_stream(query, scope, policy)?;
        Ok(Box::pin(stream.try_collect()))
    }

    pub fn scan_stream<E>(
        &self,
        query: impl QueryInput<E>,
        scope: &ServerScope<E>,
        policy: ScanPolicy,
    ) -> Result<SearchStream<E>, PgSearchError>
    where
        E: SearchEntity + Send + Unpin + 'static + for<'row> FromRow<'row, PgRow>,
    {
        self.observe_stream::<E, _>("scan", || {
            let policy = policy.validate()?;
            let plan = scan_plan(
                self.resolve_plan(query, scope, QueryTarget::TypedList, "scan")?,
                policy,
            )?;
            let stream = self.typed_stream(plan, "scan", Some(policy.timeout))?;
            Ok(self.bound_scan(stream, policy))
        })
    }

    pub fn scan_map<E>(
        &self,
        query: impl QueryInput<E>,
        scope: &ServerScope<E>,
        policy: ScanPolicy,
    ) -> Result<ScanFuture<RowMap>, PgSearchError>
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
    ) -> Result<SearchStream<RowMap>, PgSearchError>
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
    ) -> Result<SearchStream<RowMap>, PgSearchError>
    where
        E: SearchEntity + Send + Unpin + 'static,
    {
        self.observe_stream::<E, _>("scan_map", || {
            let policy = policy.validate()?;
            let plan = scan_plan(
                self.resolve_plan_with_scopes(
                    query,
                    scope,
                    group_scope,
                    QueryTarget::MapPage,
                    "scan_map",
                )?,
                policy,
            )?;
            let stream = self.map_stream(plan, "scan_map", Some(policy.timeout))?;
            Ok(self.bound_scan(stream, policy))
        })
    }

    pub(crate) fn bound_scan<T: Send + 'static>(
        &self,
        mut stream: SearchStream<T>,
        policy: ScanPolicy,
    ) -> SearchStream<T> {
        let active = Arc::clone(&self.scan_gate);
        let waiting = Arc::clone(&self.scan_wait_gate);
        Box::pin(try_stream! {
            let deadline = Instant::now() + policy.timeout;
            let _permit = if let Ok(permit) = Arc::clone(&active).try_acquire_owned() {
                permit
            } else {
                    let waiter = waiting.try_acquire_owned().map_err(|_| PgSearchError::ScanQueueFull)?;
                    let permit = timeout_at(deadline, active.acquire_owned()).await.map_err(|_| PgSearchError::Timeout(TimeoutStage::Scan))?.map_err(|_| PgSearchError::ScanQueueFull)?;
                    drop(waiter);
                    permit
            };
            let mut count = 0u32;
            while let Some(row) = timeout_at(deadline, stream.next()).await.map_err(|_| PgSearchError::Timeout(TimeoutStage::Scan))? {
                let row = row?;
                if count == policy.max_rows { Err(PgSearchError::ScanLimitExceeded { max_rows: policy.max_rows })?; }
                count += 1;
                yield row;
                if count.is_multiple_of(policy.batch_size) { tokio::task::yield_now().await; }
            }
        })
    }
}

pub(crate) fn scan_plan<E: SearchEntity>(
    mut plan: QueryPlan<E>,
    policy: ScanPolicy,
) -> Result<QueryPlan<E>, PgSearchError> {
    if plan.paging.explicit_page
        || plan.paging.explicit_size
        || plan.paging.explicit_offset
        || plan.paging.explicit_cursor
    {
        return Err(sift_query_core::ParamError::PagingNotAllowed.into());
    }
    if plan.order.is_empty() || plan.meta.stable_order.is_empty() {
        return Err(sift_query_core::ParamError::UnstablePagination.into());
    }
    plan.paging.mode = PagingMode::Page { page: 1 };
    plan.paging.offset = 0;
    plan.paging.size = policy.max_rows + 1;
    Ok(plan)
}

async fn finish_stream(
    connection: sqlx::pool::PoolConnection<sqlx::Postgres>,
    outcome: Outcome,
) -> Result<(), PgSearchError> {
    match outcome {
        Outcome::Done => {
            let (mut connection, ()) =
                finish_statement(connection, Ok(()), TimeoutStage::ListQuery).await?;
            connection.return_to_pool().await;
            Ok(())
        }
        Outcome::Failed(error) => {
            finish_statement(connection, Err::<(), _>(error), TimeoutStage::ListQuery)
                .await
                .map(|_| ())
        }
        Outcome::TimedOut => {
            close_hard(connection).await;
            Err(PgSearchError::Timeout(TimeoutStage::ListQuery))
        }
    }
}
