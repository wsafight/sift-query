//! Real PostgreSQL coverage for the Bean Searcher product alignment.
#![cfg(feature = "chrono")]
#![allow(dead_code)]
extern crate sift_query_core as sift_query;
use sift_query::{
    BucketRequest, CustomOperator, DateCodec, DateTimeCodec, EntityMeta, EnumCodec, FacetRequest,
    ListResultCodec, NumericBucketRequest, QueryBuilder, QueryPlan, QueryPolicy, QueryTarget,
    RawParams, RowValue, ScalarValue, ServerScope, TimeBucketRequest, TimeBucketUnit, TimeCodec,
    TimestampCodec, ValueType,
};
use sift_query_derive::{SearchEntity, search_entities};
use sift_query_sqlx::{web::SearchParams, *};
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use std::sync::{Arc, Mutex};
use tracing::instrument::WithSubscriber;

search_entities! {
    fields Identity { #[search_field(sortable)] id: i64, }
    fields Business {
        include Identity;
        #[search_field(sortable, codec = "status", custom_operators = ["above"])] status: i32,
        name: String,
        #[search_field(codec = "tags")] tags: Option<String>,
        #[search_field(codec = "zoned")] instant: chrono::DateTime<chrono::Utc>,
        #[search_field(codec = "clock")] clock: chrono::NaiveTime,
        #[search_field(codec = "local")] local_at: chrono::NaiveDateTime,
        #[search_field(codec = "day")] day: chrono::NaiveDate,
    }
    mapping Items {
        #[search_entity(from = "alignment_items i", column_prefix = "i.", stable_order = "id:asc")]
    }
    #[derive(Debug, SearchEntity, sqlx::FromRow)]
    #[search_mapping(Items)]
    struct Item { include Business; }
    #[derive(Debug, SearchEntity, sqlx::FromRow)]
    #[search_mapping(Items)]
    struct ItemDto {
        include Identity;
        #[search_field(column = "upper(i.name)")] display: String
    }
}

fn policy(zone: &str) -> QueryPolicy {
    QueryPolicy::default()
        .with_codec(EnumCodec::new("status", "status", [("enabled", 1), ("disabled", 2)]).unwrap())
        .with_codec(ListResultCodec::new("tags", "tags", "|", ValueType::I64, 4).unwrap())
        .with_codec(TimestampCodec::new("zoned", "instant", "%Y/%m/%d %H:%M:%S", zone).unwrap())
        .with_codec(TimeCodec {
            name: "clock",
            field: "clock",
            format: "%H:%M",
        })
        .with_codec(DateTimeCodec {
            name: "local",
            field: "local_at",
            format: "%Y/%m/%d %H:%M",
        })
        .with_codec(DateCodec {
            name: "day",
            field: "day",
            format: "%Y/%m/%d",
        })
        .with_custom_operator(
            CustomOperator::new("above", "{field} > {0}", 1).unwrap(),
            ValueType::I32,
        )
        .unwrap()
}

struct TrustedContext {
    region: &'static str,
    private_tenant: &'static str,
}
struct RegionSelector;
impl PgPoolSelector<TrustedContext> for RegionSelector {
    fn select_pool(
        &self,
        _: &'static EntityMeta,
        context: &TrustedContext,
    ) -> Result<String, PoolRouteError> {
        match context.region {
            "east" => Ok("primary".into()),
            "west" => Ok("archive".into()),
            "missing" => Ok("unregistered".into()),
            _ => Err(PoolRouteError::Rejected("invalid region")),
        }
    }
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
#[ignore = "requires a disposable PostgreSQL instance in DATABASE_URL"]
async fn rpc_codecs_mappings_and_trusted_routing_against_postgres()
-> Result<(), Box<dyn std::error::Error>> {
    let url = std::env::var("DATABASE_URL")?;
    let admin = PgPoolOptions::new()
        .max_connections(2)
        .connect(&url)
        .await?;
    for (schema, label) in [("alignment_primary", "East"), ("alignment_archive", "West")] {
        // Identifiers are fixed fixture names, never request data.
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "CREATE SCHEMA IF NOT EXISTS {schema}"
        )))
        .execute(&admin)
        .await?;
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "DROP TABLE IF EXISTS {schema}.alignment_items"
        )))
        .execute(&admin)
        .await?;
        sqlx::query(sqlx::AssertSqlSafe(format!("CREATE TABLE {schema}.alignment_items(id bigint primary key, status integer not null, name text not null, tags text, instant timestamptz not null, clock time not null, local_at timestamp not null, day date not null)"))).execute(&admin).await?;
        sqlx::query(sqlx::AssertSqlSafe(format!("INSERT INTO {schema}.alignment_items SELECT id, CASE WHEN id=1 THEN 1 ELSE 2 END, $1, CASE WHEN id=1 THEN NULL WHEN id=2 THEN '1|9007199254740993' ELSE '' END, '2026-10-10T00:00:00Z'::timestamptz, '01:02:03.123456'::time, '2026-10-10T01:02:03.123456'::timestamp, '2026-10-10'::date FROM generate_series(1,3) id"))).bind(label).execute(&admin).await?;
    }
    let options: PgConnectOptions = url.parse()?;
    let primary_pool = PgPoolOptions::new()
        .max_connections(4)
        .connect_with(
            options
                .clone()
                .options([("search_path", "alignment_primary")]),
        )
        .await?;
    let archive_pool = PgPoolOptions::new()
        .max_connections(4)
        .connect_with(options.options([("search_path", "alignment_archive")]))
        .await?;
    let primary_policy = policy("Asia/Shanghai");
    let execution = PgExecutionPolicy {
        shape_cache_capacity: 32,
        ..Default::default()
    };
    let router = PgPoolRouter::default()
        .register_named(
            "primary",
            PgSiftQuery::with_policy(primary_pool.clone(), primary_policy.clone(), execution)?,
        )?
        .register_named(
            "archive",
            PgSiftQuery::with_policy(archive_pool.clone(), policy("UTC"), execution)?,
        )?
        .with_default_named("primary")?
        .route_entity::<ItemDto>("archive")?;
    let east = TrustedContext {
        region: "east",
        private_tenant: "private-tenant-secret",
    };
    let west = TrustedContext {
        region: "west",
        private_tenant: "private-tenant-secret",
    };
    let primary = router.searcher_with_context::<Item, _>(&east, &RegionSelector)?;
    let raw = RawParams::default();
    let scope = ServerScope::<Item>::unrestricted();
    let logs = Arc::new(Mutex::new(Vec::new()));
    let captured = logs.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .without_time()
        .with_ansi(false)
        .with_writer(move || LogWriter(captured.clone()))
        .finish();
    Box::pin(
        async {
            let (east_result, west_result) = tokio::join!(
                router.search_map_with_context::<Item, _>(&raw, &scope, &east, &RegionSelector),
                router.search_map_with_context::<Item, _>(&raw, &scope, &west, &RegionSelector)
            );
            let east_result = east_result?;
            let west_result = west_result?;
            assert_eq!(east_result.total_count, 3);
            assert_eq!(west_result.total_count, 3);
            assert_eq!(east_result.data[1]["name"], RowValue::Text("East".into()));
            assert_eq!(west_result.data[1]["name"], RowValue::Text("West".into()));
            assert_eq!(
                east_result.data[1]["instant"],
                RowValue::Text("2026/10/10 08:00:00".into())
            );
            assert_eq!(
                west_result.data[1]["instant"],
                RowValue::Text("2026/10/10 00:00:00".into())
            );
            assert_eq!(east_result.data[0]["tags"], RowValue::Null);
            assert_eq!(
                east_result.data[1]["tags"],
                RowValue::List(vec![RowValue::I64(1), RowValue::I64(9_007_199_254_740_993)])
            );
            assert_eq!(east_result.data[2]["tags"], RowValue::List(vec![]));
            assert_eq!(east_result.data[1]["clock"], RowValue::Text("01:02".into()));
            assert_eq!(
                east_result.data[1]["local_at"],
                RowValue::Text("2026/10/10 01:02".into())
            );
            assert_eq!(
                east_result.data[1]["day"],
                RowValue::Text("2026/10/10".into())
            );
            let facets = primary
                .facets(
                    &raw,
                    &scope,
                    &[FacetRequest::new(ItemFields::STATUS, 2).excluding_nulls()],
                )
                .await?;
            assert_eq!(facets[0].field, "status");
            assert_eq!(facets[0].buckets.len(), 2);
            assert_eq!(facets[0].other_count, 0);
            let filtered = RawParams::parse("status=enabled")?;
            let self_excluded = primary
                .facets(
                    &filtered,
                    &scope,
                    &[FacetRequest::new(ItemFields::STATUS, 2)
                        .excluding_nulls()
                        .excluding_self_filter()],
                )
                .await?;
            assert_eq!(self_excluded[0].buckets.len(), 2);

            let mut connection = primary_pool.acquire().await?;
            sqlx::query("BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY")
                .execute(&mut *connection)
                .await?;
            let consistent = primary
                .search_map_with_facets_on(
                    &mut connection,
                    &raw,
                    &scope,
                    &[FacetRequest::new(ItemFields::STATUS, 2).excluding_nulls()],
                )
                .await?;
            assert_eq!(consistent.facets[0].buckets.len(), 2);
            sqlx::query("ROLLBACK").execute(&mut *connection).await?;
            primary.validate_schema::<Item>().await?;
            let diagnostics = primary.diagnose(&raw, &scope).await?;
            assert_eq!(diagnostics.stages.len(), 2);
            assert_ne!(diagnostics.stages[0].explain_json, "");
            let buckets = primary
                .buckets(
                    &raw,
                    &scope,
                    &[
                        BucketRequest::Numeric(NumericBucketRequest::new(
                            ItemFields::STATUS,
                            [1_i32, 2_i32],
                        )?),
                        BucketRequest::Time(TimeBucketRequest::new(
                            ItemFields::INSTANT,
                            TimeBucketUnit::Day,
                            "UTC",
                        )?),
                    ],
                )
                .await?;
            assert_eq!(buckets.len(), 2);
            Ok::<_, Box<dyn std::error::Error>>(())
        }
        .with_subscriber(subscriber),
    )
    .await?;
    let log = String::from_utf8(logs.lock().unwrap().clone())?;
    assert!(log.contains("pool_route=primary"), "{log}");
    assert!(log.contains("pool_route=archive"), "{log}");
    assert!(!log.contains(east.private_tenant));
    assert!(matches!(
        router.searcher_with_context::<Item, _>(
            &TrustedContext {
                region: "missing",
                private_tenant: ""
            },
            &RegionSelector
        ),
        Err(PoolRouteError::MissingPool(_))
    ));
    assert!(matches!(
        router.searcher_with_context::<Item, _>(
            &TrustedContext {
                region: "denied",
                private_tenant: ""
            },
            &RegionSelector
        ),
        Err(PoolRouteError::Rejected(_))
    ));
    let archive = router.searcher_with_context::<Item, _>(&west, &RegionSelector)?;
    assert!(!Arc::ptr_eq(&primary, &archive));
    let archive_before = archive.shape_cache_stats().unwrap();
    let query = QueryBuilder::<Item>::new(&primary_policy)
        .custom(
            ItemFields::STATUS,
            primary_policy
                .registered_operator("above")
                .unwrap()
                .operator,
            [1],
        )
        .build()?;
    let rpc = query.to_rpc(&primary_policy)?;
    let scoped = ServerScope::new().le(ItemFields::ID, 2).build()?;
    let direct = primary.search_map(&query, &scoped).await?;
    assert_eq!(direct.total_count, 1);
    assert_eq!(direct.data[0]["id"], RowValue::I64(2));
    for method in ["GET", "POST"] {
        let encoded = rpc.to_query_string();
        let params = SearchParams::parse_request(
            method,
            (method == "GET").then_some(encoded.as_str()),
            Some("application/x-www-form-urlencoded"),
            encoded.as_bytes(),
            primary_policy.limits(),
        )?;
        let roundtrip = primary.search_map::<Item>(params.raw(), &scoped).await?;
        assert_eq!(direct.data, roundtrip.data);
        assert_eq!(direct.total_count, roundtrip.total_count);
    }
    assert!(primary.shape_cache_stats().unwrap().hits > 0);
    assert_eq!(archive.shape_cache_stats().unwrap(), archive_before);
    let typed = router
        .search_with_context::<Item, _>(&rpc, &scoped, &east, &RegionSelector)
        .await?;
    assert_eq!(typed.data[0].status, 2);
    assert_eq!(typed.data[0].tags.as_deref(), Some("1|9007199254740993"));
    let decoded = primary.parse::<Item>(&rpc)?;
    let plan = QueryPlan::new(&decoded, &scoped, QueryTarget::MapPage)?;
    let sql = PgRenderer.render_list(&plan)?;
    assert!(sql.sql.contains("i.status > $"));
    assert!(
        sql.binds
            .contains(&PgBindValue::Scalar(ScalarValue::I32(1)))
    );
    assert!(!sql.sql.contains("enabled"));
    let empty = primary
        .search_map::<Item>(&RawParams::parse("name=%27%20OR%20TRUE%20--")?, &scope)
        .await?;
    assert_eq!(empty.total_count, 0);
    assert!(
        primary
            .search_map::<Item>(&RawParams::parse("name=x&name-op=above")?, &scope)
            .await
            .is_err()
    );
    assert!(
        primary
            .search_map::<Item>(&RawParams::parse("dataSource=archive")?, &scope)
            .await
            .is_err()
    );
    let dto = router
        .search::<ItemDto>(&raw, &ServerScope::unrestricted())
        .await?;
    assert_eq!(dto.data[0].display, "WEST");
    assert_eq!(primary.count::<Item>(&raw, &scope).await?, 3);
    let scanned = archive
        .scan_map::<Item>(&raw, &scope, ScanPolicy::default())?
        .await?;
    assert!(
        scanned
            .iter()
            .all(|row| row["name"] == RowValue::Text("West".into()))
    );
    let mut csv = Vec::new();
    CsvExporter::new(CsvExportPolicy::default())
        .with_columns([
            CsvColumn::new("tags", "Tags"),
            CsvColumn::new("instant", "Time"),
        ])
        .export::<Item, _>(&archive, &raw, &scope, &mut csv)
        .await?;
    let csv = String::from_utf8(csv)?;
    assert!(csv.contains("9007199254740993"));
    assert!(csv.contains("2026/10/10 00:00:00"));
    primary_pool.close().await;
    archive_pool.close().await;
    sqlx::query("DROP SCHEMA alignment_primary CASCADE")
        .execute(&admin)
        .await?;
    sqlx::query("DROP SCHEMA alignment_archive CASCADE")
        .execute(&admin)
        .await?;
    Ok(())
}
