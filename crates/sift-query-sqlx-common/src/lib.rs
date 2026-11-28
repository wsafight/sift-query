//! Shared portable SQL and execution. This crate enables no database driver.

mod backend;
mod capabilities;
mod csv;
mod cursor;
mod enrichment;
mod error;
mod export;
mod observe;
mod page;
mod renderer;
mod router;
mod scan;
mod template;
pub mod web;

pub use backend::Backend;
pub use capabilities::{
    BackendCapability, BackendCapabilitySet, BackendKind, backend_capabilities,
};
pub use csv::{
    CsvColumn, CsvExportDescriptor, CsvExportPolicy, ExportLimiter, RowMap, column_values,
    validate_export_policy, write_record, write_record_into, write_row_batch, write_row_into,
    write_rows,
};
pub use cursor::{cursor_page_size, entity_cursor, map_cursor};
pub use enrichment::{
    DictionaryLabelLoader, EnrichContext, ExtensionError, ExtensionFuture, Extensions, Label,
    LabelEnricher, LabelFailurePolicy, LabelLoader, LabelTarget, MissingLabelPolicy, PageEnricher,
    TypedPageEnricher,
};
pub use error::{SearchError, TimeoutStage};
pub use export::{export_csv_stream, resolve_export_columns, write_csv_page};
pub use observe::{Observers, QueryObservation, QueryObserver};
pub use page::{CursorPage, Page};
pub use renderer::{
    Dialect, RenderError, RenderedQuery, Renderer, SummaryValue, summary_output_type,
};
pub use router::{ContextPoolRouter, PoolRouteError, PoolRouterTable, PoolSelector};
pub use scan::{ScanFuture, ScanPolicy, SearchStream, bound_scan, default_scan_gates, scan_plan};
pub use sift_query_core as core;
