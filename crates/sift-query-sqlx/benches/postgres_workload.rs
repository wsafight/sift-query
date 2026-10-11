//! Opt-in reproducible database workload. Requires a disposable `DATABASE_URL`.
extern crate sift_query_core as sift_query;
use futures_util::TryStreamExt;
use sift_query_core::*;
use sift_query_derive::SearchEntity;
use sift_query_sqlx::*;
use std::error::Error;
use std::time::{Duration, Instant};

#[derive(Debug, sqlx::FromRow, SearchEntity)]
#[search_entity(
    from = "sift_benchmark.items",
    default_order = "id:asc",
    stable_order = "id:asc",
    max_offset = 40000
)]
struct Item {
    #[search_field(sortable, cursor_non_null)]
    id: i64,
    status: i32,
}

#[derive(Debug, sqlx::FromRow, SearchEntity)]
#[search_entity(
    from = "sift_benchmark.items",
    default_order = "id:asc",
    stable_order = "id:asc",
    max_offset = 40000
)]
struct UnspecifiedItem {
    #[search_field(sortable)]
    id: i64,
    status: i32,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(4)
        .connect(&std::env::var("DATABASE_URL")?)
        .await?;
    // Existing schemas fail rather than being silently destroyed.
    sqlx::raw_sql("CREATE SCHEMA sift_benchmark; CREATE TABLE sift_benchmark.items(id bigint PRIMARY KEY,status integer NOT NULL); INSERT INTO sift_benchmark.items SELECT i,(i%10)::integer FROM generate_series(1,50000) i; CREATE INDEX ON sift_benchmark.items(status,id); ANALYZE sift_benchmark.items;").execute(&pool).await?;
    let result = Box::pin(workload(&pool)).await;
    sqlx::query("DROP SCHEMA sift_benchmark CASCADE")
        .execute(&pool)
        .await?;
    pool.close().await;
    result
}

async fn workload(pool: &sqlx::PgPool) -> Result<(), Box<dyn Error + Send + Sync>> {
    let version: String = sqlx::query_scalar("SHOW server_version")
        .fetch_one(pool)
        .await?;
    println!(
        "PostgreSQL {version}; rows=50000; status distribution=10 uniform groups; indexes=PK(id),Btree(status,id); client concurrency=1; pool=4; warmup=3; samples=20; release build"
    );
    // Raise only this server-owned benchmark's offset budget, explicitly.
    let limits = QueryLimits {
        max_offset: 40000,
        ..QueryLimits::default()
    };
    let policy = QueryPolicy::with_hard_limits(
        ParamNames::default(),
        limits.clone(),
        QueryHardLimits(limits),
    )?
    .with_cursor_codec(CursorCodec::new([0x33; 32])?);
    let scope = ServerScope::unrestricted();
    let raw = RawParams::parse("status=1&size=50")?;
    for (label, strategy, count) in [
        (
            "sequential",
            ExecutionStrategy::SequentialBestEffort,
            CountStrategy::Separate,
        ),
        (
            "parallel",
            ExecutionStrategy::ParallelBestEffort,
            CountStrategy::Separate,
        ),
        (
            "snapshot_repeatable",
            ExecutionStrategy::Snapshot(SnapshotIsolation::RepeatableRead),
            CountStrategy::Separate,
        ),
        (
            "snapshot_serializable",
            ExecutionStrategy::Snapshot(SnapshotIsolation::Serializable),
            CountStrategy::Separate,
        ),
        (
            "window_count",
            ExecutionStrategy::SequentialBestEffort,
            CountStrategy::WindowWhenPossible,
        ),
    ] {
        let search = PgSiftQuery::with_policy(
            pool.clone(),
            policy.clone(),
            PgExecutionPolicy {
                strategy,
                count_strategy: count,
                ..PgExecutionPolicy::default()
            },
        )?;
        let mut samples = Vec::new();
        for i in 0..23 {
            let start = Instant::now();
            let page = search.search::<Item>(&raw, &scope).await?;
            assert_eq!(page.total_count, 5000);
            assert_eq!(page.data.len(), 50);
            assert!(page.data.iter().all(|row| row.status == 1));
            if i >= 3 {
                samples.push(start.elapsed());
            }
        }
        report(label, samples);
        if label == "sequential" {
            explain(pool, &policy, &raw, &scope).await?;
        }
    }
    let search =
        PgSiftQuery::with_policy(pool.clone(), policy.clone(), PgExecutionPolicy::default())?;
    let order = [OrderBy {
        field: ItemFields::ID.id(),
        direction: SortOrder::Asc,
        nulls: NullOrder::Last,
    }];
    let token = policy
        .cursor_codec()
        .unwrap()
        .encode::<Item>(&order, &[CursorValue::Scalar(ScalarValue::I64(40000))])?;
    let unspecified_token = policy
        .cursor_codec()
        .unwrap()
        .encode::<UnspecifiedItem>(&order, &[CursorValue::Scalar(ScalarValue::I64(40000))])?;
    let unspecified_raw = RawParams::from_entries(
        [
            ("cursor".to_owned(), unspecified_token),
            ("size".to_owned(), "50".to_owned()),
        ],
        policy.limits(),
    )?;
    let unspecified_scope = ServerScope::unrestricted();
    let mut samples = Vec::new();
    for i in 0..23 {
        let start = Instant::now();
        let page = search
            .search::<UnspecifiedItem>(&unspecified_raw, &unspecified_scope)
            .await?;
        assert_eq!(page.total_count, 50000);
        assert_eq!((page.data[0].id, page.data[0].status), (40001, 1));
        if i >= 3 {
            samples.push(start.elapsed());
        }
    }
    report("cursor_unspecified_key_after_40000", samples);
    explain(pool, &policy, &unspecified_raw, &unspecified_scope).await?;
    for (label, raw) in [
        ("offset_40000", RawParams::parse("offset=40000&size=50")?),
        (
            "cursor_after_40000",
            RawParams::from_entries(
                [
                    ("cursor".to_owned(), token),
                    ("size".to_owned(), "50".to_owned()),
                ],
                policy.limits(),
            )?,
        ),
    ] {
        let mut samples = Vec::new();
        for i in 0..23 {
            let start = Instant::now();
            let page = search.search::<Item>(&raw, &scope).await?;
            assert_eq!(page.total_count, 50000);
            assert_eq!(page.data[0].id, 40001);
            if i >= 3 {
                samples.push(start.elapsed());
            }
        }
        report(label, samples);
        explain(pool, &policy, &raw, &scope).await?;
    }
    let scan_scope = ServerScope::<Item>::new()
        .le(ItemFields::ID, 10000)
        .build()?;
    let mut samples = Vec::new();
    for i in 0..23 {
        let start = Instant::now();
        let mut stream = search.scan_map_stream::<Item>(
            &RawParams::default(),
            &scan_scope,
            ScanPolicy::default(),
        )?;
        let mut count = 0;
        while stream.try_next().await?.is_some() {
            count += 1;
        }
        assert_eq!(count, 10000);
        if i >= 3 {
            samples.push(start.elapsed());
        }
    }
    report("stream_10000", samples);
    Ok(())
}

async fn explain<E: SearchEntity>(
    pool: &sqlx::PgPool,
    policy: &QueryPolicy,
    raw: &RawParams,
    scope: &ServerScope<E>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    use sqlx::Arguments;
    let query = ParamParser::new(policy)?.parse::<E>(raw)?;
    let plan = QueryPlan::new(&query, scope, QueryTarget::TypedPage)?;
    let rendered = PgRenderer.render_list(&plan)?;
    let mut arguments = sqlx::postgres::PgArguments::default();
    for bind in rendered.binds {
        match bind {
            PgBindValue::Scalar(ScalarValue::I32(value)) => arguments.add(value)?,
            PgBindValue::Scalar(ScalarValue::I64(value)) => arguments.add(value)?,
            _ => return Err(std::io::Error::other("unexpected benchmark bind type").into()),
        }
    }
    // Explain the actual rendered query with its binds, preserving NULL semantics.
    let sql = format!("EXPLAIN (ANALYZE, BUFFERS, FORMAT TEXT) {}", rendered.sql);
    let lines: Vec<String> = sqlx::query_scalar_with(sqlx::AssertSqlSafe(sql), arguments)
        .fetch_all(pool)
        .await?;
    println!("{}", lines.join("\n"));
    Ok(())
}

fn report(label: &str, mut samples: Vec<Duration>) {
    samples.sort_unstable();
    println!(
        "{label}: median={:.3}ms p95={:.3}ms",
        samples[10].as_secs_f64() * 1000.0,
        samples[18].as_secs_f64() * 1000.0
    );
}
