use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use futures_core::Stream;
use sift_query_core::{PagingMode, QueryPlan, SearchEntity};
use tokio::sync::Semaphore;

use crate::SearchError;

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
    pub fn validate(self) -> Result<Self, SearchError> {
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
