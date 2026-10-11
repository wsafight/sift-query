use std::sync::Arc;

use sift_query_core::{
    CustomAggregate, EntityLimits, EntityMeta, FieldId, FieldKind, FieldMeta, Operator,
    OperatorSet, OrderMeta, ParamParser, QueryPlan, QueryPolicy, QueryTarget, RawParams,
    ScalarKind, SearchEntity, SearchOptions, ServerScope, SortOrder, ValueType,
};
use sift_query_sqlx::{
    ExtensionError, PartitionName, PartitionResolver, PgRenderer, PgSearchError, PgSiftQuery,
    QueryContext, QueryShapeCache, RenderedPgQuery, ResultFilter, SqlContext, SqlInterceptor,
};
use sqlx::postgres::PgPoolOptions;

const OPS: &[Operator] = &[Operator::Eq, Operator::Gt];
static FIELDS: &[FieldMeta] = &[
    FieldMeta {
        id: FieldId(0),
        public_name: "id",
        column: "u.id",
        result_alias: Some("id"),
        value_type: ValueType::I64,
        kind: FieldKind::Scalar,
        filterable: true,
        selectable: true,
        sortable: true,
        operators: OperatorSet(OPS),
    },
    FieldMeta {
        id: FieldId(1),
        public_name: "age",
        column: "u.age",
        result_alias: Some("age"),
        value_type: ValueType::I32,
        kind: FieldKind::Scalar,
        filterable: true,
        selectable: true,
        sortable: false,
        operators: OperatorSet(OPS),
    },
];
static ORDER: &[OrderMeta] = &[OrderMeta {
    field: FieldId(0),
    direction: SortOrder::Asc,
}];
static META: EntityMeta = EntityMeta {
    entity_name: "User",
    from: "app_user u",
    base_where: None,
    fixed_having: None,
    distinct: false,
    fields: FIELDS,
    default_order: ORDER,
    stable_order: ORDER,
    limits: EntityLimits {
        max_page_size: 100,
        max_offset: 20_000,
        timeout_ms: 100,
    },
    schema_fingerprint: 77,
};

#[derive(Debug, sqlx::FromRow)]
#[allow(dead_code)]
struct User {
    id: i64,
    age: i32,
}

impl SearchEntity for User {
    const META: &'static EntityMeta = &META;
}

fn plan(raw: &str) -> QueryPlan<User> {
    let policy = QueryPolicy::default();
    let raw = RawParams::parse(raw).unwrap();
    let query = ParamParser::new(&policy)
        .unwrap()
        .parse::<User>(&raw)
        .unwrap();
    QueryPlan::new(&query, &ServerScope::unrestricted(), QueryTarget::TypedPage).unwrap()
}

fn plan_with_policy(raw: &str, policy: &QueryPolicy) -> QueryPlan<User> {
    let raw = RawParams::parse(raw).unwrap();
    let query = ParamParser::new(policy)
        .unwrap()
        .parse::<User>(&raw)
        .unwrap();
    QueryPlan::new(&query, &ServerScope::unrestricted(), QueryTarget::TypedPage).unwrap()
}

#[test]
fn shape_cache_is_bounded_and_reuses_the_sql_allocation() {
    let cache = QueryShapeCache::new(2).unwrap();
    let first = PgRenderer.render_list(&plan("id=1")).unwrap();
    let first_sql = Arc::clone(&first.sql);
    let (_, hit) = cache.intern(first);
    assert!(!hit);

    let same_shape = PgRenderer.render_list(&plan("id=9")).unwrap();
    let (same_shape, hit) = cache.intern(same_shape);
    assert!(hit);
    assert!(Arc::ptr_eq(&first_sql, &same_shape.sql));

    let _ = cache.intern(PgRenderer.render_list(&plan("id=1&id-op=gt")).unwrap());
    let _ = cache.intern(PgRenderer.render_list(&plan("age=20")).unwrap());
    let stats = cache.stats();
    assert_eq!(stats.entries, 2);
    assert_eq!(stats.hits, 1);
    assert_eq!(stats.evictions, 1);
}

#[test]
fn shape_cache_detects_hash_collisions_without_reusing_wrong_sql() {
    let cache = QueryShapeCache::new(1).unwrap();
    let first = PgRenderer.render_list(&plan("id=1")).unwrap();
    let collision = RenderedPgQuery {
        sql: Arc::from("SELECT 42"),
        binds: Vec::new(),
        shape_id: first.shape_id,
    };
    let _ = cache.intern(first);
    let (collision, hit) = cache.intern(collision);
    assert!(!hit);
    assert_eq!(collision.sql.as_ref(), "SELECT 42");
    assert_eq!(cache.stats().collisions, 1);
}

#[test]
fn shape_id_includes_the_query_policy_fingerprint() {
    let strict = QueryPolicy::default();
    let compatible = QueryPolicy::bean_searcher_4812();
    let strict_query = PgRenderer
        .render_list(&plan_with_policy("id=1", &strict))
        .unwrap();
    let compatible_query = PgRenderer
        .render_list(&plan_with_policy("id=1", &compatible))
        .unwrap();
    assert_eq!(strict_query.sql, compatible_query.sql);
    assert_ne!(strict_query.shape_id, compatible_query.shape_id);
}

#[test]
fn window_count_and_all_m2_aggregates_render_explicitly() {
    let plan = plan("size=10");
    let window = PgRenderer.render_window_list(&plan).unwrap();
    assert!(window.sql.contains("count(*) OVER() AS \"sq_total_count\""));

    let custom =
        CustomAggregate::new("spread", "max({field}) - min({field})", ValueType::I32).unwrap();
    let age = sift_query_core::FieldRef::<User, i32, ScalarKind>::new(FieldId(1));
    let options = SearchOptions::<User>::builder()
        .sum(age)
        .avg(age)
        .min(age)
        .max(age)
        .custom(age, custom)
        .build()
        .unwrap();
    let rendered = PgRenderer.render_cluster(&plan, Some(&options)).unwrap();
    for fragment in [
        "sum(u.age)",
        "avg(u.age)",
        "min(u.age)",
        "max(u.age)",
        "max(u.age) - min(u.age)",
    ] {
        assert!(rendered.sql.contains(fragment), "missing {fragment}");
    }
}

#[test]
fn partition_names_reject_sql_syntax() {
    assert!(PartitionName::new("users_2026").is_ok());
    assert!(PartitionName::qualified("archive", "users_2026").is_ok());
    assert!(PartitionName::new("users; DROP TABLE users").is_err());
    assert!(PartitionName::new("public.users").is_err());
}

struct FixedPartition;

impl PartitionResolver for FixedPartition {
    fn resolve(
        &self,
        _context: &QueryContext<'_>,
    ) -> Result<Option<PartitionName>, ExtensionError> {
        Ok(Some(
            PartitionName::qualified("archive", "users_2026")?.with_alias("u")?,
        ))
    }
}

struct RejectAfterInspect;

impl SqlInterceptor for RejectAfterInspect {
    fn intercept(
        &self,
        _context: &SqlContext,
        sql: &str,
    ) -> Result<Option<String>, ExtensionError> {
        assert!(sql.contains("FROM \"archive\".\"users_2026\" AS \"u\""));
        Err(ExtensionError::rejected("test", "observed rewritten SQL"))
    }
}

#[tokio::test]
async fn partition_resolution_runs_before_sql_interception_and_database_acquire() {
    let pool = PgPoolOptions::new()
        .connect_lazy("postgres://localhost:1/unused")
        .unwrap();
    let search = PgSiftQuery::new(pool)
        .with_partition_resolver(FixedPartition)
        .with_sql_interceptor(RejectAfterInspect);
    let raw = RawParams::parse("").unwrap();
    let error = search
        .search_list::<User>(&raw, &ServerScope::unrestricted())
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        PgSearchError::Extension(ExtensionError::Rejected {
            extension: "test",
            ..
        })
    ));
}

#[test]
fn custom_aggregate_rejects_uncontrolled_templates() {
    assert!(
        CustomAggregate::new("unsafe", "sum({field}); DELETE FROM users", ValueType::I64).is_err()
    );
    assert!(CustomAggregate::new("missing", "count(*)", ValueType::I64).is_err());
}

// ---------------------------------------------------------------------------
// Additional renderer-only coverage
// ---------------------------------------------------------------------------

#[test]
fn result_filters_can_mutate_rows_without_discarding_them() {
    use sift_query_sqlx::{ResultContext, ResultFilterDecision};

    struct Rename;
    impl ResultFilter for Rename {
        fn filter(
            &self,
            _context: &ResultContext,
            _row_index: usize,
            row: &mut sift_query_sqlx::RowMap,
        ) -> Result<ResultFilterDecision, ExtensionError> {
            row.insert("injected".to_owned(), sift_query_core::RowValue::Bool(true));
            Ok(ResultFilterDecision::Keep)
        }
    }
    // The filter is pure plumbing here; the live database test covers the
    // end-to-end path.
    let filter = Rename;
    let mut row = sift_query_sqlx::RowMap::new();
    let decision = filter
        .filter(
            &ResultContext {
                entity_name: "User",
                operation: "search_map",
                target: QueryTarget::MapPage,
            },
            0,
            &mut row,
        )
        .unwrap();
    assert_eq!(decision, ResultFilterDecision::Keep);
    assert_eq!(row["injected"], sift_query_core::RowValue::Bool(true));
}

#[test]
fn partition_resolvers_may_decline_to_rewrite() {
    struct Decline;
    impl PartitionResolver for Decline {
        fn resolve(
            &self,
            _context: &QueryContext<'_>,
        ) -> Result<Option<PartitionName>, ExtensionError> {
            Ok(None)
        }
    }
    let resolver = Decline;
    let context = QueryContext {
        entity: User::META,
        target: QueryTarget::TypedPage,
        operation: "search",
        server_predicate: &sift_query_core::PredicateArena::default(),
        server_group_predicate: &sift_query_core::PredicateArena::default(),
        row_predicate: &sift_query_core::PredicateArena::default(),
        group_predicate: &sift_query_core::PredicateArena::default(),
        paging: &sift_query_core::Paging::default(),
    };
    assert!(resolver.resolve(&context).unwrap().is_none());
    assert!(format!("{context:?}").contains("User"));
}

#[test]
fn partition_names_accept_dollar_suffixed_tables() {
    assert!(PartitionName::new("users_2026$01").is_ok());
    assert!(PartitionName::qualified("archive_2026", "users$1").is_ok());
}

#[test]
fn window_rendering_is_rejected_for_targets_it_cannot_serve() {
    let count_plan = {
        let raw = RawParams::parse("").unwrap();
        let policy = QueryPolicy::default();
        let query = ParamParser::new(&policy)
            .unwrap()
            .parse::<User>(&raw)
            .unwrap();
        QueryPlan::new(&query, &ServerScope::unrestricted(), QueryTarget::Count).unwrap()
    };
    assert_eq!(
        PgRenderer.render_window_list(&count_plan).unwrap_err(),
        sift_query_sqlx::RenderError::WrongTarget
    );
}
