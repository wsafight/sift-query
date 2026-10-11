use sift_query_core::{ParamError, PlanError};
use thiserror::Error;

use crate::{ExtensionError, RenderError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeoutStage {
    PoolAcquire,
    ListQuery,
    ClusterQuery,
    Scan,
    Export,
    Enrichment,
}

impl std::fmt::Display for TimeoutStage {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::PoolAcquire => "pool acquire",
            Self::ListQuery => "list query",
            Self::ClusterQuery => "cluster query",
            Self::Scan => "scan",
            Self::Export => "export",
            Self::Enrichment => "enrichment",
        })
    }
}

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum SearchError {
    #[error(transparent)]
    Param(#[from] ParamError),
    #[error(transparent)]
    Plan(#[from] PlanError),
    #[error(transparent)]
    Render(#[from] RenderError),
    #[error(transparent)]
    Extension(#[from] ExtensionError),
    #[error("argument encoding failed: {0}")]
    Bind(String),
    #[error(transparent)]
    Sql(#[from] sqlx::Error),
    #[error("failed to decode column {column}: {reason}")]
    Decode { column: String, reason: String },
    #[error("scan exceeded its maximum row budget of {max_rows}")]
    ScanLimitExceeded { max_rows: u32 },
    #[error("scan waiting queue is full")]
    ScanQueueFull,
    #[error("CSV output failed")]
    ExportIo(#[source] std::io::Error),
    #[error("query timed out during {0}")]
    Timeout(TimeoutStage),
}
