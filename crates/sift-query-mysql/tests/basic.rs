extern crate sift_query_core as sift_query;

use sift_query_core::{
    FieldId, FieldRef, RawParams, RowValue, ScalarKind, SearchOptions, ServerScope,
};
use sift_query_derive::SearchEntity;
use sift_query_mysql::{
    CsvColumn, CsvExportPolicy, CsvExporter, MySqlSiftQuery, ScanPolicy, SearchError,
};
use sqlx::FromRow;
use sqlx::mysql::MySqlPoolOptions;
use std::sync::OnceLock;

static DATABASE_TEST_LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();

async fn database_test_guard() -> tokio::sync::MutexGuard<'static, ()> {
    DATABASE_TEST_LOCK
        .get_or_init(|| tokio::sync::Mutex::const_new(()))
        .lock()
        .await
}

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
#[search_entity(from = "users_like", default_order = "id:asc", stable_order = "id:asc")]
struct UserLike {
    #[search_field(column = "id", sortable)]
    id: i64,
    #[search_field(column = "name", sortable, operators = [Eq, StartWith])]
    name: String,
}

#[tokio::test]
#[ignore = "requires DATABASE_URL pointing to a MySQL or MariaDB test service"]
async fn mysql_search_supports_strict_params_and_count() {
    let _guard = database_test_guard().await;
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL");
    let pool = sqlx::MySqlPool::connect(&url).await.unwrap();
    sqlx::query("DROP TABLE IF EXISTS users")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("CREATE TABLE users (id BIGINT NOT NULL, name VARCHAR(255) NOT NULL)")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO users (id, name) VALUES (?, ?), (?, ?)")
        .bind(1_i64)
        .bind("Alice")
        .bind(2_i64)
        .bind("Bob")
        .execute(&pool)
        .await
        .unwrap();
    let search = MySqlSiftQuery::new(pool);
    let query = search
        .parse::<User>(&sift_query_core::RawParams::parse("name=A&name-op=sw&size=10").unwrap())
        .unwrap();
    let page = search
        .search(&query, &ServerScope::unrestricted())
        .await
        .unwrap();
    assert_eq!(page.total_count, 1);
}

#[tokio::test]
#[ignore = "requires DATABASE_URL pointing to a MySQL or MariaDB test service"]
async fn mysql_search_with_returns_summaries() {
    let _guard = database_test_guard().await;
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL");
    let pool = sqlx::MySqlPool::connect(&url).await.unwrap();
    sqlx::query("DROP TABLE IF EXISTS scores")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "CREATE TABLE scores (id BIGINT NOT NULL, value INT NOT NULL, label VARCHAR(255) NOT NULL)",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO scores (id, value, label) VALUES (?, ?, ?), (?, ?, ?)")
        .bind(1_i64)
        .bind(10_i32)
        .bind("a")
        .bind(2_i64)
        .bind(20_i32)
        .bind("b")
        .execute(&pool)
        .await
        .unwrap();
    let search = MySqlSiftQuery::new(pool);
    let options = SearchOptions::<Score>::builder()
        .sum(FieldRef::<Score, i32, ScalarKind>::new(FieldId(1)))
        .avg(FieldRef::<Score, i32, ScalarKind>::new(FieldId(1)))
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
    assert_eq!(page.summaries[0].value, Some(RowValue::I64(30)));
    assert_eq!(page.summaries[1].value, Some(RowValue::F64(15.0)));
}

#[tokio::test]
#[ignore = "requires DATABASE_URL pointing to a MySQL or MariaDB test service"]
async fn mysql_like_escapes_user_wildcards() {
    let _guard = database_test_guard().await;
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL");
    let pool = sqlx::MySqlPool::connect(&url).await.unwrap();
    sqlx::query("DROP TABLE IF EXISTS users_like")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("CREATE TABLE users_like (id BIGINT NOT NULL, name VARCHAR(255) NOT NULL)")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO users_like (id, name) VALUES (1, 'A%literal'), (2, 'Alice')")
        .execute(&pool)
        .await
        .unwrap();
    let search = MySqlSiftQuery::new(pool);
    let query = search
        .parse::<UserLike>(&RawParams::parse("name=A%25&name-op=sw&size=10").unwrap())
        .unwrap();
    let page = search
        .search(&query, &ServerScope::unrestricted())
        .await
        .unwrap();
    assert_eq!(page.total_count, 1);
    assert_eq!(page.data[0].name, "A%literal");
}

#[tokio::test]
#[ignore = "requires DATABASE_URL pointing to a MySQL or MariaDB test service"]
async fn mysql_scan_and_csv_release_connections() {
    let _guard = database_test_guard().await;
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL");
    let pool = MySqlPoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
        .unwrap();
    sqlx::query("DROP TABLE IF EXISTS users")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("CREATE TABLE users (id BIGINT NOT NULL, name VARCHAR(255) NOT NULL)")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO users (id, name) VALUES (?, ?), (?, ?), (?, ?)")
        .bind(1_i64)
        .bind("Alice")
        .bind(2_i64)
        .bind("Bob")
        .bind(3_i64)
        .bind("Cara")
        .execute(&pool)
        .await
        .unwrap();
    let search = MySqlSiftQuery::new(pool.clone());
    let query = search
        .parse::<User>(&RawParams::parse("").unwrap())
        .unwrap();
    let error = search
        .scan_map(
            &query,
            &ServerScope::unrestricted(),
            ScanPolicy {
                max_rows: 2,
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

    let mut output = Vec::new();
    let rows = CsvExporter::new(CsvExportPolicy {
        max_rows: 10,
        batch_size: 1,
        ..CsvExportPolicy::default()
    })
    .with_columns([CsvColumn::new("id", "ID"), CsvColumn::new("name", "Name")])
    .export::<User, _>(&search, &query, &ServerScope::unrestricted(), &mut output)
    .await
    .unwrap();
    assert_eq!(rows, 3);
    assert!(String::from_utf8(output).unwrap().contains("ID,Name"));
    tokio::time::timeout(
        std::time::Duration::from_secs(2),
        sqlx::query("SELECT 1").execute(&pool),
    )
    .await
    .expect("the single MySQL pool connection must be reusable after scan/export")
    .unwrap();
}
