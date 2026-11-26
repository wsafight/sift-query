//! PostgreSQL execution configuration and query entry points.
use std::sync::Arc;
use std::time::Duration;

use sift_query_core::{
    ConfigError, FieldRef, GroupScope, ParamError, ParamParser, QueryBuilder, QueryInput,
    QueryPlan, QueryPolicy, QueryTarget, RawParams, SearchEntity, SearchOptions, ServerScope,
    SummableKind,
};
use sqlx::postgres::{PgPool, PgRow};
use sqlx::{Decode, FromRow, Postgres, Type};
use tokio::sync::Semaphore;
use tokio::time::{Instant, timeout_at};

use crate::cache::RenderRequest;
use crate::execution::io::{fetch_cluster, fetch_maps, fetch_sum, fetch_typed, prepare};
use crate::{
    CountStrategy, ExecutionStrategy, PartitionResolver, PgRenderer, PgSearchError, PgSumType,
    RenderedPgQuery, ResultFilter, ShapeCacheStats, SqlInterceptor, TimeoutStage,
};

mod aggregate;
mod config;
pub(crate) mod cursor_page;
pub(crate) mod external;
pub(crate) mod io;
mod prepare;
pub(crate) mod projection;
mod search;
pub(crate) mod strategy;
pub(crate) mod stream;
pub(crate) mod window;

pub use sift_query_sqlx_common::{CursorPage, Page, RowMap, SummaryValue};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PgExecutionPolicy {
    pub timeout: Duration,
    pub strategy: ExecutionStrategy,
    pub count_strategy: CountStrategy,
    pub max_parallel_queries: usize,
    /// Number of SQL templates retained by the optional shape cache. Zero disables it.
    pub shape_cache_capacity: usize,
}

impl Default for PgExecutionPolicy {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(5),
            strategy: ExecutionStrategy::SequentialBestEffort,
            count_strategy: CountStrategy::Separate,
            max_parallel_queries: 16,
            shape_cache_capacity: 0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PgSiftQuery {
    pool: PgPool,
    pub(crate) policy: QueryPolicy,
    pub(crate) execution: PgExecutionPolicy,
    pub(crate) renderer: PgRenderer,
    pub(crate) parallel_gate: Arc<Semaphore>,
    pub(crate) scan_gate: Arc<Semaphore>,
    pub(crate) scan_wait_gate: Arc<Semaphore>,
    shape_cache: Option<Arc<crate::QueryShapeCache>>,
    pub(crate) extensions: crate::enrichment::PgExtensions,
    pub(crate) observers: crate::observe::Observers,
}
