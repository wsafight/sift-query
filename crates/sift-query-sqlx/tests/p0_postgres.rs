//! Product contracts verified with a disposable PostgreSQL `DATABASE_URL`.
extern crate sift_query_core as sift_query;
use futures_util::{StreamExt, TryStreamExt};
use sift_query::{
    EnumCodec, GroupScope, QueryPolicy, RawParams, RowValue, SearchOptions, ServerScope,
};
use sift_query_derive::SearchEntity;
use sift_query_sqlx::*;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Debug, sqlx::FromRow, SearchEntity)]
#[search_entity(
    from = "sift_query_p0.items i",
    default_order = "id:asc",
    stable_order = "id:asc"
)]
struct Item {
    #[search_field(column = "i.id", sortable)]
    id: i64,
    #[search_field(column = "i.status", operators = [Eq, InList])]
    status: i32,
    #[search_field(column = "i.name")]
    name: String,
    #[sqlx(skip)]
    #[search_ignore]
    status_label: Option<String>,
}
impl LabelTarget for Item {
    fn label_id(&self, source: &str) -> Option<String> {
        (source == "status").then(|| self.status.to_string())
    }
    fn set_label(&mut self, source: &str, label: Option<&str>) -> Result<(), ExtensionError> {
        if source != "status" {
            return Err(ExtensionError::rejected("item", "unknown label field"));
        }
        self.status_label = label.map(str::to_owned);
        Ok(())
    }
}

#[derive(Debug, sqlx::FromRow, SearchEntity)]
#[search_entity(
    from = "sift_query_p0.items i",
    fixed_having = "count(*) >= 1",
    default_order = "status:asc",
    stable_order = "status:asc"
)]
struct Grouped {
    #[search_field(column = "i.status", kind = GroupKey, sortable)]
    status: i32,
    #[search_field(column = "count(*)", kind = Aggregate, operators = [Eq, Ge, Le])]
    count: i64,
}

#[derive(Debug, sqlx::FromRow, SearchEntity)]
#[search_entity(
    from = "sift_query_p0.items i",
    distinct,
    default_order = "status:asc",
    stable_order = "status:asc"
)]
struct DistinctGrouped {
    #[search_field(column = "i.status", kind = GroupKey, sortable)]
    status: i32,
    #[search_field(column = "count(*)", kind = Aggregate, operators = [Eq, Ge, Le])]
    count: i64,
}

#[derive(Debug, sqlx::FromRow, SearchEntity)]
#[search_entity(
    from = "sift_query_p0.items i",
    default_order = "id:asc",
    stable_order = "id:asc"
)]
struct NamedCodec {
    #[search_field(column = "i.id", sortable)]
    id: i64,
    #[search_field(column = "i.status", codec = "status", operators = [Eq, InList])]
    status: i32,
}

#[derive(Clone)]
struct Recorder(Arc<Mutex<Vec<QueryObservation>>>);
impl QueryObserver for Recorder {
    fn completed(&self, observation: &QueryObservation) {
        self.0.lock().unwrap().push(*observation);
    }
}

#[tokio::test]
#[ignore = "requires a disposable PostgreSQL instance in DATABASE_URL"]
#[allow(clippy::items_after_statements)]
async fn p0_contracts_against_postgres() -> Result<(), Box<dyn std::error::Error>> {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(4)
        .connect(&std::env::var("DATABASE_URL")?)
        .await?;
    sqlx::query("CREATE SCHEMA IF NOT EXISTS sift_query_p0")
        .execute(&pool)
        .await?;
    sqlx::query("DROP TABLE IF EXISTS sift_query_p0.items")
        .execute(&pool)
        .await?;
    sqlx::query("CREATE TABLE sift_query_p0.items(id bigint primary key, status integer not null, name text not null)").execute(&pool).await?;
    sqlx::query(
        "INSERT INTO sift_query_p0.items VALUES (1, 1, 'Alice'), (2, 1, 'Bob'), (3, 2, '=cmd')",
    )
    .execute(&pool)
    .await?;
    let recorded = Arc::new(Mutex::new(Vec::new()));
    let search = PgSiftQuery::with_policy(
        pool.clone(),
        QueryPolicy::default(),
        PgExecutionPolicy {
            shape_cache_capacity: 32,
            ..Default::default()
        },
    )?
    .with_scan_concurrency(1, 0)?
    .with_query_observer(Recorder(recorded.clone()))
    .with_slow_query_threshold(Duration::ZERO);
    let raw = RawParams::default();
    let scope = ServerScope::<Item>::unrestricted();

    // Runtime HAVING is ANDed outside user OR, and count/summaries see only
    // groups that survive that server-owned predicate.
    let group = GroupScope::new().ge(GroupedFields::COUNT, 2).build()?;
    let typed_group = search
        .search_with_group_scope::<Grouped>(&raw, &ServerScope::unrestricted(), &group)
        .await?;
    assert_eq!(typed_group.data[0].count, 2);
    let grouped = search
        .search_map_with_group_scope::<Grouped>(
            &RawParams::parse("a.count=1&b.count=2&gexpr=a|b")?,
            &ServerScope::unrestricted(),
            &group,
        )
        .await?;
    assert_eq!(grouped.total_count, 1);
    assert_eq!(grouped.data[0]["status"], RowValue::I32(1));
    let options = SearchOptions::builder().max(GroupedFields::COUNT).build()?;
    let summary = search
        .search_map_with_scopes::<Grouped>(
            &raw,
            &ServerScope::unrestricted(),
            &group,
            Some(&options),
        )
        .await?;
    assert_eq!(summary.summaries[0].value, Some(RowValue::I64(2)));
    let mut connection = pool.acquire().await?;
    let grouped = search
        .search_map_on_with::<Grouped>(
            &mut connection,
            &raw,
            &ServerScope::unrestricted(),
            &group,
            Some(&options),
        )
        .await?;
    assert_eq!(grouped.summaries[0].value, Some(RowValue::I64(2)));
    drop(connection);
    let group = GroupScope::new()
        .ge(DistinctGroupedFields::COUNT, 2)
        .build()?;
    let distinct = search
        .search_with_group_scope::<DistinctGrouped>(&raw, &ServerScope::unrestricted(), &group)
        .await?;
    assert_eq!(distinct.total_count, 1);
    assert_eq!(distinct.data[0].status, 1);
    assert_eq!(distinct.data[0].count, 2);

    // Typed opt-in and dynamic enrichment cover both separate and window counts.
    let labels = || {
        LabelEnricher::new().bind("status", "status").with_loader(
            DictionaryLabelLoader::default().with_labels(
                "status",
                [
                    ("1".to_owned(), "Enabled".to_owned()),
                    ("2".to_owned(), "Disabled".to_owned()),
                ],
            ),
        )
    };
    let typed = Box::pin(search.search_enriched::<Item, _>(&raw, &scope, &labels())).await?;
    assert_eq!(typed.data[0].status_label.as_deref(), Some("Enabled"));
    assert_eq!(typed.total_count, 3);
    for strategy in [CountStrategy::Separate, CountStrategy::WindowWhenPossible] {
        let enriched = PgSiftQuery::with_policy(
            pool.clone(),
            QueryPolicy::default(),
            PgExecutionPolicy {
                count_strategy: strategy,
                ..PgExecutionPolicy::default()
            },
        )?
        .with_page_enricher(labels())
        .search_map::<Item>(&raw, &scope)
        .await?;
        assert_eq!(
            enriched.data[0]["statusLabel"],
            RowValue::Text("Enabled".to_owned())
        );
    }

    // Codec result representation is explicit for maps; typed sqlx rows retain
    // their native database type. Missing codec registration fails pre-execution.
    assert!(search.parse::<NamedCodec>(&raw).is_err());
    let coded = PgSiftQuery::with_policy(
        pool.clone(),
        QueryPolicy::default().with_codec(EnumCodec::new(
            "status",
            "status",
            [("Enabled", 1), ("Disabled", 2)],
        )?),
        PgExecutionPolicy::default(),
    )?;
    let result = coded
        .search_map::<NamedCodec>(
            &RawParams::parse("status=Enabled")?,
            &ServerScope::unrestricted(),
        )
        .await?;
    assert_eq!(result.data.len(), 2);
    assert_eq!(
        result.data[0]["status"],
        RowValue::Text("Enabled".to_owned())
    );
    let typed = coded
        .search::<NamedCodec>(&raw, &ServerScope::unrestricted())
        .await?;
    assert_eq!(typed.data[0].status, 1);
    assert_eq!(typed.data[0].id, 1);

    // At the exact cap the scan succeeds; an extra row is a terminal error,
    // never a silent truncation. Early drop releases the dedicated scan permit.
    let policy = ScanPolicy {
        max_rows: 3,
        batch_size: 1,
        timeout: Duration::from_secs(5),
    };
    assert_eq!(
        search
            .scan_stream::<Item>(&raw, &scope, policy)?
            .try_collect::<Vec<_>>()
            .await?
            .len(),
        3
    );
    let mut scan = search.scan_map_stream::<Item>(
        &raw,
        &scope,
        ScanPolicy {
            max_rows: 2,
            ..policy
        },
    )?;
    assert!(scan.try_next().await?.is_some());
    assert!(scan.try_next().await?.is_some());
    assert!(matches!(
        scan.try_next().await,
        Err(PgSearchError::ScanLimitExceeded { max_rows: 2 })
    ));
    drop(scan);
    let mut scan = search.scan_stream::<Item>(&raw, &scope, policy)?;
    assert_eq!(scan.try_next().await?.unwrap().id, 1);
    let mut blocked = search.scan_stream::<Item>(&raw, &scope, policy)?;
    assert!(matches!(
        blocked.next().await,
        Some(Err(PgSearchError::ScanQueueFull))
    ));
    drop(blocked);
    drop(scan);
    assert_eq!(search.scan::<Item>(&raw, &scope, policy)?.await?.len(), 3);
    let mut connection = pool.acquire().await?;
    let timeout: String = sqlx::query_scalar("SHOW statement_timeout")
        .fetch_one(&mut *connection)
        .await?;
    assert_eq!(timeout, "0");
    drop(connection);

    // Streaming CSV writes each bounded batch and reports a partial failure.
    let csv = CsvExporter::new(CsvExportPolicy {
        max_rows: 3,
        batch_size: 1,
        timeout: Duration::from_secs(5),
    })
    .with_columns([CsvColumn::new("id", "ID"), CsvColumn::new("name", "Name")]);
    let mut bytes = Vec::new();
    assert_eq!(
        csv.export::<Item, _>(&search, &raw, &scope, &mut bytes)
            .await?,
        3
    );
    assert_eq!(
        String::from_utf8(bytes)?,
        "ID,Name\n1,Alice\n2,Bob\n3,'=cmd\n"
    );
    let csv = CsvExporter::new(CsvExportPolicy {
        max_rows: 2,
        batch_size: 1,
        timeout: Duration::from_secs(5),
    });
    let mut labeled_bytes = Vec::new();
    let labeled_csv = CsvExporter::new(CsvExportPolicy {
        max_rows: 3,
        batch_size: 2,
        timeout: Duration::from_secs(5),
    })
    .with_columns([
        CsvColumn::new("id", "ID"),
        CsvColumn::label("status", "Status"),
    ]);
    labeled_csv
        .export::<Item, _>(
            &search.clone().with_page_enricher(labels()),
            &raw,
            &scope,
            &mut labeled_bytes,
        )
        .await?;
    assert_eq!(
        String::from_utf8(labeled_bytes)?,
        "ID,Status\n1,Enabled\n2,Enabled\n3,Disabled\n"
    );
    let mut partial = Vec::new();
    assert!(matches!(
        csv.export::<Item, _>(&search, &raw, &scope, &mut partial)
            .await,
        Err(PgSearchError::ScanLimitExceeded { .. })
    ));
    assert_ne!(partial, [] as [u8; 0]);
    assert!(
        recorded
            .lock()
            .unwrap()
            .iter()
            .any(|event| event.operation == "csv_export" && event.outcome == "budget")
    );

    // External execution sees uncommitted writes but neither commits nor
    // rolls back the caller's transaction, and preserves session ownership.
    let mut transaction = pool.begin().await?;
    sqlx::query("SET LOCAL statement_timeout = '2s'")
        .execute(&mut *transaction)
        .await?;
    sqlx::query("INSERT INTO sift_query_p0.items VALUES (9, 1, 'Uncommitted')")
        .execute(&mut *transaction)
        .await?;
    let page = search
        .search_on::<Item>(&mut transaction, &RawParams::parse("id=9")?, &scope)
        .await?;
    assert_eq!(page.data[0].name, "Uncommitted");
    assert_eq!(
        search
            .count::<Item>(&RawParams::parse("id=9")?, &scope)
            .await?,
        0
    );
    let timeout: String = sqlx::query_scalar("SHOW statement_timeout")
        .fetch_one(&mut *transaction)
        .await?;
    assert_eq!(timeout, "2s");
    for strategy in [
        ExecutionStrategy::ParallelBestEffort,
        ExecutionStrategy::Snapshot(SnapshotIsolation::RepeatableRead),
    ] {
        let incompatible = PgSiftQuery::with_policy(
            pool.clone(),
            QueryPolicy::default(),
            PgExecutionPolicy {
                strategy,
                ..PgExecutionPolicy::default()
            },
        )?;
        assert!(matches!(
            incompatible
                .search_on::<Item>(&mut transaction, &raw, &scope)
                .await,
            Err(PgSearchError::ExternalExecutionStrategy)
        ));
    }
    transaction.rollback().await?;
    assert_eq!(
        search
            .count::<Item>(&RawParams::parse("id=9")?, &scope)
            .await?,
        0
    );

    // Database cancellation leaves the caller's transaction aborted. The
    // library reports the timeout but does not silently roll it back.
    struct SlowList;
    impl SqlInterceptor for SlowList {
        fn intercept(
            &self,
            context: &SqlContext,
            _: &str,
        ) -> Result<Option<String>, ExtensionError> {
            Ok((context.phase == "list").then(|| "SELECT 1::BIGINT AS id, 1::INT AS status, 'slow'::TEXT AS name FROM pg_sleep(0.2) WHERE $1::BIGINT > 0 AND $2::BIGINT = 0".to_owned()))
        }
    }
    let mut transaction = pool.begin().await?;
    sqlx::query("SET LOCAL statement_timeout = '5ms'")
        .execute(&mut *transaction)
        .await?;
    let timed = search
        .clone()
        .with_sql_interceptor(SlowList)
        .search_on::<Item>(&mut transaction, &raw, &scope)
        .await;
    assert!(matches!(
        timed,
        Err(PgSearchError::Timeout(TimeoutStage::ListQuery))
    ));
    assert!(
        sqlx::query("SELECT 1")
            .execute(&mut *transaction)
            .await
            .is_err()
    );
    transaction.rollback().await?;
    assert_eq!(search.count::<Item>(&raw, &scope).await?, 3);

    // Successful external execution also leaves commit ownership with the caller.
    let mut transaction = pool.begin().await?;
    sqlx::query("INSERT INTO sift_query_p0.items VALUES (10, 1, 'Committed by caller')")
        .execute(&mut *transaction)
        .await?;
    assert_eq!(
        search
            .search_map_on::<Item>(&mut transaction, &RawParams::parse("id=10")?, &scope)
            .await?
            .total_count,
        1
    );
    transaction.commit().await?;
    assert_eq!(
        search
            .count::<Item>(&RawParams::parse("id=10")?, &scope)
            .await?,
        1
    );
    pool.close().await;
    Ok(())
}
