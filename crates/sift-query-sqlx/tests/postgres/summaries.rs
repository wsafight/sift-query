use super::support::*;
use super::wide_support::*;

/// Covers the summary-options entry points and the window-count fallback.
#[tokio::test]
#[ignore = "requires a disposable PostgreSQL instance in DATABASE_URL"]
async fn summary_options_reach_every_search_entry_point() -> Result<(), Box<dyn std::error::Error>>
{
    let database_url = std::env::var("DATABASE_URL")?;
    let pool = PgPoolOptions::new()
        .max_connections(6)
        .connect(&database_url)
        .await?;
    let _lock = serialise(&pool).await?;
    create_wide_table(&pool).await?;
    let scope = ServerScope::<Wide>::unrestricted();
    let options = SearchOptions::<Wide>::builder()
        .sum(WideFields::COUNT32)
        .min(WideFields::TITLE)
        .build()?;
    let raw = RawParams::parse("size=10")?;

    // The typed search and the static projection both accept summaries.
    let search = PgSiftQuery::new(pool.clone());
    let page = search.search_with::<Wide>(&raw, &scope, &options).await?;
    assert_eq!(page.summaries.len(), 2);
    assert_eq!(page.summaries[0].value, Some(RowValue::I64(15)));

    let cursor_scope = ServerScope::<WideCursor>::unrestricted();
    let cursor_options = SearchOptions::<WideCursor>::builder()
        .sum(WideCursorFields::ID)
        .build()?;
    let projected = search
        .search_as_with::<WideCursor, WideProjection>(&raw, &cursor_scope, &cursor_options)
        .await?;
    assert_eq!(projected.summaries.len(), 1);
    assert_eq!(projected.data.len(), 2);

    // Summaries force the separate count strategy: the window shape cannot
    // carry them.
    let windowed = PgSiftQuery::with_policy(
        pool.clone(),
        QueryPolicy::default(),
        PgExecutionPolicy {
            count_strategy: CountStrategy::WindowWhenPossible,
            ..PgExecutionPolicy::default()
        },
    )?;
    let windowed_page = windowed.search_with::<Wide>(&raw, &scope, &options).await?;
    assert_eq!(windowed_page.summaries.len(), 2);
    assert_eq!(windowed_page.total_count, 2);

    // A map search with summaries keeps the dynamic row shape.
    let map_page = windowed
        .search_map_with::<Wide>(&raw, &scope, &options)
        .await?;
    assert_eq!(map_page.summaries.len(), 2);
    assert_eq!(map_page.total_count, 2);

    sqlx::query("DROP TABLE sift_query_test.wide")
        .execute(&pool)
        .await?;
    Ok(())
}
