use sift_query_core::{ParamError, PlanError};
use thiserror::Error;

use crate::ExtensionError;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum RenderError {
    #[error("query plan references unknown field id {0}")]
    UnknownField(u16),
    #[error("field {0} has no result alias")]
    MissingAlias(String),
    #[error("condition {0} has invalid values")]
    InvalidCondition(String),
    #[error("PostgreSQL binding is not implemented for {0}")]
    UnsupportedBindType(String),
    #[error("query target cannot be rendered by this operation")]
    WrongTarget,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("failed to decode column {column}: {reason}")]
pub struct DecodeError {
    pub column: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("schema result contract failed for {entity}: {details}")]
pub struct SchemaValidationError {
    pub entity: &'static str,
    pub details: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeoutStage {
    PoolAcquire,
    SchemaValidation,
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
            Self::SchemaValidation => "schema validation",
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
pub enum PgSearchError {
    #[error(transparent)]
    Param(#[from] ParamError),
    #[error(transparent)]
    Plan(#[from] PlanError),
    #[error(transparent)]
    Render(#[from] RenderError),
    #[error("failed to encode PostgreSQL arguments: {0}")]
    Bind(String),
    #[error("parallel execution gate is closed")]
    ParallelGateClosed,
    #[error(transparent)]
    Sql(#[from] sqlx::Error),
    #[error(transparent)]
    Decode(#[from] DecodeError),
    #[error(transparent)]
    Extension(#[from] ExtensionError),
    #[error("query timed out during {0}")]
    Timeout(TimeoutStage),
    #[error("scan exceeded its maximum row budget of {max_rows}")]
    ScanLimitExceeded { max_rows: u32 },
    #[error("external PostgreSQL execution only supports SequentialBestEffort")]
    ExternalExecutionStrategy,
    #[error("scan waiting queue is full")]
    ScanQueueFull,
    #[error("CSV output failed")]
    ExportIo(#[source] std::io::Error),
    #[error(transparent)]
    SchemaValidation(#[from] SchemaValidationError),
}
