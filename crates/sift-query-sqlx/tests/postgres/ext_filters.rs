use super::ext_support::*;
use super::support::*;
use super::wide_support::*;

/// Exercises the extension failure and filtering paths.
#[tokio::test]
#[ignore = "requires a disposable PostgreSQL instance in DATABASE_URL"]
async fn extensions_filter_rows_and_report_failures() -> Result<(), Box<dyn std::error::Error>> {
    let database_url = std::env::var("DATABASE_URL")?;
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&database_url)
        .await?;
    let _lock = serialise(&pool).await?;
    create_wide_table(&pool).await?;
    let scope = ServerScope::<Wide>::unrestricted();

    let filtered = PgSiftQuery::new(pool.clone()).with_result_filter(DropTitle);
    let page = filtered
        .search_map::<Wide>(&RawParams::parse("onlySelect=id,title&size=10")?, &scope)
        .await?;
    assert_eq!(page.data.len(), 1);
    assert_eq!(page.data[0]["title"], RowValue::Text("alpha".to_owned()));
    // Discarded rows are not reflected in the database count.
    assert_eq!(page.total_count, 2);

    // A partition resolver that declines leaves the SQL untouched.
    let declined = PgSiftQuery::new(pool.clone()).with_partition_resolver(NullPartition);
    let untouched = declined
        .search_list::<Wide>(&RawParams::parse("size=10")?, &scope)
        .await?;
    assert_eq!(untouched.len(), 2);

    let broken = PgSiftQuery::new(pool.clone())
        .with_partition_resolver(RewriteToWide)
        .with_sql_interceptor(RejectWide);
    let error = broken
        .search_list::<Wide>(&RawParams::parse("size=10")?, &scope)
        .await
        .unwrap_err();
    assert!(
        matches!(
            error,
            PgSearchError::Extension(ExtensionError::Rejected {
                extension: "test",
                ..
            })
        ),
        "{error:?}"
    );

    sqlx::query("DROP TABLE sift_query_test.wide")
        .execute(&pool)
        .await?;
    Ok(())
}
