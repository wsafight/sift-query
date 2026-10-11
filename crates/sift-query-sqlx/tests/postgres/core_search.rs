use super::support::*;

/// Requires a disposable PostgreSQL database named by `DATABASE_URL`.
#[tokio::test]
#[ignore = "requires a disposable PostgreSQL instance in DATABASE_URL"]
async fn executes_list_count_map_and_sum_against_postgres() -> Result<(), Box<dyn std::error::Error>>
{
    let database_url = std::env::var("DATABASE_URL")?;
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&database_url)
        .await?;

    let _lock = serialise(&pool).await?;
    sqlx::query("CREATE SCHEMA IF NOT EXISTS sift_query_test")
        .execute(&pool)
        .await?;
    for table in ["users", "scores", "pg_types"] {
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "DROP TABLE IF EXISTS sift_query_test.{table}"
        )))
        .execute(&pool)
        .await?;
    }
    sqlx::query(
        "CREATE TABLE sift_query_test.users (\
             id BIGINT PRIMARY KEY, \
             name TEXT NOT NULL, \
             status INTEGER NOT NULL, \
             age INTEGER NOT NULL, \
             deleted BOOLEAN NOT NULL DEFAULT FALSE\
         )",
    )
    .execute(&pool)
    .await?;
    sqlx::query(
        "INSERT INTO sift_query_test.users (id, name, status, age, deleted) VALUES \
         (1, 'Alice', 1, 20, false), \
         (2, 'Bob', 2, 30, false), \
         (3, 'Hidden', 1, 40, true), \
         (4, 'Amy', 1, 25, false)",
    )
    .execute(&pool)
    .await?;
    sqlx::query(
        "CREATE TABLE sift_query_test.scores (\
             course_id BIGINT NOT NULL, \
             score INTEGER NOT NULL\
         )",
    )
    .execute(&pool)
    .await?;
    sqlx::query(
        "INSERT INTO sift_query_test.scores (course_id, score) VALUES \
         (10, 10), (10, 20), (20, 40), (30, 5)",
    )
    .execute(&pool)
    .await?;
    sqlx::query(
        "CREATE TABLE sift_query_test.pg_types (\
             id BIGINT PRIMARY KEY, flag BOOLEAN NOT NULL, ratio DOUBLE PRECISION NOT NULL, \
             title TEXT NOT NULL, data BYTEA NOT NULL, event_date DATE NOT NULL, \
             event_time TIME NOT NULL, local_at TIMESTAMP NOT NULL, \
             observed_at TIMESTAMPTZ NOT NULL, external_id UUID NOT NULL, \
             amount NUMERIC NOT NULL, payload JSONB NOT NULL\
         )",
    )
    .execute(&pool)
    .await?;
    sqlx::query(
        "INSERT INTO sift_query_test.pg_types VALUES (\
             1, true, 1.5, 'typed', decode('fbff', 'hex'), DATE '2026-10-08', \
             TIME '12:34:56', TIMESTAMP '2026-10-08 12:34:56', \
             TIMESTAMPTZ '2026-10-08 12:34:56+08', \
             UUID '123e4567-e89b-12d3-a456-426614174000', 12.50, '{\"ok\":true}'::jsonb\
         )",
    )
    .execute(&pool)
    .await?;

    let search = PgSiftQuery::new(pool.clone());
    let scope = ServerScope::<User>::unrestricted();
    let raw = RawParams::parse("status=1&status=2&status-op=il&size=10")?;
    let page = search.search::<User>(&raw, &scope).await?;
    assert_eq!(page.total_count, 3);
    assert_eq!(page.data.len(), 3);
    assert_eq!(page.data[0].name, "Alice");

    let not_in = search
        .search_list::<User>(&RawParams::parse("status=2&status-op=ni&size=10")?, &scope)
        .await?;
    assert_eq!(
        not_in.iter().map(|user| user.id).collect::<Vec<_>>(),
        [1, 4]
    );
    let not_between = search
        .search_list::<User>(
            &RawParams::parse("age=20&age=25&age-op=nb&size=10")?,
            &scope,
        )
        .await?;
    assert_eq!(not_between.len(), 1);
    assert_eq!(not_between[0].name, "Bob");
    let ends_with = search
        .search_list::<User>(&RawParams::parse("name=ice&name-op=ew&size=10")?, &scope)
        .await?;
    assert_eq!(ends_with.len(), 1);
    assert_eq!(ends_with[0].name, "Alice");

    let map_raw = RawParams::parse("onlySelect=id,name&size=10")?;
    let map_page = search.search_map::<User>(&map_raw, &scope).await?;
    assert_eq!(map_page.total_count, 3);
    assert_eq!(map_page.data[0].len(), 2);

    let projected = search.search_as::<User, UserSummary>(&raw, &scope).await?;
    assert_eq!(projected.total_count, 3);
    assert_eq!(projected.data[0].username, "Alice");

    let sum = search
        .sum::<User, i32, _>(&raw, &scope, UserFields::AGE)
        .await?;
    assert_eq!(sum, Some(75_i64));

    let status_scope = ServerScope::<User>::new()
        .eq(UserFields::STATUS, 1_i32)
        .build()?;
    let scoped_count = search
        .count::<User>(&RawParams::parse("")?, &status_scope)
        .await?;
    assert_eq!(scoped_count, 2);

    let cursor_search = PgSiftQuery::with_policy(
        pool.clone(),
        QueryPolicy::default().with_cursor_codec(CursorCodec::new([0x33; 32])?),
        PgExecutionPolicy::default(),
    )?;
    let first_cursor_page = cursor_search
        .search::<User>(
            &RawParams::parse("cursor=&size=2")?,
            &ServerScope::unrestricted(),
        )
        .await?;
    assert_eq!(first_cursor_page.total_count, 3);
    assert_eq!(first_cursor_page.data.len(), 2);
    let cursor = first_cursor_page.next_cursor.expect("full page has cursor");
    let second_cursor_page = cursor_search
        .search::<User>(
            &RawParams::parse(&format!("cursor={cursor}&size=2"))?,
            &ServerScope::unrestricted(),
        )
        .await?;
    assert_eq!(second_cursor_page.total_count, 3);
    assert_eq!(second_cursor_page.data.len(), 1);
    assert_eq!(second_cursor_page.data[0].id, 4);
    assert!(second_cursor_page.next_cursor.is_none());

    let distinct = search
        .search::<DistinctStatus>(&RawParams::parse("size=10")?, &ServerScope::unrestricted())
        .await?;
    assert_eq!(distinct.total_count, 2);
    assert_eq!(distinct.data.len(), 2);

    let group_raw = RawParams::parse("total=30&total-op=ge&size=10")?;
    let options = SearchOptions::<CourseTotal>::builder()
        .sum(CourseTotalFields::TOTAL)
        .build()?;
    let grouped = search
        .search_with::<CourseTotal>(&group_raw, &ServerScope::unrestricted(), &options)
        .await?;
    assert_eq!(grouped.total_count, 2);
    assert_eq!(grouped.data.len(), 2);
    assert_eq!(
        grouped.summaries[0].value,
        Some(RowValue::Decimal(rust_decimal::Decimal::from(70)))
    );

    let grouped_sum = search
        .sum::<CourseTotal, i64, _>(
            &group_raw,
            &ServerScope::unrestricted(),
            CourseTotalFields::TOTAL,
        )
        .await?;
    assert_eq!(grouped_sum, Some(rust_decimal::Decimal::from(70)));

    let type_page = search
        .search::<PgTypes>(&RawParams::parse("size=10")?, &ServerScope::unrestricted())
        .await?;
    assert_eq!(type_page.data.len(), 1);
    assert_eq!(type_page.data[0].title, "typed");
    let type_map = search
        .search_map::<PgTypes>(&RawParams::parse("size=10")?, &ServerScope::unrestricted())
        .await?;
    assert_eq!(type_map.data[0]["id"], RowValue::I64(1));
    assert_eq!(type_map.data[0]["flag"], RowValue::Bool(true));
    assert_eq!(
        type_map.data[0]["external_id"],
        RowValue::Uuid(uuid::Uuid::parse_str(
            "123e4567-e89b-12d3-a456-426614174000"
        )?)
    );

    let aggregate_options = SearchOptions::<User>::builder()
        .sum(UserFields::AGE)
        .avg(UserFields::AGE)
        .min(UserFields::AGE)
        .max(UserFields::AGE)
        .custom(
            UserFields::AGE,
            CustomAggregate::new("present", "count({field})", ValueType::I64)?,
        )
        .build()?;
    let aggregate_page = search
        .search_with::<User>(&raw, &scope, &aggregate_options)
        .await?;
    assert_eq!(aggregate_page.summaries.len(), 5);
    assert_eq!(aggregate_page.summaries[0].value, Some(RowValue::I64(75)));
    assert_eq!(
        aggregate_page.summaries[1].value,
        Some(RowValue::Decimal(rust_decimal::Decimal::from(25)))
    );
    assert_eq!(aggregate_page.summaries[2].value, Some(RowValue::I32(20)));
    assert_eq!(aggregate_page.summaries[3].value, Some(RowValue::I32(30)));
    assert_eq!(aggregate_page.summaries[4].value, Some(RowValue::I64(3)));

    let window_search = PgSiftQuery::with_policy(
        pool.clone(),
        QueryPolicy::default(),
        PgExecutionPolicy {
            count_strategy: CountStrategy::WindowWhenPossible,
            shape_cache_capacity: 16,
            ..PgExecutionPolicy::default()
        },
    )?;
    let window_page = window_search
        .search::<User>(&RawParams::parse("page=99&size=2")?, &scope)
        .await?;
    assert!(window_page.data.is_empty());
    assert_eq!(window_page.total_count, 3);
    let _ = window_search.search::<User>(&raw, &scope).await?;
    let _ = window_search.search::<User>(&raw, &scope).await?;
    assert!(window_search.shape_cache_stats().unwrap().hits > 0);

    for strategy in [
        ExecutionStrategy::ParallelBestEffort,
        ExecutionStrategy::Snapshot(SnapshotIsolation::RepeatableRead),
        ExecutionStrategy::Snapshot(SnapshotIsolation::Serializable),
    ] {
        let strategy_search = PgSiftQuery::with_policy(
            pool.clone(),
            QueryPolicy::default(),
            PgExecutionPolicy {
                strategy,
                max_parallel_queries: 2,
                ..PgExecutionPolicy::default()
            },
        )?;
        let strategy_page = strategy_search.search::<User>(&raw, &scope).await?;
        assert_eq!(strategy_page.total_count, 3);
        assert_eq!(strategy_page.data.len(), 3);
    }

    let seen_sql = Arc::new(AtomicBool::new(false));
    let hooked_search = PgSiftQuery::new(pool.clone())
        .with_partition_resolver(UsersPartition)
        .with_sql_interceptor(CommentInterceptor(Arc::clone(&seen_sql)))
        .with_result_filter(RemoveName);
    let filtered_page = hooked_search.search_map::<User>(&map_raw, &scope).await?;
    assert!(seen_sql.load(Ordering::Relaxed));
    assert_eq!(filtered_page.total_count, 3);
    assert!(!filtered_page.data[0].contains_key("name"));

    let mut stream = search.search_stream::<User>(&raw, &scope)?;
    let mut streamed = Vec::new();
    while let Some(row) = stream.try_next().await? {
        streamed.push(row);
    }
    assert_eq!(streamed.len(), 3);
    assert_eq!(streamed[0].id, 1);

    let single_pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&database_url)
        .await?;
    let stream_search = PgSiftQuery::new(single_pool.clone());
    let mut interrupted = stream_search.search_stream::<User>(&raw, &scope)?;
    assert!(interrupted.try_next().await?.is_some());
    drop(interrupted);
    let one: i32 = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        sqlx::query_scalar("SELECT 1").fetch_one(&single_pool),
    )
    .await??;
    assert_eq!(one, 1);

    for table in ["users", "scores", "pg_types"] {
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "DROP TABLE IF EXISTS sift_query_test.{table}"
        )))
        .execute(&pool)
        .await?;
    }
    Ok(())
}
