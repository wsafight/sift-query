extern crate sift_query_core as sift_query;

use sift_query::SearchEntity;
use sift_query_derive::SearchEntity;
use sift_query_sqlx_common::{
    BackendCapability, BackendKind, ContextPoolRouter, CsvExportDescriptor, PoolRouteError,
    PoolSelector, backend_capabilities, validate_export_policy,
};
use std::time::Duration;

#[derive(Debug, SearchEntity)]
#[search_entity(stable_order = "id:asc")]
struct LabeledRow {
    #[search_field(sortable, label = "status_dict")]
    id: i64,
}

struct Selector;
impl PoolSelector<u8> for Selector {
    fn select_pool(
        &self,
        _: &'static sift_query::EntityMeta,
        context: &u8,
    ) -> Result<String, PoolRouteError> {
        Ok(if *context == 1 { "archive" } else { "primary" }.to_owned())
    }
}

#[test]
fn capability_matrix_and_safe_export_descriptor_are_explicit() {
    assert!(backend_capabilities(BackendKind::Sqlite).supports(BackendCapability::CsvExport));
    assert!(!backend_capabilities(BackendKind::Sqlite).supports(BackendCapability::Spatial));
    assert!(backend_capabilities(BackendKind::MySql).supports(BackendCapability::CsvExport));
    assert_eq!(
        backend_capabilities(BackendKind::MySql).all(),
        backend_capabilities(BackendKind::MariaDb).all()
    );
    assert!(backend_capabilities(BackendKind::PostgreSql).supports(BackendCapability::Spatial));
    let descriptor = CsvExportDescriptor::new("orders.csv").unwrap();
    assert_eq!(descriptor.content_type, "text/csv; charset=utf-8");
    assert!(CsvExportDescriptor::new("../orders.csv").is_err());
    assert!(CsvExportDescriptor::new("orders\".csv").is_err());
    assert!(CsvExportDescriptor::new("orders\0.csv").is_err());
    assert!(validate_export_policy(0, Duration::from_secs(1), 1).is_err());
    assert!(validate_export_policy(1, Duration::from_secs(1), 0).is_err());
    assert!(sift_query_sqlx_common::ExportLimiter::new(0).is_err());
    assert!(sift_query_sqlx_common::ExportLimiter::new(2).is_ok());
}

#[test]
fn derive_label_bindings_and_context_router_fail_closed() {
    assert_eq!(LabeledRow::label_bindings(), &[("id", "status_dict")]);
    let router = ContextPoolRouter::<u8>::default()
        .register_pool("primary")
        .unwrap()
        .register_pool("archive")
        .unwrap()
        .route_entity::<LabeledRow>("primary")
        .unwrap();
    let selector = Selector;
    assert_eq!(
        router.select::<LabeledRow, _>(&0, &selector).unwrap(),
        "primary"
    );
    assert_eq!(
        router.select::<LabeledRow, _>(&1, &selector).unwrap(),
        "archive"
    );
    assert!(router.clone().register_pool("../bad").is_err());
    assert!(router.clone().register_pool("primary").is_err());
}
