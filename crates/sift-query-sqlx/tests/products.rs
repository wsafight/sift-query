extern crate sift_query_core as sift_query;
use sift_query::{RawParams, ServerScope};
use sift_query_derive::SearchEntity;
use sift_query_sqlx::*;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};
use std::time::Duration;

fn page(data: Vec<RowMap>) -> Page<RowMap> {
    Page {
        data,
        total_count: 20,
        summaries: Vec::new(),
        next_cursor: Some("opaque-cursor".to_owned()),
    }
}
fn context() -> EnrichContext {
    EnrichContext {
        entity_name: "Item",
        operation: "search_map",
    }
}
fn rows() -> Vec<RowMap> {
    [1, 1, 2]
        .into_iter()
        .map(|id| {
            [
                ("status".to_owned(), core::RowValue::I32(id)),
                ("other".to_owned(), core::RowValue::I32(1)),
            ]
            .into_iter()
            .collect()
        })
        .collect()
}
#[derive(Clone)]
struct Loader(Arc<AtomicUsize>);
impl LabelLoader for Loader {
    fn supports(&self, key: &str) -> bool {
        key == "status"
    }
    fn load(&self, _: &str, ids: Vec<String>) -> ExtensionFuture<Vec<Label>> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Box::pin(async move {
            Ok(ids
                .into_iter()
                .filter(|id| id == "1")
                .map(|id| Label {
                    id,
                    label: "Enabled".to_owned(),
                })
                .collect())
        })
    }
}

#[tokio::test]
async fn labels_batch_across_bindings_and_preserve_page_metadata() {
    let calls = Arc::new(AtomicUsize::new(0));
    let labels = LabelEnricher::new()
        .bind("status", "status")
        .bind("other", "status")
        .with_loader(Loader(calls.clone()))
        .with_missing_policy(MissingLabelPolicy::Null);
    let enriched = PageEnricher::enrich(&labels, page(rows()), context())
        .await
        .unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        enriched.data[0]["statusLabel"],
        core::RowValue::Text("Enabled".to_owned())
    );
    assert_eq!(enriched.data[2]["statusLabel"], core::RowValue::Null);
    assert_eq!(
        enriched.data[2]["otherLabel"],
        core::RowValue::Text("Enabled".to_owned())
    );
    assert_eq!(enriched.total_count, 20);
    assert_eq!(enriched.next_cursor.as_deref(), Some("opaque-cursor"));
}

#[tokio::test]
async fn label_cache_is_owned_by_the_instance_and_scoped_by_key_and_entity() {
    let calls = Arc::new(AtomicUsize::new(0));
    let labels = LabelEnricher::new()
        .bind("status", "status")
        .with_loader(Loader(calls.clone()))
        .with_cache(2, Duration::from_secs(60))
        .unwrap();
    let input = || page(vec![rows().remove(0)]);
    PageEnricher::enrich(&labels, input(), context())
        .await
        .unwrap();
    PageEnricher::enrich(&labels.clone(), input(), context())
        .await
        .unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    PageEnricher::enrich(
        &labels,
        input(),
        EnrichContext {
            entity_name: "OtherItem",
            ..context()
        },
    )
    .await
    .unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    let separate = LabelEnricher::new()
        .bind("status", "status")
        .with_loader(Loader(calls.clone()))
        .with_cache(2, Duration::from_secs(60))
        .unwrap();
    PageEnricher::enrich(&separate, input(), context())
        .await
        .unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 3);
}

#[tokio::test]
#[allow(clippy::items_after_statements)]
async fn label_id_budget_missing_labels_and_timeout_have_explicit_outcomes() {
    let calls = Arc::new(AtomicUsize::new(0));
    let labels = LabelEnricher::new()
        .bind("status", "status")
        .with_loader(Loader(calls.clone()))
        .with_budget(1, Duration::from_secs(1));
    assert!(matches!(
        PageEnricher::enrich(&labels, page(rows()), context()).await,
        Err(ExtensionError::LabelIdLimit(1))
    ));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    let labels = LabelEnricher::new()
        .bind("status", "status")
        .with_loader(Loader(calls))
        .with_missing_policy(MissingLabelPolicy::Error);
    assert!(
        PageEnricher::enrich(&labels, page(rows()), context())
            .await
            .is_err()
    );
    struct Pending;
    impl LabelLoader for Pending {
        fn supports(&self, _: &str) -> bool {
            true
        }
        fn load(&self, _: &str, _: Vec<String>) -> ExtensionFuture<Vec<Label>> {
            Box::pin(std::future::pending())
        }
    }
    let labels = LabelEnricher::new()
        .bind("status", "status")
        .with_loader(Pending)
        .with_budget(10, Duration::from_millis(5));
    assert_eq!(
        PageEnricher::enrich(&labels, page(rows()), context())
            .await
            .unwrap_err(),
        ExtensionError::LabelTimeout
    );
    let labels = labels.with_failure_policy(LabelFailurePolicy::KeepRows);
    assert_eq!(
        PageEnricher::enrich(&labels, page(rows()), context())
            .await
            .unwrap()
            .data,
        rows()
    );
}

#[tokio::test]
async fn malicious_label_batches_never_poison_cache() {
    struct BadThenGood(Arc<AtomicUsize>);
    impl LabelLoader for BadThenGood {
        fn supports(&self, _: &str) -> bool {
            true
        }
        fn load(&self, _: &str, _: Vec<String>) -> ExtensionFuture<Vec<Label>> {
            let attempt = self.0.fetch_add(1, Ordering::SeqCst);
            Box::pin(async move {
                Ok(if attempt == 0 {
                    vec![
                        Label {
                            id: "1".to_owned(),
                            label: "poison".to_owned(),
                        },
                        Label {
                            id: "999".to_owned(),
                            label: "foreign".to_owned(),
                        },
                    ]
                } else {
                    vec![Label {
                        id: "1".to_owned(),
                        label: "Good".to_owned(),
                    }]
                })
            })
        }
    }
    let calls = Arc::new(AtomicUsize::new(0));
    let labels = LabelEnricher::new()
        .bind("status", "status")
        .with_loader(BadThenGood(calls.clone()))
        .with_cache(10, Duration::from_secs(60))
        .unwrap();
    let input = || page(vec![rows().remove(0)]);
    assert!(
        PageEnricher::enrich(&labels, input(), context())
            .await
            .is_err()
    );
    let result = PageEnricher::enrich(&labels, input(), context())
        .await
        .unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert_eq!(
        result.data[0]["statusLabel"],
        core::RowValue::Text("Good".to_owned())
    );
}

#[tokio::test]
async fn typed_label_target_declares_writable_presentation_fields() {
    struct Item {
        id: i64,
        status: i32,
        label: Option<String>,
    }
    impl LabelTarget for Item {
        fn label_id(&self, source: &str) -> Option<String> {
            (source == "status").then(|| self.status.to_string())
        }
        fn set_label(&mut self, source: &str, label: Option<&str>) -> Result<(), ExtensionError> {
            if source != "status" {
                return Err(ExtensionError::rejected("test", "unknown label target"));
            }
            self.label = label.map(str::to_owned);
            Ok(())
        }
    }
    let labels = LabelEnricher::new().bind("status", "status").with_loader(
        DictionaryLabelLoader::default()
            .with_labels("status", [("1".to_owned(), "Enabled".to_owned())]),
    );
    let rows = TypedPageEnricher::enrich(
        &labels,
        vec![Item {
            id: 99,
            status: 1,
            label: None,
        }],
        context(),
    )
    .await
    .unwrap();
    assert_eq!(rows[0].label.as_deref(), Some("Enabled"));
    assert_eq!(rows[0].id, 99);
    assert_eq!(rows[0].status, 1);
}

#[tokio::test]
async fn csv_uses_server_column_order_and_handles_injection_quotes_unicode_and_newlines() {
    let row = [
        (
            "secret".to_owned(),
            core::RowValue::Text("must-not-export".to_owned()),
        ),
        (
            "name".to_owned(),
            core::RowValue::Text("\t =SUM(1,2)\r\n\"中文\"".to_owned()),
        ),
        ("id".to_owned(), core::RowValue::I64(7)),
    ]
    .into_iter()
    .collect();
    let csv = CsvExporter::new(CsvExportPolicy::default())
        .with_columns([CsvColumn::new("id", "ID"), CsvColumn::new("name", "Name")]);
    let mut output = Vec::new();
    assert_eq!(
        csv.write_page(&mut output, &page(vec![row])).await.unwrap(),
        1
    );
    assert_eq!(
        String::from_utf8(output).unwrap(),
        "ID,Name\n7,\"'\t =SUM(1,2)\r\n\"\"中文\"\"\"\n"
    );
}

#[derive(Debug, sqlx::FromRow, SearchEntity)]
#[search_entity(from = "items i", default_order = "id:asc", stable_order = "id:asc")]
struct Entity {
    #[search_field(column = "i.id", sortable)]
    id: i64,
}

#[tokio::test]
async fn scan_rejects_client_paging_before_any_connection_attempt() {
    let search = PgSiftQuery::new(
        sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://localhost:1/unused")
            .unwrap(),
    );
    for query in ["size=1", "page=1", "offset=0"] {
        assert!(matches!(
            search.scan_map_stream::<Entity>(
                &RawParams::parse(query).unwrap(),
                &ServerScope::unrestricted(),
                ScanPolicy::default()
            ),
            Err(PgSearchError::Param(core::ParamError::PagingNotAllowed))
        ));
    }
    assert!(
        ScanPolicy {
            max_rows: u32::MAX,
            ..ScanPolicy::default()
        }
        .validate()
        .is_err()
    );
}

#[test]
fn public_error_body_never_contains_internal_details() {
    let error =
        PgSearchError::Bind("SELECT internal FROM tenant WHERE secret = hidden-cursor".to_owned());
    assert_eq!(
        public_error(&error).body(),
        "{\"error\":{\"code\":\"query_failed\"}}"
    );
    assert_eq!(
        public_error(&PgSearchError::Timeout(TimeoutStage::Export)).status,
        504
    );
}

#[cfg(all(feature = "axum", feature = "actix-web", feature = "poem"))]
#[tokio::test]
async fn all_framework_extractors_preserve_order_duplicates_and_reject_bad_encoding() {
    let query = "id=2&id=1&name=%E4%B8%AD%E6%96%87+%2B";
    let uri = format!("/?{query}");
    let (mut parts, ()) = axum::http::Request::builder()
        .uri(&uri)
        .body(())
        .unwrap()
        .into_parts();
    let axum_params =
        <QueryParams as axum::extract::FromRequestParts<()>>::from_request_parts(&mut parts, &())
            .await
            .unwrap();
    let request = actix_web::test::TestRequest::with_uri(&uri).to_http_request();
    let actix_params = <QueryParams as actix_web::FromRequest>::from_request(
        &request,
        &mut actix_web::dev::Payload::None,
    )
    .await
    .unwrap();
    let request = poem::Request::builder().uri(uri.parse().unwrap()).finish();
    let poem_params = <QueryParams as poem::FromRequest>::from_request_without_body(&request)
        .await
        .unwrap();
    assert_eq!(axum_params, actix_params);
    assert_eq!(axum_params, poem_params);
    assert_eq!(
        axum_params.raw().entries()[1],
        ("id".to_owned(), "1".to_owned())
    );
    assert!(QueryParams::parse(Some("id=%GG")).is_err());
}

#[derive(Clone)]
struct Recorder(Arc<Mutex<Vec<QueryObservation>>>);
impl QueryObserver for Recorder {
    fn completed(&self, observation: &QueryObservation) {
        self.0.lock().unwrap().push(*observation);
    }
}

#[tokio::test]
async fn observer_records_success_parameter_failure_and_stream_cancellation() {
    let recorded = Arc::new(Mutex::new(Vec::new()));
    let search = PgSiftQuery::with_policy(
        sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://localhost:1/unused")
            .unwrap(),
        core::QueryPolicy::bean_searcher_4812(),
        PgExecutionPolicy::default(),
    )
    .unwrap()
    .with_query_observer(Recorder(recorded.clone()))
    .with_slow_query_threshold(Duration::ZERO);
    assert!(
        search
            .search::<Entity>(
                &RawParams::parse("size=999").unwrap(),
                &ServerScope::unrestricted()
            )
            .await
            .unwrap()
            .data
            .is_empty()
    );
    assert!(
        search
            .search::<Entity>(
                &RawParams::parse("id=secret-value").unwrap(),
                &ServerScope::unrestricted()
            )
            .await
            .is_err()
    );
    let stream = search
        .search_stream::<Entity>(&RawParams::default(), &ServerScope::unrestricted())
        .unwrap();
    drop(stream);
    let observations = recorded.lock().unwrap();
    assert_eq!(
        observations
            .iter()
            .map(|observation| observation.outcome)
            .collect::<Vec<_>>(),
        ["success", "parameter", "cancelled"]
    );
    assert!(observations.iter().all(|observation| observation.slow));
    assert!(!format!("{observations:?}").contains("secret-value"));
}

#[tokio::test]
async fn observer_panics_cannot_fail_a_query() {
    struct Panic;
    impl QueryObserver for Panic {
        fn completed(&self, _: &QueryObservation) {
            panic!("observer failure");
        }
    }
    let search = PgSiftQuery::with_policy(
        sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://localhost:1/unused")
            .unwrap(),
        core::QueryPolicy::bean_searcher_4812(),
        PgExecutionPolicy::default(),
    )
    .unwrap()
    .with_query_observer(Panic);
    assert!(
        search
            .search::<Entity>(
                &RawParams::parse("size=999").unwrap(),
                &ServerScope::unrestricted()
            )
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn csv_writer_errors_and_deadlines_are_explicit() {
    use std::{
        pin::Pin,
        task::{Context, Poll},
    };
    struct Writer(bool);
    impl tokio::io::AsyncWrite for Writer {
        fn poll_write(
            self: Pin<&mut Self>,
            _: &mut Context<'_>,
            _: &[u8],
        ) -> Poll<std::io::Result<usize>> {
            if self.0 {
                Poll::Pending
            } else {
                Poll::Ready(Err(std::io::Error::from(std::io::ErrorKind::BrokenPipe)))
            }
        }
        fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<std::io::Result<()>> {
            Poll::Ready(Ok(()))
        }
        fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<std::io::Result<()>> {
            Poll::Ready(Ok(()))
        }
    }
    let csv = CsvExporter::new(CsvExportPolicy {
        timeout: Duration::from_millis(5),
        ..CsvExportPolicy::default()
    });
    assert!(matches!(
        csv.write_page(&mut Writer(false), &page(rows())).await,
        Err(PgSearchError::ExportIo(_))
    ));
    assert!(matches!(
        csv.write_page(&mut Writer(true), &page(rows())).await,
        Err(PgSearchError::Timeout(TimeoutStage::Export))
    ));
}

#[tokio::test]
async fn label_cache_expires_and_existing_targets_are_not_overwritten() {
    let calls = Arc::new(AtomicUsize::new(0));
    let labels = LabelEnricher::new()
        .bind("status", "status")
        .with_loader(Loader(calls.clone()))
        .with_cache(1, Duration::from_nanos(1))
        .unwrap();
    let input = || page(vec![rows().remove(0)]);
    PageEnricher::enrich(&labels, input(), context())
        .await
        .unwrap();
    PageEnricher::enrich(&labels, input(), context())
        .await
        .unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    let mut data = rows();
    data[0].insert(
        "statusLabel".to_owned(),
        core::RowValue::Text("existing".to_owned()),
    );
    assert!(
        PageEnricher::enrich(&labels, page(data), context())
            .await
            .is_err()
    );
}
