use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use futures_core::Stream;
use futures_util::StreamExt;
use sift_query_core::{PagingMode, QueryPlan, SearchEntity};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio::time::{Instant, timeout_at};

use crate::{SearchError, TimeoutStage};

pub type SearchStream<T> = Pin<Box<dyn Stream<Item = Result<T, SearchError>> + Send + 'static>>;
pub type ScanFuture<T> =
    Pin<Box<dyn std::future::Future<Output = Result<Vec<T>, SearchError>> + Send>>;

/// Server-owned single-statement scan budget shared by all sqlx backends.
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
    pub fn validate(self) -> Result<Self, sift_query_core::ParamError> {
        if self.max_rows == 0
            || self.max_rows == u32::MAX
            || self.batch_size == 0
            || self.batch_size > 16_384
            || self.timeout.is_zero()
        {
            return Err(sift_query_core::ParamError::InvalidValue {
                field: "scan".to_owned(),
                reason: "scan requires finite row, batch and time budgets".to_owned(),
            });
        }
        Ok(self)
    }
}

pub fn scan_plan<E: SearchEntity>(
    mut plan: QueryPlan<E>,
    policy: ScanPolicy,
) -> Result<QueryPlan<E>, SearchError> {
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

pub fn default_scan_gates() -> (Arc<Semaphore>, Arc<Semaphore>) {
    (Arc::new(Semaphore::new(4)), Arc::new(Semaphore::new(16)))
}

struct BoundScanState<T> {
    active: Arc<Semaphore>,
    waiting: Arc<Semaphore>,
    permit: Option<OwnedSemaphorePermit>,
    stream: SearchStream<T>,
    policy: ScanPolicy,
    deadline: Option<Instant>,
    count: u32,
}

/// Applies scan concurrency gates, row budget, deadline, and cooperative yielding.
pub fn bound_scan<T: Send + 'static>(
    active: Arc<Semaphore>,
    waiting: Arc<Semaphore>,
    stream: SearchStream<T>,
    policy: ScanPolicy,
) -> SearchStream<T> {
    let state = BoundScanState {
        active,
        waiting,
        permit: None,
        stream,
        policy,
        deadline: None,
        count: 0,
    };
    Box::pin(futures_util::stream::try_unfold(state, |mut state| async move {
        let deadline = *state.deadline.get_or_insert_with(|| {
            Instant::now()
                .checked_add(state.policy.timeout)
                .unwrap_or_else(Instant::now)
        });
        if state.permit.is_none() {
            let permit = if let Ok(permit) = Arc::clone(&state.active).try_acquire_owned() {
                permit
            } else {
                let waiter = state
                    .waiting
                    .clone()
                    .try_acquire_owned()
                    .map_err(|_| SearchError::ScanQueueFull)?;
                let permit = timeout_at(deadline, Arc::clone(&state.active).acquire_owned())
                    .await
                    .map_err(|_| SearchError::Timeout(TimeoutStage::Scan))?
                    .map_err(|_| SearchError::ScanQueueFull)?;
                drop(waiter);
                permit
            };
            state.permit = Some(permit);
        }
        let next = timeout_at(deadline, state.stream.next())
            .await
            .map_err(|_| SearchError::Timeout(TimeoutStage::Scan))?;
        let Some(row) = next else {
            return Ok(None);
        };
        let row = row?;
        if state.count == state.policy.max_rows {
            return Err(SearchError::ScanLimitExceeded {
                max_rows: state.policy.max_rows,
            });
        }
        state.count += 1;
        if state.count.is_multiple_of(state.policy.batch_size) {
            tokio::task::yield_now().await;
        }
        Ok(Some((row, state)))
    }))
}
