#![allow(clippy::doc_markdown)]
#![doc = include_str!("../README.md")]

pub use sift_query_core::*;
pub use sift_query_derive::{SearchEntity, SearchProjection, search_entities};

#[cfg(feature = "postgres")]
pub use sift_query_sqlx::*;

#[cfg(feature = "postgres")]
pub use sift_query_sqlx::PgSearchError as SearchError;

#[cfg(feature = "mysql")]
pub use sift_query_mysql::{
    CsvColumn as MySqlCsvColumn, CsvExportPolicy as MySqlCsvExportPolicy,
    CsvExporter as MySqlCsvExporter, CursorPage as MySqlCursorPage, MySqlBackend,
    MySqlExecutionPolicy, MySqlPoolRouter, MySqlSiftQuery, Page as MySqlPage,
    ScanFuture as MySqlScanFuture, ScanPolicy as MySqlScanPolicy,
    SearchStream as MySqlSearchStream,
};

#[cfg(feature = "mysql")]
pub use sift_query_mysql::SearchError as MySqlSearchError;
#[cfg(feature = "mysql")]
pub use sift_query_mysql::web as mysql_web;

#[cfg(feature = "sqlite")]
pub use sift_query_sqlite::{
    CsvColumn as SqliteCsvColumn, CsvExportPolicy as SqliteCsvExportPolicy,
    CsvExporter as SqliteCsvExporter, CursorPage as SqliteCursorPage, Page as SqlitePage,
    ScanFuture as SqliteScanFuture, ScanPolicy as SqliteScanPolicy,
    SearchStream as SqliteSearchStream, SqliteBackend, SqliteExecutionPolicy, SqlitePoolRouter,
    SqliteSiftQuery,
};

#[cfg(feature = "sqlite")]
pub use sift_query_sqlite::SearchError as SqliteSearchError;
#[cfg(feature = "sqlite")]
pub use sift_query_sqlite::web as sqlite_web;

#[doc(hidden)]
pub mod __private {
    pub use sift_query_core::__private::*;
}
