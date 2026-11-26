#![allow(clippy::doc_markdown)]
#![doc = include_str!("../README.md")]

pub use sift_query_core::*;
pub use sift_query_derive::{SearchEntity, SearchProjection, search_entities};

#[cfg(any(
    feature = "postgres",
    feature = "mysql",
    feature = "sqlite",
    feature = "axum",
    feature = "actix-web",
    feature = "poem"
))]
pub use sift_query_sqlx_common::{
    BackendCapability, BackendCapabilitySet, BackendKind, CsvColumn, CsvExportPolicy, CursorPage,
    DictionaryLabelLoader, EnrichContext, ExportLimiter, ExtensionError, ExtensionFuture, Label,
    LabelEnricher, LabelFailurePolicy, LabelLoader, LabelTarget, MissingLabelPolicy, Page,
    PageEnricher, QueryObservation, QueryObserver, RowMap, ScanPolicy, SummaryValue, TimeoutStage,
    TypedPageEnricher, backend_capabilities,
};

#[cfg(all(
    any(
        feature = "mysql",
        feature = "sqlite",
        feature = "axum",
        feature = "actix-web",
        feature = "poem"
    ),
    not(feature = "postgres")
))]
pub use sift_query_sqlx_common::web::{
    self, PublicError, QueryParams, SearchParamConfig, SearchParams, WebError, public_error,
};

#[cfg(feature = "postgres")]
pub use sift_query_sqlx::*;

#[cfg(feature = "postgres")]
pub use sift_query_sqlx::PgSearchError as SearchError;

#[cfg(feature = "mysql")]
pub use sift_query_mysql::{
    CsvExporter as MySqlCsvExporter, MySqlBackend, MySqlExecutionPolicy, MySqlPoolRouter,
    MySqlSiftQuery, ScanFuture as MySqlScanFuture, SearchError as MySqlSearchError,
    SearchStream as MySqlSearchStream,
};

#[cfg(feature = "sqlite")]
pub use sift_query_sqlite::{
    CsvExporter as SqliteCsvExporter, ScanFuture as SqliteScanFuture,
    SearchError as SqliteSearchError, SearchStream as SqliteSearchStream, SqliteBackend,
    SqliteExecutionPolicy, SqlitePoolRouter, SqliteSiftQuery,
};

#[doc(hidden)]
pub mod __private {
    pub use sift_query_core::__private::*;
}
