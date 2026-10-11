//! Subscriber lifecycle test in its own binary so unrelated uninstrumented
//! tests do not race global tracing callsite-interest cache initialization.
extern crate sift_query_core as sift_query;
use sift_query::{RawParams, ServerScope};
use sift_query_derive::SearchEntity;
use sift_query_sqlx::{PgExecutionPolicy, PgSiftQuery};
use std::sync::{Arc, Mutex};
use tracing::instrument::WithSubscriber;
use tracing_subscriber::fmt::format::FmtSpan;

#[derive(Debug, sqlx::FromRow, SearchEntity)]
#[search_entity(from = "items i", default_order = "id:asc", stable_order = "id:asc")]
struct Entity {
    #[search_field(column = "i.id", sortable)]
    id: i64,
}

#[derive(Clone)]
struct LogWriter(Arc<Mutex<Vec<u8>>>);
impl std::io::Write for LogWriter {
    fn write(&mut self, input: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(input);
        Ok(input.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[tokio::test]
async fn tracing_spans_close_on_success_and_failure_and_never_log_user_input() {
    let logs = Arc::new(Mutex::new(Vec::new()));
    let captured = logs.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_ansi(false)
        .without_time()
        .with_span_events(FmtSpan::NEW | FmtSpan::CLOSE)
        .with_writer(move || LogWriter(captured.clone()))
        .finish();
    async {
        let search = PgSiftQuery::with_policy(
            sqlx::postgres::PgPoolOptions::new()
                .connect_lazy("postgres://localhost:1/unused")
                .unwrap(),
            sift_query::QueryPolicy::bean_searcher_4812(),
            PgExecutionPolicy::default(),
        )
        .unwrap();
        search
            .search::<Entity>(
                &RawParams::parse("size=999").unwrap(),
                &ServerScope::unrestricted(),
            )
            .await
            .unwrap();
        assert!(
            search
                .search::<Entity>(
                    &RawParams::parse("id=private-bind-value").unwrap(),
                    &ServerScope::unrestricted()
                )
                .await
                .is_err()
        );
    }
    .with_subscriber(subscriber)
    .await;
    let output = String::from_utf8(logs.lock().unwrap().clone()).unwrap();
    assert!(output.contains("outcome=\"success\""), "{output}");
    assert!(output.contains("outcome=\"parameter\""), "{output}");
    assert_eq!(output.matches("close").count(), 2, "{output}");
    assert!(!output.contains("private-bind-value"));
    assert!(!output.contains("acquired PostgreSQL connection"));
    assert!(!output.contains("executing SiftQuery"));
}
