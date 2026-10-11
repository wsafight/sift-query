extern crate sift_query_core as sift_query;

use futures_util::StreamExt;
use sift_query_core::{
    AggregateKind, EnumCodec, FieldId, FieldRef, ListResultCodec, QueryPolicy, RawParams, RowValue,
    ScalarKind, SearchOptions, ServerScope, ValueType,
};
use sift_query_derive::SearchEntity;
use sift_query_sqlite::{
    CsvColumn, CsvExportPolicy, CsvExporter, ScanPolicy, SearchError, SqliteSiftQuery,
};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{FromRow, Row};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Debug, FromRow, SearchEntity)]
#[search_entity(from = "users", default_order = "id:asc", stable_order = "id:asc")]
struct User {
    #[search_field(column = "id", sortable, operators = [Eq, InList])]
    id: i64,
    #[search_field(column = "name", sortable, operators = [Eq, StartWith])]
    name: String,
}

#[allow(dead_code)]
#[derive(Debug, FromRow, SearchEntity)]
#[search_entity(
    from = "codec_users",
    default_order = "id:asc",
    stable_order = "id:asc"
)]
struct CodecUser {
    #[search_field(column = "id", sortable)]
    id: i64,
    #[search_field(
        column = "status",
        codec = "status",
        label = "status",
        operators = [Eq]
    )]
    status: i32,
    #[search_field(column = "tags", codec = "tags")]
    tags: String,
}

#[allow(dead_code)]
#[derive(Debug, FromRow, SearchEntity)]
#[search_entity(from = "scores", default_order = "id:asc", stable_order = "id:asc")]
struct Score {
    #[search_field(column = "id", sortable)]
    id: i64,
    #[search_field(column = "value", operators = [Eq, Ge, Le])]
    value: i32,
    #[search_field(column = "label", operators = [Eq])]
    label: String,
}

#[derive(Debug, FromRow, SearchEntity)]
#[search_entity(
    from = "grouped_scores",
    fixed_having = "count(*) >= 1",
    default_order = "status:asc",
    stable_order = "status:asc"
)]
struct GroupedScore {
    #[search_field(column = "status", kind = GroupKey, sortable)]
    status: i32,
    #[search_field(column = "count(*)", kind = Aggregate, operators = [Ge])]
    count: i64,
}

#[tokio::test]
async fn sqlite_search_supports_strict_params_and_count() {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::query("CREATE TABLE users (id INTEGER NOT NULL, name TEXT NOT NULL)")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO users (id, name) VALUES (?, ?), (?, ?), (?, ?)")
        .bind(1_i64)
        .bind("Alice")
        .bind(2_i64)
        .bind("Bob")
        .bind(3_i64)
        .bind("Ari")
        .execute(&pool)
        .await
        .unwrap();

    let search = SqliteSiftQuery::new(pool);
    let raw = RawParams::parse("name=A&name-op=sw&size=10").unwrap();
    let query = search.parse::<User>(&raw).unwrap();
    let page = search
        .search(&query, &ServerScope::unrestricted())
        .await
        .unwrap();
    assert_eq!(page.total_count, 2);
    assert_eq!(
        page.data.iter().map(|user| user.id).collect::<Vec<_>>(),
        vec![1, 3]
    );
}

#[tokio::test]
async fn sqlite_external_connection_entrypoint_works_inside_transaction() {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::query("CREATE TABLE users (id INTEGER NOT NULL, name TEXT NOT NULL)")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO users (id, name) VALUES (1, 'Alice')")
        .execute(&pool)
        .await
        .unwrap();
    let search = SqliteSiftQuery::new(pool.clone());
    let query = search
        .parse::<User>(&RawParams::parse("size=1").unwrap())
        .unwrap();
    let mut transaction = pool.begin().await.unwrap();
    let page = search
        .search_on(&mut transaction, &query, &ServerScope::unrestricted())
        .await
        .unwrap();
    assert_eq!(page.total_count, 1);
    transaction.rollback().await.unwrap();
}

#[tokio::test]
async fn sqlite_external_connection_supports_summaries() {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::query(
        "CREATE TABLE scores (id INTEGER NOT NULL, value INTEGER NOT NULL, label TEXT NOT NULL)",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO scores (id, value, label) VALUES (1, 10, 'a'), (2, 20, 'b')")
        .execute(&pool)
        .await
        .unwrap();
    let search = SqliteSiftQuery::new(pool.clone());
    let options = SearchOptions::<Score>::builder()
        .sum(FieldRef::<Score, i32, ScalarKind>::new(FieldId(1)))
        .build()
        .unwrap();
    let query = search
        .parse::<Score>(&RawParams::parse("size=1").unwrap())
        .unwrap();
    let mut transaction = pool.begin().await.unwrap();
    let page = search
        .search_on_with(
            &mut transaction,
            &query,
            &ServerScope::unrestricted(),
            Some(&options),
        )
        .await
        .unwrap();
    assert_eq!(page.total_count, 2);
    assert_eq!(page.summaries[0].value, Some(RowValue::I64(30)));
    transaction.rollback().await.unwrap();
}

#[tokio::test]
async fn sqlite_like_escapes_user_wildcards() {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::query("CREATE TABLE users (id INTEGER NOT NULL, name TEXT NOT NULL)")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO users (id, name) VALUES (1, 'A%literal'), (2, 'Alice')")
        .execute(&pool)
        .await
        .unwrap();
    let search = SqliteSiftQuery::new(pool);
    let query = search
        .parse::<User>(&RawParams::parse("name=A%25&name-op=sw&size=10").unwrap())
        .unwrap();
    let page = search
        .search(&query, &ServerScope::unrestricted())
        .await
        .unwrap();
    assert_eq!(page.total_count, 1);
    assert_eq!(page.data[0].name, "A%literal");
}

#[tokio::test]
async fn sqlite_search_with_returns_typed_summaries() {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::query(
        "CREATE TABLE scores (id INTEGER NOT NULL, value INTEGER NOT NULL, label TEXT NOT NULL)",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO scores (id, value, label) VALUES (1, 10, 'a'), (2, 20, 'b')")
        .execute(&pool)
        .await
        .unwrap();

    let search = SqliteSiftQuery::new(pool);
    let options = SearchOptions::<Score>::builder()
        .sum(FieldRef::<Score, i32, ScalarKind>::new(FieldId(1)))
        .avg(FieldRef::<Score, i32, ScalarKind>::new(FieldId(1)))
        .min(FieldRef::<Score, i32, ScalarKind>::new(FieldId(1)))
        .max(FieldRef::<Score, i32, ScalarKind>::new(FieldId(1)))
        .build()
        .unwrap();
    let query = search
        .parse::<Score>(&RawParams::parse("size=10").unwrap())
        .unwrap();
    let page = search
        .search_with(&query, &ServerScope::unrestricted(), &options)
        .await
        .unwrap();

    assert_eq!(page.total_count, 2);
    assert_eq!(page.summaries.len(), 4);
    assert_eq!(page.summaries[0].value, Some(RowValue::I64(30)));
    assert_eq!(page.summaries[1].value, Some(RowValue::F64(15.0)));
    assert_eq!(page.summaries[2].value, Some(RowValue::I32(10)));
    assert_eq!(page.summaries[3].value, Some(RowValue::I32(20)));
}

#[tokio::test]
async fn sqlite_grouped_search_with_summarizes_groups() {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::query("CREATE TABLE grouped_scores (status INTEGER NOT NULL)")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO grouped_scores (status) VALUES (1), (1), (2)")
        .execute(&pool)
        .await
        .unwrap();

    let search = SqliteSiftQuery::new(pool);
    let options = SearchOptions::<GroupedScore>::builder()
        .max(FieldRef::<GroupedScore, i64, ScalarKind>::new(FieldId(1)))
        .build()
        .unwrap();
    let query = search
        .parse::<GroupedScore>(&RawParams::parse("size=10").unwrap())
        .unwrap();
    let page = search
        .search_with(&query, &ServerScope::unrestricted(), &options)
        .await
        .unwrap();

    assert_eq!(page.total_count, 2);
    assert_eq!(page.summaries[0].value, Some(RowValue::I64(2)));
    assert_eq!(
        page.data.iter().map(|row| row.count).collect::<Vec<_>>(),
        vec![2, 1]
    );

    let group_scope = sift_query_core::GroupScope::<GroupedScore>::new()
        .ge(
            FieldRef::<GroupedScore, i64, AggregateKind>::new(FieldId(1)),
            2_i64,
        )
        .build()
        .unwrap();
    let filtered = search
        .search_with_group_scope(&query, &ServerScope::unrestricted(), &group_scope)
        .await
        .unwrap();
    assert_eq!(filtered.total_count, 1);
    assert_eq!(filtered.data[0].status, 1);
}

#[tokio::test]
async fn sqlite_scan_rejects_client_paging_and_enforces_row_budget() {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::query("CREATE TABLE users (id INTEGER NOT NULL, name TEXT NOT NULL)")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO users (id, name) VALUES (1, 'Alice'), (2, 'Bob'), (3, 'Cara')")
        .execute(&pool)
        .await
        .unwrap();

    let search = SqliteSiftQuery::new(pool);
    let paged = RawParams::parse("size=1").unwrap();
    let Err(error) =
        search.scan_map::<User>(&paged, &ServerScope::unrestricted(), ScanPolicy::default())
    else {
        panic!("scan must reject explicit paging")
    };
    assert!(matches!(error, SearchError::Param(_)));

    let raw = RawParams::parse("").unwrap();
    let error = search
        .scan_map::<User>(
            &raw,
            &ServerScope::unrestricted(),
            ScanPolicy {
                max_rows: 2,
                batch_size: 1,
                ..ScanPolicy::default()
            },
        )
        .unwrap()
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        SearchError::ScanLimitExceeded { max_rows: 2 }
    ));
}

#[tokio::test]
async fn sqlite_scan_drop_releases_pool_connection() {
    let path = std::env::temp_dir().join(format!("sift-query-scan-{}.db", std::process::id()));
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            SqliteConnectOptions::new()
                .filename(&path)
                .create_if_missing(true),
        )
        .await
        .unwrap();
    sqlx::query("CREATE TABLE users (id INTEGER NOT NULL, name TEXT NOT NULL)")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO users (id, name) VALUES (1, 'Alice'), (2, 'Bob')")
        .execute(&pool)
        .await
        .unwrap();

    let search = SqliteSiftQuery::new(pool.clone());
    let query = search
        .parse::<User>(&RawParams::parse("").unwrap())
        .unwrap();
    let mut stream = search
        .scan_map_stream(&query, &ServerScope::unrestricted(), ScanPolicy::default())
        .unwrap();
    assert!(stream.next().await.unwrap().is_ok());
    drop(stream);

    let count = tokio::time::timeout(
        Duration::from_secs(2),
        sqlx::query("SELECT count(*) AS count FROM users").fetch_one(&pool),
    )
    .await
    .expect("pool slot should be released after scan cancellation")
    .unwrap()
    .get::<i64, _>("count");
    assert_eq!(count, 2);
    pool.close().await;
    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn sqlite_csv_export_is_streamed_and_escaped() {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::query("CREATE TABLE users (id INTEGER NOT NULL, name TEXT NOT NULL)")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO users (id, name) VALUES (1, 'a,b'), (2, '=formula')")
        .execute(&pool)
        .await
        .unwrap();

    let search = SqliteSiftQuery::new(pool);
    let raw = RawParams::parse("").unwrap();
    let mut output = Vec::new();
    let rows = CsvExporter::new(CsvExportPolicy {
        max_rows: 10,
        batch_size: 1,
        ..CsvExportPolicy::default()
    })
    .with_columns([CsvColumn::new("id", "ID"), CsvColumn::new("name", "Name")])
    .export::<User, _>(&search, &raw, &ServerScope::unrestricted(), &mut output)
    .await
    .unwrap();
    assert_eq!(rows, 2);
    assert_eq!(
        String::from_utf8(output).unwrap(),
        "ID,Name\n1,\"a,b\"\n2,'=formula\n"
    );
}

#[tokio::test]
async fn sqlite_map_results_apply_server_codecs() {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::query(
        "CREATE TABLE codec_users (id INTEGER NOT NULL, status INTEGER NOT NULL, tags TEXT NOT NULL)",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO codec_users (id, status, tags) VALUES (1, 1, '10|20')")
        .execute(&pool)
        .await
        .unwrap();

    let policy = QueryPolicy::default()
        .with_codec(EnumCodec::new("status", "status", [("enabled", 1)]).unwrap())
        .with_codec(ListResultCodec::new("tags", "tags", "|", ValueType::I64, 4).unwrap());
    let search = SqliteSiftQuery::new(pool)
        .with_policy(policy, sift_query_sqlite::SqliteExecutionPolicy::default());
    let raw = RawParams::parse("status=enabled&size=10").unwrap();
    let page = search
        .search_map::<CodecUser>(&raw, &ServerScope::unrestricted())
        .await
        .unwrap();
    assert_eq!(page.data[0]["status"], RowValue::Text("enabled".to_owned()));
    assert_eq!(
        page.data[0]["tags"],
        RowValue::List(vec![RowValue::I64(10), RowValue::I64(20)])
    );
}

#[tokio::test]
async fn sqlite_map_pages_support_batched_label_enrichment() {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::query(
        "CREATE TABLE codec_users (id INTEGER NOT NULL, status INTEGER NOT NULL, tags TEXT NOT NULL)",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO codec_users (id, status, tags) VALUES (1, 1, '10|20')")
        .execute(&pool)
        .await
        .unwrap();
    let policy = QueryPolicy::default()
        .with_codec(EnumCodec::new("status", "status", [("enabled", 1)]).unwrap())
        .with_codec(ListResultCodec::new("tags", "tags", "|", ValueType::I64, 4).unwrap());
    let labels = sift_query_sqlite::LabelEnricher::new()
        .from_entity::<CodecUser>()
        .with_loader(
            sift_query_sqlite::DictionaryLabelLoader::default()
                .with_labels("status", [("enabled".to_owned(), "Enabled".to_owned())]),
        );
    let search = SqliteSiftQuery::new(pool.clone())
        .with_policy(policy, sift_query_sqlite::SqliteExecutionPolicy::default())
        .with_page_enricher(labels);
    let raw = RawParams::parse("status=enabled&size=10").unwrap();
    let page = search
        .search_map::<CodecUser>(&raw, &ServerScope::unrestricted())
        .await
        .unwrap();
    assert_eq!(
        page.data[0]["statusLabel"],
        RowValue::Text("Enabled".to_owned())
    );
    assert_eq!(page.total_count, 1);

    let mut transaction = pool.begin().await.unwrap();
    let external = search
        .search_map_on::<CodecUser>(&mut transaction, &raw, &ServerScope::unrestricted())
        .await
        .unwrap();
    assert_eq!(
        external.data[0]["statusLabel"],
        RowValue::Text("Enabled".to_owned())
    );
    transaction.rollback().await.unwrap();
}

#[derive(Clone)]
struct TestObserver(Arc<Mutex<Vec<sift_query_sqlite::QueryObservation>>>);

impl sift_query_sqlite::QueryObserver for TestObserver {
    fn completed(&self, observation: &sift_query_sqlite::QueryObservation) {
        self.0.lock().unwrap().push(*observation);
    }
}

#[tokio::test]
async fn sqlite_observer_reports_success_and_parameter_failure() {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::query("CREATE TABLE users (id INTEGER NOT NULL, name TEXT NOT NULL)")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO users (id, name) VALUES (1, 'Alice'), (2, 'Bob')")
        .execute(&pool)
        .await
        .unwrap();
    let observations = Arc::new(Mutex::new(Vec::new()));
    let search = SqliteSiftQuery::new(pool).with_query_observer(TestObserver(observations.clone()));
    search
        .search::<User>(
            &RawParams::parse("size=1").unwrap(),
            &ServerScope::unrestricted(),
        )
        .await
        .unwrap();
    assert!(
        search
            .search::<User>(
                &RawParams::parse("unknown=1").unwrap(),
                &ServerScope::unrestricted()
            )
            .await
            .is_err()
    );
    let policy = ScanPolicy {
        max_rows: 1,
        ..ScanPolicy::default()
    };
    assert!(
        search
            .scan_map::<User>(
                &RawParams::parse("").unwrap(),
                &ServerScope::unrestricted(),
                policy
            )
            .expect("scan plan")
            .await
            .is_err()
    );
    let observations = observations.lock().unwrap();
    assert_eq!(observations.len(), 3);
    assert_eq!(observations[0].outcome, "success");
    assert_eq!(observations[1].outcome, "parameter");
    assert_eq!(observations[2].outcome, "budget");
    assert_eq!(observations[2].rows_returned, 1);
}
