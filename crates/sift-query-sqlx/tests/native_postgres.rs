#![cfg(all(
    feature = "postgres-search",
    feature = "postgres-types",
    feature = "pg-trgm",
    feature = "postgis",
    feature = "ltree"
))]
#![allow(dead_code)]
extern crate sift_query_core as sift_query;
use sift_query_core::*;
use sift_query_derive::SearchEntity;
use sift_query_sqlx::{postgres::*, *};

#[derive(Debug, sqlx::FromRow, SearchEntity)]
#[search_entity(
    from = "sift_native.items i",
    default_order = "id:asc",
    stable_order = "id:asc"
)]
struct Item {
    #[search_field(column = "i.id", sortable, cursor_non_null)]
    id: i64,
    #[search_field(column="i.title", custom_operators=["trgm_similar"])]
    title: String,
    #[search_field(column="i.payload", operators=[IsNull, NotNull], custom_operators=["json_contains", "json_contained_by", "json_key", "json_any_keys", "json_all_keys"])]
    payload: Option<serde_json::Value>,
    #[search_field(column="i.tags", operators=[Eq, IsNull], custom_operators=["array_contains", "array_contained_by", "array_overlap"])]
    tags: Option<Vec<Option<i64>>>,
    #[search_field(column="i.span", operators=[Eq, IsNull], custom_operators=["range_contains", "range_element", "range_contained_by", "range_overlap", "range_left", "range_right", "range_adjacent"])]
    span: PgInt8Range,
    #[search_field(column="i.net", custom_operators=["net_contains", "net_contains_eq", "net_contained_by", "net_contained_by_eq", "net_overlap"])]
    net: ipnet::IpNet,
    #[sqlx(skip)]
    #[search_field(column="i.document", kind=FilterOnly, selectable=false, custom_operators=["fts_plain", "fts_phrase", "fts_web", "fts_query"])]
    document: FilterOnly<String>,
    #[sqlx(skip)]
    #[search_field(column="i.path", codec="ltree", kind=FilterOnly, selectable=false, custom_operators=["tree_ancestor", "tree_descendant"])]
    path: FilterOnly<String>,
    #[sqlx(skip)]
    #[search_field(column="i.geom", codec="ewkt", kind=FilterOnly, selectable=false, custom_operators=["geo_intersects", "geo_contains", "geo_within"])]
    geom: FilterOnly<String>,
    #[sqlx(skip)]
    #[search_field(column="i.payload ->> 'city'", kind=FilterOnly, selectable=false)]
    city: FilterOnly<String>,
    #[sqlx(skip)]
    #[search_field(column="i.title",kind=FilterOnly,selectable=false,filterable=false,sortable,sort_from="title",sort_operator="trgm_similar",sort_template="({field} <-> {0})")]
    distance: FilterOnly<f32>,
    #[search_field(column="ARRAY[i.title,NULL]::text[]",operators=[Eq,IsNull],custom_operators=["array_text_contains","array_text_contained_by","array_text_overlap"])]
    text_tags: Vec<Option<String>>,
    #[search_field(column="i.net::cidr",custom_operators=["net_contains","net_contains_eq","net_contained_by","net_contained_by_eq","net_overlap"])]
    subnet: ipnet::IpNet,
}

fn capabilities() -> PgCapabilities {
    let mut caps = PgCapabilities::default()
        .with_full_text(TextConfig::English)
        .require(Extension::Trgm)
        .require(Extension::Ltree)
        .require(Extension::Postgis);
    for mode in [
        JsonQuery::Contains,
        JsonQuery::ContainedBy,
        JsonQuery::Key,
        JsonQuery::AnyKeys,
        JsonQuery::AllKeys,
    ] {
        caps = caps.with_operator(jsonb(mode));
    }
    for mode in [
        ArrayQuery::Contains,
        ArrayQuery::ContainedBy,
        ArrayQuery::Overlap,
    ] {
        caps = caps.with_operator(array(ValueType::ArrayI64, mode).unwrap());
        caps = caps.with_operator(array(ValueType::ArrayText, mode).unwrap());
    }
    for mode in [
        RangeQuery::Contains,
        RangeQuery::ContainsElement,
        RangeQuery::ContainedBy,
        RangeQuery::Overlap,
        RangeQuery::Left,
        RangeQuery::Right,
        RangeQuery::Adjacent,
    ] {
        caps = caps.with_operator(range(mode));
    }
    for mode in [
        NetworkQuery::Contains,
        NetworkQuery::ContainsOrEqual,
        NetworkQuery::ContainedBy,
        NetworkQuery::ContainedByOrEqual,
        NetworkQuery::Overlap,
    ] {
        caps = caps.with_operator(network(mode));
    }
    caps.with_operator(trgm_similar())
        .with_operator(ltree(TreeQuery::Ancestor))
        .with_operator(ltree(TreeQuery::Descendant))
        .with_operator(spatial(SpatialQuery::Intersects))
        .with_operator(spatial(SpatialQuery::Contains))
        .with_operator(spatial(SpatialQuery::Within))
}

#[test]
fn native_types_preserve_rpc_and_cache_binds() {
    assert!(Item::cursor_key_non_null(ItemFields::ID.id()));
    let policy = capabilities()
        .apply(
            QueryPolicy::default()
                .with_codec(LtreeCodec::new("path"))
                .with_codec(EwktCodec::new("geom", 4326).unwrap()),
        )
        .unwrap();
    let parser = ParamParser::new(&policy).unwrap();
    for (field, operator, value) in [
        ("payload", "json_contains", r#"{"city":"上海"}"#),
        ("payload", "json_key", "city"),
        (
            "payload",
            "json_contains",
            "2424142244442112242414224444211224414122424",
        ),
        ("payload", "json_any_keys", r#"["city","nullable"]"#),
        ("tags", "array_contains", "[2,null]"),
        ("tags", "array_overlap", "[]"),
        ("span", "range_overlap", "(1,5]"),
        ("span", "range_element", "3"),
        ("net", "net_contains_eq", "::1/128"),
        ("document", "fts_web", "rust -java"),
    ] {
        let raw = RawParams::from_entries(
            [
                (field.to_owned(), value.to_owned()),
                (format!("{field}-op"), operator.to_owned()),
            ],
            policy.limits(),
        )
        .unwrap();
        let query = parser.parse::<Item>(&raw).unwrap();
        assert_eq!(
            parser
                .parse::<Item>(&query.to_rpc(&policy).unwrap())
                .unwrap()
                .predicate(),
            query.predicate()
        );
        let plan =
            QueryPlan::new(&query, &ServerScope::unrestricted(), QueryTarget::MapPage).unwrap();
        let cache = QueryShapeCache::new(2).unwrap();
        let rendered = PgRenderer.render_list(&plan).unwrap();
        let (cold, _) = cache.render_list(&plan).unwrap();
        let (warm, hit) = cache.render_list(&plan).unwrap();
        assert!(hit);
        assert_eq!(cold, rendered);
        assert_eq!(warm, rendered);
        assert!(rendered.sql.contains("$1"));
    }
    let large = format!(
        "[{}]",
        vec!["1"; policy.limits().max_values_per_condition + 1].join(",")
    );
    let raw = RawParams::from_entries(
        [
            ("tags".to_owned(), large),
            ("tags-op".to_owned(), "array_overlap".to_owned()),
        ],
        policy.limits(),
    )
    .unwrap();
    assert!(parser.parse::<Item>(&raw).is_err());
    assert!(
        QueryBuilder::<Item>::new(&policy)
            .custom_values(ItemFields::PAYLOAD, jsonb(JsonQuery::Key), [42_i64])
            .build()
            .is_err()
    );
    assert!(CursorValue::from_row_value(&RowValue::ArrayI64(vec![])).is_err());
    let scope = ServerScope::<Item>::new()
        .custom_values(
            ItemFields::PAYLOAD,
            jsonb(JsonQuery::Key),
            ["city".to_owned()],
        )
        .build()
        .unwrap();
    let query = parser.parse::<Item>(&RawParams::default()).unwrap();
    let codec = CursorCodec::new([0x33; 32]).unwrap();
    let token = codec
        .encode::<Item>(query.order(), &[CursorValue::Scalar(ScalarValue::I64(1))])
        .unwrap();
    let seek_policy = policy.clone().with_cursor_codec(codec);
    let seek = ParamParser::new(&seek_policy)
        .unwrap()
        .parse::<Item>(
            &RawParams::from_entries([("cursor".to_owned(), token)], seek_policy.limits()).unwrap(),
        )
        .unwrap();
    let seek_plan =
        QueryPlan::new(&seek, &ServerScope::unrestricted(), QueryTarget::MapPage).unwrap();
    let cache = QueryShapeCache::new(2).unwrap();
    for _ in 0..2 {
        let (rendered, _) = cache.render_list(&seek_plan).unwrap();
        assert!(rendered.sql.contains("i.id > $1"));
        assert!(!rendered.sql.contains("i.id IS NULL"));
        assert_eq!(rendered, PgRenderer.render_list(&seek_plan).unwrap());
    }
    let plan = QueryPlan::new(&query, &scope, QueryTarget::MapPage).unwrap();
    assert!(
        PgRenderer
            .render_list(&plan)
            .unwrap()
            .sql
            .contains("::text")
    );
    let invalid = Int8Range::Bounds {
        lower: Some(5),
        upper: Some(1),
    };
    assert!(
        ServerScope::<Item>::new()
            .custom_values(ItemFields::SPAN, range(RangeQuery::Overlap), [invalid])
            .build()
            .is_err()
    );
    assert!(
        QueryBuilder::<Item>::new(&policy)
            .custom_values(ItemFields::SPAN, range(RangeQuery::Overlap), [invalid])
            .build()
            .is_err()
    );
    for raw in [
        "orderBy=distance:asc",
        "a.title=Rust&a.title-op=trgm_similar&b.title=Java&b.title-op=trgm_similar&gexpr=a%7Cb&orderBy=distance:asc",
    ] {
        let rejected = parser
            .parse::<Item>(&RawParams::parse(raw).unwrap())
            .map_or(true, |query| {
                QueryPlan::new(&query, &ServerScope::unrestricted(), QueryTarget::MapPage).is_err()
            });
        assert!(rejected, "{raw}");
    }
    let bounded = capabilities()
        .apply(
            QueryPolicy::new(
                ParamNames::default(),
                QueryLimits {
                    max_bind_slots: 1,
                    ..QueryLimits::default()
                },
            )
            .unwrap()
            .with_codec(LtreeCodec::new("path"))
            .with_codec(EwktCodec::new("geom", 4326).unwrap()),
        )
        .unwrap();
    // One predicate bind fits; computed order consumes a second input bind.
    let bounded_parser = ParamParser::new(&bounded).unwrap();
    assert!(
        bounded_parser
            .parse::<Item>(&RawParams::parse("title=Rust&title-op=trgm_similar").unwrap())
            .is_ok()
    );
    assert!(
        bounded_parser
            .parse::<Item>(
                &RawParams::parse("title=Rust&title-op=trgm_similar&orderBy=distance:asc").unwrap()
            )
            .is_err()
    );
    let oversized_json = serde_json::json!(vec![1; policy.limits().max_values_per_condition]);
    assert!(
        QueryBuilder::<Item>::new(&policy)
            .custom_values(
                ItemFields::PAYLOAD,
                jsonb(JsonQuery::Contains),
                [oversized_json]
            )
            .build()
            .is_err()
    );
    assert_eq!("(1,5]".parse::<Int8Range>().unwrap().to_string(), "[2,6)");
    assert_eq!("[1,1)".parse::<Int8Range>().unwrap(), Int8Range::Empty);
    assert_eq!("(,)".parse::<Int8Range>().unwrap().to_string(), "(,)");
    assert!(
        "[9223372036854775807,9223372036854775807]"
            .parse::<Int8Range>()
            .is_err()
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL with pg_trgm, ltree and PostGIS available"]
async fn native_queries_against_postgres() -> Result<(), Box<dyn std::error::Error>> {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&std::env::var("DATABASE_URL")?)
        .await?;
    let caps = capabilities();
    let installed: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM pg_extension WHERE extname IN ('pg_trgm','ltree','postgis')",
    )
    .fetch_one(&pool)
    .await?;
    if installed < 3 {
        assert!(caps.check(&pool).await.is_err());
    }
    for sql in [
        "CREATE EXTENSION IF NOT EXISTS pg_trgm",
        "CREATE EXTENSION IF NOT EXISTS ltree",
        "CREATE EXTENSION IF NOT EXISTS postgis",
    ] {
        sqlx::query(sql).execute(&pool).await?;
    }
    caps.check(&pool).await?;
    for config in [
        TextConfig::Simple,
        TextConfig::English,
        TextConfig::German,
        TextConfig::French,
    ] {
        PgCapabilities::default()
            .with_full_text(config)
            .check(&pool)
            .await?;
    }
    sqlx::query("DROP SCHEMA IF EXISTS sift_native CASCADE")
        .execute(&pool)
        .await?;
    sqlx::raw_sql("CREATE SCHEMA sift_native; CREATE TABLE sift_native.items(id bigint PRIMARY KEY, title text, payload jsonb, tags bigint[], span int8range, net inet, document tsvector, path ltree, geom geometry(Point,4326));
    INSERT INTO sift_native.items VALUES
      (1,'Rust query','{\"city\":\"上海\",\"nullable\":null}',ARRAY[1,2,NULL]::bigint[],'[1,5)','10.0.0.0/24',to_tsvector('english','Rust query library'),'Top.Rust',ST_GeomFromText('POINT(1 2)',4326)),
      (2,'Java search','{}','{}','empty','10.0.1.0/24',to_tsvector('english','Java search library'),'Top.Java',ST_GeomFromText('POINT(9 9)',4326)),
      (3,'Other',NULL,NULL,'(,)','::1',to_tsvector('english','Other documentation'),'Other',NULL);
    CREATE INDEX ON sift_native.items USING gin(document); CREATE INDEX ON sift_native.items USING gin(payload); CREATE INDEX ON sift_native.items USING gin(tags); CREATE INDEX ON sift_native.items USING gist(span);").execute(&pool).await?;
    let policy = caps.apply(
        QueryPolicy::default()
            .with_codec(LtreeCodec::new("path"))
            .with_codec(EwktCodec::new("geom", 4326)?),
    )?;
    let search = PgSiftQuery::with_policy(
        pool.clone(),
        policy.clone(),
        PgExecutionPolicy {
            shape_cache_capacity: 16,
            ..PgExecutionPolicy::default()
        },
    )?;
    for (field, op, value, expected) in [
        ("document", "fts_plain", "rust", vec![1]),
        ("document", "fts_phrase", "rust query", vec![1]),
        ("document", "fts_web", "rust OR java", vec![1, 2]),
        ("document", "fts_query", "rust & query", vec![1]),
        ("document", "fts_plain", "", vec![]),
        ("payload", "json_contains", r#"{"nullable":null}"#, vec![1]),
        ("payload", "json_contains", "null", vec![]),
        ("payload", "json_key", "nullable", vec![1]),
        ("payload", "json_any_keys", r#"["city","missing"]"#, vec![1]),
        ("payload", "json_all_keys", "[]", vec![1, 2]),
        ("payload", "nl", "", vec![3]),
        ("tags", "array_contains", "[2]", vec![1]),
        ("tags", "array_contains", "[]", vec![1, 2]),
        ("tags", "array_overlap", "[]", vec![]),
        (
            "text_tags",
            "array_text_contains",
            r#"["Rust query"]"#,
            vec![1],
        ),
        ("subnet", "net_contains_eq", "10.0.0.1", vec![1]),
        ("span", "range_element", "3", vec![1, 3]),
        ("span", "range_contains", "[2,3)", vec![1, 3]),
        ("span", "range_contained_by", "(,)", vec![1, 2, 3]),
        ("span", "range_adjacent", "[5,7)", vec![1]),
        ("span", "eq", "empty", vec![2]),
        ("net", "net_contains_eq", "10.0.0.1", vec![1]),
        ("path", "tree_descendant", "Top", vec![1, 2]),
        ("path", "tree_ancestor", "Top.Rust.Lib", vec![1]),
        ("geom", "geo_intersects", "SRID=4326;POINT(1 2)", vec![1]),
        ("geom", "geo_contains", "SRID=4326;POINT(1 2)", vec![1]),
        ("geom", "geo_within", "SRID=4326;POINT(1 2)", vec![1]),
        ("title", "trgm_similar", "Rust query", vec![1]),
        ("city", "eq", "上海", vec![1]),
    ] {
        let mut entries = vec![(format!("{field}-op"), op.to_owned())];
        if op != "nl" {
            entries.push((field.to_owned(), value.to_owned()));
        }
        let raw = RawParams::from_entries(entries, policy.limits())?;
        let page = search
            .search::<Item>(&raw, &ServerScope::unrestricted())
            .await?;
        assert_eq!(
            page.data.iter().map(|item| item.id).collect::<Vec<_>>(),
            expected,
            "{field}/{op}"
        );
        assert_eq!(page.total_count, i64::try_from(expected.len())?);
    }
    let maps = search
        .search_map::<Item>(&RawParams::parse("")?, &ServerScope::unrestricted())
        .await?;
    let cursor_search = PgSiftQuery::with_policy(
        pool.clone(),
        policy
            .clone()
            .with_cursor_codec(CursorCodec::new([0x33; 32])?),
        PgExecutionPolicy::default(),
    )?;
    let first = cursor_search
        .search::<Item>(
            &RawParams::parse("cursor=&size=1")?,
            &ServerScope::unrestricted(),
        )
        .await?;
    assert_eq!(first.data[0].id, 1);
    let next = RawParams::from_entries(
        [
            ("cursor".to_owned(), first.next_cursor.unwrap()),
            ("size".to_owned(), "1".to_owned()),
        ],
        policy.limits(),
    )?;
    let second = cursor_search
        .search::<Item>(&next, &ServerScope::unrestricted())
        .await?;
    assert_eq!(second.data[0].id, 2);
    assert_eq!(second.total_count, 3);
    let ordered =
        RawParams::parse("title=Rust%20query&title-op=trgm_similar&orderBy=distance:asc")?;
    let page = search
        .search::<Item>(&ordered, &ServerScope::unrestricted())
        .await?;
    assert_eq!(page.data[0].id, 1);
    let second_order =
        RawParams::parse("title=Java%20search&title-op=trgm_similar&orderBy=distance:asc")?;
    assert_eq!(
        search
            .search::<Item>(&second_order, &ServerScope::unrestricted())
            .await?
            .data[0]
            .id,
        2
    );
    let parsed = sift_query_core::ParamParser::new(&policy)?.parse::<Item>(&ordered)?;
    let plan = QueryPlan::new(&parsed, &ServerScope::unrestricted(), QueryTarget::MapPage)?;
    let cache = QueryShapeCache::new(2)?;
    let (_, hit) = cache.render_list(&plan)?;
    assert!(!hit);
    let (cached, hit) = cache.render_list(&plan)?;
    assert!(hit);
    assert_eq!(cached, PgRenderer.render_list(&plan)?);
    let (window, _) = cache.render_window_list(&plan)?;
    let (warm, hit) = cache.render_window_list(&plan)?;
    assert!(hit);
    assert_eq!(window, PgRenderer.render_window_list(&plan)?);
    assert_eq!(warm, window);
    assert!(
        search
            .search::<Item>(
                &RawParams::parse("orderBy=distance:asc")?,
                &ServerScope::unrestricted()
            )
            .await
            .is_err()
    );
    assert_eq!(
        maps.data[0]["tags"],
        RowValue::ArrayI64(vec![Some(1), Some(2), None])
    );
    assert_eq!(maps.data[1]["span"], RowValue::Int8Range(Int8Range::Empty));
    assert_eq!(maps.data[2]["span"].to_external_text(), "(,)");
    assert!(
        search
            .search::<Item>(
                &RawParams::parse("document=invalid%26&document-op=fts_query")?,
                &ServerScope::unrestricted()
            )
            .await
            .is_err()
    );
    assert_eq!(
        sqlx::query_scalar::<_, i32>("SELECT 1")
            .fetch_one(&pool)
            .await?,
        1
    );
    pool.close().await;
    Ok(())
}
