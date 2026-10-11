#[allow(dead_code, unused_imports)]
#[path = "renderer_coverage/support.rs"]
mod support;

use sift_query_sqlx::QueryShapeCache;
use std::sync::Arc;
use support::*;

fn make_plan<E: SearchEntity>(raw: &str) -> QueryPlan<E> {
    let policy = QueryPolicy::default();
    let query = ParamParser::new(&policy)
        .unwrap()
        .parse::<E>(&RawParams::parse(raw).unwrap())
        .unwrap();
    QueryPlan::new(&query, &ServerScope::unrestricted(), QueryTarget::MapPage).unwrap()
}

#[test]
fn hot_templates_rebuild_current_scalars_arrays_like_and_paging() {
    for (first, second) in [
        ("id=1", "id=9"),
        ("id=1&id-op=ne", "id=9&id-op=ne"),
        ("id=1&id=2&id-op=bt", "id=3&id=4&id-op=bt"),
        ("id=1&id=2&id-op=nb", "id=3&id=4&id-op=nb"),
        ("id=1&id-op=il", "id=3&id=4&id=5&id-op=il"),
        ("id=1&id-op=ni", "id=3&id=4&id-op=ni"),
        ("name=a%25&name-op=sw", "name=b_&name-op=sw"),
        ("name=a%25&name-op=ew", "name=b_&name-op=ew"),
        (
            "name=a%25&name-op=ct&name-ic=true",
            "name=b_&name-op=ct&name-ic=true",
        ),
        ("name=a&name=b&name-op=ol", "name=c&name=d&name-op=ol"),
        (
            "name=a&name-op=nk&name-ic=true",
            "name=b&name-op=nk&name-ic=true",
        ),
        ("id=1&size=2&page=1", "id=3&size=4&page=2"),
        ("id-op=nl", "id-op=nl"),
        ("id-op=at", "id-op=at"),
    ] {
        let cache = QueryShapeCache::new(4).unwrap();
        let first = make_plan::<User>(first);
        let second = make_plan::<User>(second);
        let (cold, hit) = cache.render_list(&first).unwrap();
        assert!(!hit);
        assert_eq!(cold, PgRenderer.render_list(&first).unwrap());
        let (hot, hit) = cache.render_list(&second).unwrap();
        assert!(hit);
        assert!(Arc::ptr_eq(&cold.sql, &hot.sql));
        assert_eq!(hot, PgRenderer.render_list(&second).unwrap());
    }
}

#[test]
fn topology_case_projection_order_and_operator_arity_do_not_alias() {
    let pairs = [
        ("a.id=1&b.age=2&gexpr=a|b", "a.id=1&b.age=2&gexpr=a%26b"),
        ("name=x&name-op=ct", "name=x&name-op=ct&name-ic=true"),
        ("name=x&name-op=ol", "name=x&name=y&name-op=ol"),
        ("onlySelect=id,name", "onlySelect=id,age"),
        ("sort=id&order=asc", "sort=id&order=desc"),
    ];
    for (first, second) in pairs {
        let cache = QueryShapeCache::new(2).unwrap();
        cache.render_list(&make_plan::<User>(first)).unwrap();
        let second = make_plan::<User>(second);
        let (rendered, hit) = cache.render_list(&second).unwrap();
        assert!(!hit, "{first} / {second:?}");
        assert_eq!(rendered, PgRenderer.render_list(&second).unwrap());
    }
}

fn check_all<E: SearchEntity>(
    plan: &QueryPlan<E>,
    options: Option<&SearchOptions<E>>,
    field: FieldId,
) {
    let cache = QueryShapeCache::new(8).unwrap();
    for _ in 0..2 {
        assert_eq!(
            cache.render_list(plan).unwrap().0,
            PgRenderer.render_list(plan).unwrap()
        );
        assert_eq!(
            cache.render_cluster(plan, options).unwrap().0,
            PgRenderer.render_cluster(plan, options).unwrap()
        );
        assert_eq!(
            cache.render_sum(plan, field).unwrap().0,
            PgRenderer.render_sum(plan, field).unwrap()
        );
    }
    assert_eq!(cache.stats().hits, 3);
}

#[test]
fn grouped_distinct_summaries_and_both_server_scopes_keep_their_semantics() {
    let query = ParamParser::new(&QueryPolicy::default())
        .unwrap()
        .parse::<Score>(&RawParams::parse("total=5&total-op=gt").unwrap())
        .unwrap();
    let row = ServerScope::new()
        .ge(
            FieldRef::<Score, i64, sift_query_core::GroupKeyKind>::new(FieldId(0)),
            10,
        )
        .build()
        .unwrap();
    let group = sift_query_core::GroupScope::new()
        .ge(
            FieldRef::<Score, i64, sift_query_core::AggregateKind>::new(FieldId(1)),
            2,
        )
        .build()
        .unwrap();
    let plan = QueryPlan::new_with_scopes(&query, &row, &group, QueryTarget::MapPage).unwrap();
    let total = FieldRef::<Score, i64, sift_query_core::AggregateKind>::new(FieldId(1));
    let options = SearchOptions::builder()
        .sum(total)
        .avg(total)
        .max(total)
        .build()
        .unwrap();
    check_all(&plan, Some(&options), FieldId(1));
    let distinct = make_plan::<DistinctUser>("id=1&name=x&name-op=ct");
    let id = FieldRef::<DistinctUser, i64, ScalarKind>::new(FieldId(0));
    check_all(
        &distinct,
        Some(&SearchOptions::builder().sum(id).build().unwrap()),
        FieldId(0),
    );
    let cache = QueryShapeCache::new(2).unwrap();
    let plan = make_plan::<User>("id=1&size=3");
    assert!(!cache.render_window_list(&plan).unwrap().1);
    let plan = make_plan::<User>("id=8&size=4");
    let (hot, hit) = cache.render_window_list(&plan).unwrap();
    assert!(hit);
    assert_eq!(hot, PgRenderer.render_window_list(&plan).unwrap());
}

#[test]
fn templates_are_bounded_and_thread_safe() {
    let cache = Arc::new(QueryShapeCache::new(2).unwrap());
    cache.render_list(&make_plan::<User>("id=1")).unwrap();
    std::thread::scope(|threads| {
        for value in 0..8 {
            let cache = Arc::clone(&cache);
            threads.spawn(move || {
                let plan = make_plan::<User>(&format!("id={value}"));
                assert_eq!(
                    cache.render_list(&plan).unwrap().0,
                    PgRenderer.render_list(&plan).unwrap()
                );
            });
        }
    });
    assert_eq!(cache.stats().hits, 8);
    cache.render_list(&make_plan::<User>("age=2")).unwrap();
    cache.render_list(&make_plan::<User>("name=x")).unwrap();
    assert_eq!(cache.stats().entries, 2);
    assert_eq!(cache.stats().evictions, 1);
    cache.clear();
    assert_eq!(cache.stats().entries, 0);
}

#[test]
fn cursor_null_shapes_and_repeated_keyset_binds_match_uncached_rendering() {
    use sift_query_core::{CursorCodec, CursorValue};
    let codec = CursorCodec::new([42; 32]).unwrap();
    let policy = QueryPolicy::default().with_cursor_codec(codec.clone());
    let parser = ParamParser::new(&policy).unwrap();
    let base = parser
        .parse::<User>(&RawParams::parse("sort=name&order=asc&cursor=").unwrap())
        .unwrap();
    let cache = QueryShapeCache::new(8).unwrap();
    for values in [
        vec![
            CursorValue::Scalar(ScalarValue::Text("A".to_owned())),
            CursorValue::Scalar(ScalarValue::I64(1)),
        ],
        vec![
            CursorValue::Scalar(ScalarValue::Text("B".to_owned())),
            CursorValue::Scalar(ScalarValue::I64(2)),
        ],
        vec![CursorValue::Null, CursorValue::Scalar(ScalarValue::I64(2))],
        vec![
            CursorValue::Scalar(ScalarValue::Text("B".to_owned())),
            CursorValue::Null,
        ],
        vec![CursorValue::Null, CursorValue::Null],
    ] {
        let token = codec.encode::<User>(base.order(), &values).unwrap();
        let query = parser
            .parse::<User>(
                &RawParams::parse(&format!("sort=name&order=asc&cursor={token}")).unwrap(),
            )
            .unwrap();
        let plan =
            QueryPlan::new(&query, &ServerScope::unrestricted(), QueryTarget::MapPage).unwrap();
        for _ in 0..2 {
            assert_eq!(
                cache.render_list(&plan).unwrap().0,
                PgRenderer.render_list(&plan).unwrap()
            );
        }
    }
    assert_eq!(cache.stats().entries, 4);
}

#[test]
fn current_scope_values_and_custom_templates_are_never_confused() {
    let cache = QueryShapeCache::new(4).unwrap();
    let policy = QueryPolicy::default();
    let query = QueryBuilder::<User>::new(&policy)
        .eq(FieldRef::<User, i64, ScalarKind>::new(FieldId(0)), 2)
        .build()
        .unwrap();
    for value in [7, 8] {
        let scope = ServerScope::new()
            .eq(FieldRef::<User, i64, ScalarKind>::new(FieldId(0)), value)
            .build()
            .unwrap();
        let plan = QueryPlan::new(&query, &scope, QueryTarget::TypedPage).unwrap();
        assert_eq!(
            cache.render_list(&plan).unwrap().0,
            PgRenderer.render_list(&plan).unwrap()
        );
    }
    let field = FieldRef::<User, i64, ScalarKind>::new(FieldId(0));
    for template in ["{field} > {0}", "{field} < {0}"] {
        let operator = CustomOperator::new("compare", template, 1).unwrap();
        let filter = FilterExpr::custom(field, operator, [3_i64]).unwrap();
        let query = QueryBuilder::<User>::new(&policy)
            .filter(filter)
            .build()
            .unwrap();
        let plan =
            QueryPlan::new(&query, &ServerScope::unrestricted(), QueryTarget::TypedPage).unwrap();
        assert!(!cache.render_list(&plan).unwrap().1);
        assert_eq!(
            cache.render_list(&plan).unwrap().0,
            PgRenderer.render_list(&plan).unwrap()
        );
    }
}

#[test]
fn warm_templates_preserve_renderer_validation_errors() {
    use sift_query_core::{Condition, PredicateArena, PredicatePhase};
    let cache = QueryShapeCache::new(4).unwrap();
    let mut plan = make_plan::<User>("name=x&name-op=nk");
    cache.render_list(&plan).unwrap();
    plan.row_predicate = PredicateArena::from_condition(Condition {
        field: FieldId(1),
        operator: Operator::NotLike,
        values: vec![ScalarValue::I64(1)],
        ignore_case: false,
        phase: PredicatePhase::Row,
    });
    assert!(PgRenderer.render_list(&plan).is_err());
    assert!(cache.render_list(&plan).is_err());
    let mut plan = make_plan::<User>("size=3");
    cache.render_window_list(&plan).unwrap();
    plan.paging.size = 0;
    assert_eq!(
        cache.render_window_list(&plan).unwrap_err(),
        RenderError::WrongTarget
    );
}

#[test]
fn warm_in_list_templates_recheck_element_types() {
    use sift_query_core::{Condition, PredicateArena, PredicatePhase};
    let cache = QueryShapeCache::new(4).unwrap();
    let mut plan = make_plan::<User>("id=1&id=2&id-op=il");
    cache.render_list(&plan).unwrap();
    plan.row_predicate = PredicateArena::from_condition(Condition {
        field: FieldId(0),
        operator: Operator::InList,
        values: vec![ScalarValue::Text("wrong-type".to_owned())],
        ignore_case: false,
        phase: PredicatePhase::Row,
    });
    assert!(PgRenderer.render_list(&plan).is_err());
    assert!(cache.render_list(&plan).is_err());
}
