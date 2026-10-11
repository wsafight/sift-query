//! PostgreSQL renderer and sqlx executor for `SiftQuery`.

mod buckets;
mod cache;
mod diagnostics;
mod enrichment;
mod error;
mod execution;
mod export;
mod facet;
mod observe;
pub mod postgres;
mod renderer;
mod router;
mod schema;
pub mod web;
#[cfg(feature = "rls")]
pub use postgres::rls::{PgRlsTransaction, RlsConfig, RlsContext};

pub use cache::{QueryShapeCache, ShapeCacheStats};
pub use diagnostics::{DiagnosticStage, IndexAdvice, QueryDiagnosticReport};
pub use enrichment::label::{
    DictionaryLabelLoader, Label, LabelEnricher, LabelFailurePolicy, LabelLoader, LabelTarget,
    MissingLabelPolicy,
};
pub use enrichment::{
    EnrichContext, ExtensionError, ExtensionFuture, PageEnricher, PartitionName, PartitionResolver,
    QueryContext, ResultContext, ResultFilter, ResultFilterDecision, SqlContext, SqlInterceptor,
    TypedPageEnricher,
};
pub use error::{DecodeError, PgSearchError, RenderError, SchemaValidationError, TimeoutStage};
pub use execution::strategy::{CountStrategy, ExecutionStrategy, SnapshotIsolation};
pub use execution::stream::{ScanFuture, ScanPolicy, SearchStream};
pub use execution::{CursorPage, Page, PgExecutionPolicy, PgSiftQuery, RowMap, SummaryValue};
pub use export::{CsvColumn, CsvExportPolicy, CsvExporter};
pub use facet::{FacetPage, FacetResultSet};
pub use observe::{QueryObservation, QueryObserver};
pub use postgres::sum::{PgSumType, PgSummaryValue};
#[cfg(feature = "postgres-types")]
pub use postgres::types::PgInt8Range;
pub use renderer::{PgBindValue, PgRenderer, QueryShapeId, RenderedPgQuery};
pub use router::{PgPoolRouter, PgPoolSelector, PoolRouteError};
pub use schema::{SchemaValidationReport, SchemaValidationTarget};
pub use sift_query_core as core;
pub use sift_query_core::{FacetBucket, FacetRequest, FacetResult};
pub use web::{PublicError, QueryParams, SearchParamConfig, SearchParams, WebError, public_error};
