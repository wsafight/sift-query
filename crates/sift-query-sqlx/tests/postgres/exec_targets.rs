use super::support::*;
use super::wide_support::*;

/// Exercises every execution target, including cursor pagination for typed,
/// map, and static-projection results.
#[tokio::test]
#[ignore = "requires a disposable PostgreSQL instance in DATABASE_URL"]
async fn exercises_every_execution_target() -> Result<(), Box<dyn std::error::Error>> {
    let database_url = std::env::var("DATABASE_URL")?;
    let pool = PgPoolOptions::new()
        .max_connections(6)
        .connect(&database_url)
        .await?;
    let _lock = serialise(&pool).await?;
    create_wide_table(&pool).await?;

    let codec = CursorCodec::new([0x5c; 32])?;
    let search = PgSiftQuery::with_policy(
        pool.clone(),
        QueryPolicy::default().with_cursor_codec(codec.clone()),
        PgExecutionPolicy::default(),
    )?;
    let scope = ServerScope::<Wide>::unrestricted();
    let cursor_scope = ServerScope::<WideCursor>::unrestricted();

    // `search_first` returns one row and rejects explicit paging.
    let first = search
        .search_first::<Wide>(&RawParams::parse("")?, &scope)
        .await?;
    assert_eq!(first.map(|row| row.id), Some(1));
    assert!(
        search
            .search_first::<Wide>(&RawParams::parse("size=1")?, &scope)
            .await
            .is_err()
    );

    // `search_list` honours a zero page size without issuing a query.
    let empty = search
        .search_list::<Wide>(&RawParams::parse("size=0")?, &scope)
        .await?;
    assert!(empty.is_empty());

    // A page past the end still reports the full count.
    let beyond = search
        .search::<Wide>(&RawParams::parse("page=5&size=10")?, &scope)
        .await?;
    assert!(beyond.data.is_empty());
    assert_eq!(beyond.total_count, 2);

    // Cursor pagination across typed, map, and projection results.
    let first_page = search
        .search::<WideCursor>(&RawParams::parse("cursor=&size=1")?, &cursor_scope)
        .await?;
    let cursor = first_page
        .next_cursor
        .clone()
        .expect("full page has a cursor");
    let second_page = search
        .search::<WideCursor>(
            &RawParams::parse(&format!("cursor={cursor}&size=1"))?,
            &cursor_scope,
        )
        .await?;
    assert_eq!(second_page.data[0].id, 2);
    // The cursor is minted from the last row whenever the page is full, so a
    // further page is still offered; it simply returns no rows.
    let third_cursor = second_page
        .next_cursor
        .clone()
        .expect("full page has a cursor");
    let third_page = search
        .search::<WideCursor>(
            &RawParams::parse(&format!("cursor={third_cursor}&size=1"))?,
            &cursor_scope,
        )
        .await?;
    assert!(third_page.data.is_empty());
    assert!(third_page.next_cursor.is_none());

    let map_first = search
        .search_map::<WideCursor>(
            &RawParams::parse("cursor=&size=1&onlySelect=id,title")?,
            &cursor_scope,
        )
        .await?;
    let map_cursor = map_first
        .next_cursor
        .clone()
        .expect("map page has a cursor");
    let map_second = search
        .search_map::<WideCursor>(
            &RawParams::parse(&format!("cursor={map_cursor}&size=1&onlySelect=id,title"))?,
            &cursor_scope,
        )
        .await?;
    assert_eq!(map_second.data.len(), 1);
    assert_eq!(map_second.data[0]["id"], RowValue::I64(2));

    let projected_first = search
        .search_as::<WideCursor, WideProjection>(
            &RawParams::parse("cursor=&size=1")?,
            &cursor_scope,
        )
        .await?;
    let projected_cursor = projected_first
        .next_cursor
        .clone()
        .expect("projection page has a cursor");
    let projected_second = search
        .search_as::<WideCursor, WideProjection>(
            &RawParams::parse(&format!("cursor={projected_cursor}&size=1"))?,
            &cursor_scope,
        )
        .await?;
    assert_eq!(projected_second.data.len(), 1);
    assert_eq!(projected_second.data[0].heading, "beta");

    // A forced-empty page short-circuits every target.
    let compatible = PgSiftQuery::with_policy(
        pool.clone(),
        QueryPolicy::bean_searcher_4812().with_cursor_codec(codec),
        PgExecutionPolicy::default(),
    )?;
    let forced = compatible
        .search::<WideCursor>(&RawParams::parse("page=0")?, &cursor_scope)
        .await?;
    assert!(forced.data.is_empty());
    assert_eq!(forced.total_count, 0);
    assert!(
        compatible
            .search_list::<WideCursor>(&RawParams::parse("page=0")?, &cursor_scope)
            .await?
            .is_empty()
    );
    assert_eq!(
        compatible
            .search_map::<WideCursor>(&RawParams::parse("page=0")?, &cursor_scope)
            .await?
            .data,
        [] as [indexmap::map::IndexMap<std::string::String, sift_query::RowValue>; 0]
    );
    assert!(
        compatible
            .search_as::<WideCursor, WideProjection>(&RawParams::parse("page=0")?, &cursor_scope)
            .await?
            .data
            .is_empty()
    );
    assert!(
        compatible
            .search_first::<WideCursor>(&RawParams::parse("page=0")?, &cursor_scope)
            .await?
            .is_none()
    );
    assert_eq!(
        compatible
            .count::<WideCursor>(&RawParams::parse("page=0")?, &cursor_scope)
            .await?,
        0
    );
    assert_eq!(
        compatible
            .sum::<WideCursor, i64, _>(
                &RawParams::parse("page=0")?,
                &cursor_scope,
                WideCursorFields::ID,
            )
            .await?,
        None
    );
    let mut stream =
        compatible.search_stream::<WideCursor>(&RawParams::parse("page=0")?, &cursor_scope)?;
    assert!(stream.try_next().await?.is_none());

    // An empty stream is also produced by a zero page size.
    let mut zero_stream =
        search.search_stream::<WideCursor>(&RawParams::parse("size=0")?, &cursor_scope)?;
    assert!(zero_stream.try_next().await?.is_none());

    // `sum` for every supported input type.
    assert_eq!(
        search
            .sum::<Wide, i32, _>(&RawParams::parse("")?, &scope, WideFields::COUNT32)
            .await?,
        Some(15_i64)
    );
    assert_eq!(
        search
            .sum::<Wide, f32, _>(&RawParams::parse("")?, &scope, WideFields::RATIO32)
            .await?,
        Some(5.0_f32)
    );
    assert_eq!(
        search
            .sum::<Wide, f64, _>(&RawParams::parse("")?, &scope, WideFields::RATIO64)
            .await?,
        Some(7.0_f64)
    );
    assert_eq!(
        search
            .sum::<Wide, i64, _>(&RawParams::parse("")?, &scope, WideFields::ID)
            .await?,
        Some(rust_decimal::Decimal::from(3))
    );
    assert_eq!(
        search
            .sum::<Wide, rust_decimal::Decimal, _>(
                &RawParams::parse("")?,
                &scope,
                WideFields::AMOUNT
            )
            .await?,
        Some(rust_decimal::Decimal::from(3250) / rust_decimal::Decimal::from(100))
    );

    // Summaries for every aggregate function, including a custom one.
    // PostgreSQL has no `min`/`max` aggregate for `uuid`, so the summary set
    // stays within the types the database actually supports.
    let options = SearchOptions::<Wide>::builder()
        .sum(WideFields::COUNT32)
        .avg(WideFields::COUNT32)
        .min(WideFields::COUNT32)
        .max(WideFields::COUNT32)
        .min(WideFields::TITLE)
        .min(WideFields::DAY)
        .min(WideFields::INSTANT)
        .min(WideFields::AMOUNT)
        .build()?;
    let summarized = search
        .search_with::<Wide>(&RawParams::parse("")?, &scope, &options)
        .await?;
    assert_eq!(summarized.summaries.len(), 8);
    assert_eq!(summarized.summaries[0].value, Some(RowValue::I64(15)));
    assert_eq!(
        summarized.summaries[1].value,
        Some(RowValue::Decimal(rust_decimal::Decimal::new(75, 1)))
    );
    assert_eq!(summarized.summaries[2].value, Some(RowValue::I32(7)));
    assert_eq!(summarized.summaries[3].value, Some(RowValue::I32(8)));
    assert_eq!(
        summarized.summaries[4].value,
        Some(RowValue::Text("alpha".to_owned()))
    );
    assert_eq!(
        summarized.summaries[5].value,
        Some(RowValue::Date(
            chrono::NaiveDate::from_ymd_opt(2026, 10, 8).unwrap()
        ))
    );
    assert!(matches!(
        summarized.summaries[6].value,
        Some(RowValue::Timestamp(_))
    ));
    assert_eq!(
        summarized.summaries[7].value,
        Some(RowValue::Decimal(rust_decimal::Decimal::new(1250, 2)))
    );

    // Floating-point aggregates are checked in a second request set.
    let float_options = SearchOptions::<Wide>::builder()
        .sum(WideFields::RATIO32)
        .avg(WideFields::RATIO32)
        .sum(WideFields::RATIO64)
        .avg(WideFields::RATIO64)
        .build()?;
    let floats = search
        .search_with::<Wide>(&RawParams::parse("")?, &scope, &float_options)
        .await?;
    assert_eq!(floats.summaries[0].value, Some(RowValue::F32(5.0)));
    // `avg(real)` is computed in double precision by PostgreSQL.
    assert_eq!(floats.summaries[1].value, Some(RowValue::F64(2.5)));
    assert_eq!(floats.summaries[2].value, Some(RowValue::F64(7.0)));
    assert_eq!(floats.summaries[3].value, Some(RowValue::F64(3.5)));

    // Every decoded column type is visible through the map target.
    let mapped = search
        .search_map::<Wide>(&RawParams::parse("onlySelect=id,flag,count32,ratio32,ratio64,title,data,day,clock,local_at,instant,external,amount,payload")?, &scope)
        .await?;
    let row = &mapped.data[0];
    assert_eq!(row["id"], RowValue::I64(1));
    assert_eq!(row["flag"], RowValue::Bool(true));
    assert_eq!(row["count32"], RowValue::I32(7));
    assert_eq!(row["ratio32"], RowValue::F32(1.5));
    assert_eq!(row["ratio64"], RowValue::F64(2.5));
    assert_eq!(row["title"], RowValue::Text("alpha".to_owned()));
    assert_eq!(row["data"], RowValue::Bytes(vec![0xfb, 0xff]));
    assert_eq!(
        row["day"],
        RowValue::Date(chrono::NaiveDate::from_ymd_opt(2026, 10, 8).unwrap())
    );
    assert_eq!(
        row["clock"],
        RowValue::Time(chrono::NaiveTime::from_hms_opt(12, 34, 56).unwrap())
    );
    assert_eq!(
        row["local_at"],
        RowValue::DateTime(
            chrono::NaiveDate::from_ymd_opt(2026, 10, 8)
                .unwrap()
                .and_hms_opt(12, 34, 56)
                .unwrap()
        )
    );
    assert!(matches!(row["instant"], RowValue::Timestamp(_)));
    assert!(matches!(row["external"], RowValue::Uuid(_)));
    assert!(matches!(row["amount"], RowValue::Decimal(_)));
    assert_eq!(
        row["payload"],
        RowValue::Json(serde_json::json!({"ok": true}))
    );

    // Window counting is used for map targets as well.
    let windowed = PgSiftQuery::with_policy(
        pool.clone(),
        QueryPolicy::default(),
        PgExecutionPolicy {
            count_strategy: CountStrategy::WindowWhenPossible,
            ..PgExecutionPolicy::default()
        },
    )?;
    let window_map = windowed
        .search_map::<Wide>(&RawParams::parse("onlySelect=id&size=1")?, &scope)
        .await?;
    assert_eq!(window_map.total_count, 2);
    assert_eq!(window_map.data.len(), 1);

    // The static projection window path uses its own decoder.
    let window_projection = windowed
        .search_as::<WideCursor, WideProjection>(&RawParams::parse("size=1")?, &cursor_scope)
        .await?;
    assert_eq!(window_projection.total_count, 2);
    assert_eq!(window_projection.data[0].heading, "alpha");

    // A page past the end makes the window query return no rows, which forces
    // the fallback count query.
    let fallback = windowed
        .search::<WideCursor>(&RawParams::parse("page=5&size=1")?, &cursor_scope)
        .await?;
    assert!(fallback.data.is_empty());
    assert_eq!(fallback.total_count, 2);
    let fallback_map = windowed
        .search_map::<WideCursor>(
            &RawParams::parse("page=5&size=1&onlySelect=id")?,
            &cursor_scope,
        )
        .await?;
    assert_eq!(
        fallback_map.data,
        [] as [indexmap::map::IndexMap<std::string::String, sift_query::RowValue>; 0]
    );
    assert_eq!(fallback_map.total_count, 2);

    // The shape cache is observable and clearable.
    let cached = PgSiftQuery::with_policy(
        pool.clone(),
        QueryPolicy::default(),
        PgExecutionPolicy {
            shape_cache_capacity: 4,
            ..PgExecutionPolicy::default()
        },
    )?;
    let _ = cached
        .search::<WideCursor>(&RawParams::parse("")?, &cursor_scope)
        .await?;
    let _ = cached
        .search::<WideCursor>(&RawParams::parse("")?, &cursor_scope)
        .await?;
    assert!(cached.shape_cache_stats().unwrap().hits > 0);
    cached.clear_shape_cache();
    assert_eq!(cached.shape_cache_stats().unwrap().entries, 0);
    assert!(PgSiftQuery::new(pool.clone()).shape_cache_stats().is_none());

    // Execution strategies with no list query to run.
    for strategy in [
        ExecutionStrategy::ParallelBestEffort,
        ExecutionStrategy::Snapshot(SnapshotIsolation::RepeatableRead),
        ExecutionStrategy::Snapshot(SnapshotIsolation::Serializable),
    ] {
        let no_list = PgSiftQuery::with_policy(
            pool.clone(),
            QueryPolicy::default(),
            PgExecutionPolicy {
                strategy,
                ..PgExecutionPolicy::default()
            },
        )?;
        let page = no_list
            .search::<WideCursor>(&RawParams::parse("size=0")?, &cursor_scope)
            .await?;
        assert!(page.data.is_empty());
        assert_eq!(page.total_count, 2);
        let map_page = no_list
            .search_map::<WideCursor>(&RawParams::parse("size=0")?, &cursor_scope)
            .await?;
        assert_eq!(map_page.total_count, 2);
    }

    sqlx::query("DROP TABLE sift_query_test.wide")
        .execute(&pool)
        .await?;
    Ok(())
}
