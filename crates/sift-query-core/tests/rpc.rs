mod support;
use sift_query_core::*;
use support::*;

#[test]
fn canonical_rpc_preserves_nested_predicates_projection_order_and_paging() {
    let policy = QueryPolicy::default();
    let parser = ParamParser::new(&policy).unwrap();
    for input in [
        "id=1&id=2&id-op=il&size=3&page=2",
        "a.id=1&b.id=2&c.name=hello%20%2B&gexpr=(a|b)%26c&orderBy=name:desc&onlySelect=id,name&offset=4&size=5",
        "id-op=nl",
        "",
    ] {
        let query = parser
            .parse::<TestEntity>(&RawParams::parse(input).unwrap())
            .unwrap();
        let raw = query.to_rpc(&policy).unwrap();
        assert_eq!(
            raw.to_query_string(),
            query.to_rpc(&policy).unwrap().to_query_string()
        );
        let roundtrip = parser.parse::<TestEntity>(&raw).unwrap();
        assert_eq!(query.order(), roundtrip.order());
        assert_eq!(query.paging(), roundtrip.paging());
        assert_eq!(query.projection(), roundtrip.projection());
        let renamed = ParamNames {
            page: "pageNo".to_owned(),
            size: "pageSize".to_owned(),
            group_expr: "groups".to_owned(),
            ..Default::default()
        };
        let remote = policy.clone().with_param_names(renamed.clone()).unwrap();
        let raw = query.to_rpc_with_names(&policy, &renamed).unwrap();
        assert_eq!(
            ParamParser::new(&remote)
                .unwrap()
                .parse::<TestEntity>(&raw)
                .unwrap()
                .paging(),
            query.paging()
        );
    }
}

#[test]
fn builder_rpc_and_cursor_use_the_same_validated_query_contract() {
    let policy = QueryPolicy::default().with_cursor_codec(CursorCodec::new([42; 32]).unwrap());
    let field =
        sift_query_core::FieldRef::<TestEntity, i64, sift_query_core::ScalarKind>::new(FieldId(0));
    let raw = QueryBuilder::new(&policy)
        .eq(field, 42)
        .page(2, 3)
        .build_for_rpc()
        .unwrap();
    let query = ParamParser::new(&policy)
        .unwrap()
        .parse::<TestEntity>(&raw)
        .unwrap();
    assert_eq!(query.paging().offset, 3);
    let first = ParamParser::new(&policy)
        .unwrap()
        .parse::<CursorEntity>(&RawParams::parse("cursor=").unwrap())
        .unwrap();
    let token = policy
        .cursor_codec()
        .unwrap()
        .encode::<CursorEntity>(
            first.order(),
            &[CursorValue::Scalar(sift_query_core::ScalarValue::I64(7))],
        )
        .unwrap();
    let next = ParamParser::new(&policy)
        .unwrap()
        .parse::<CursorEntity>(&RawParams::parse(&format!("cursor={token}")).unwrap())
        .unwrap();
    assert_eq!(
        next.to_rpc(&policy)
            .unwrap()
            .entries()
            .iter()
            .find(|(key, _)| key == "cursor")
            .unwrap()
            .1,
        token
    );
    assert!(next.into_user_query(&QueryPolicy::default()).is_err());
}

#[test]
fn rpc_preserves_compatibility_empty_results_and_enforces_transport_budgets() {
    let compatible = QueryPolicy::bean_searcher_4812();
    let query = ParamParser::new(&compatible)
        .unwrap()
        .parse::<TestEntity>(&RawParams::parse("size=999&page=2&id=4").unwrap())
        .unwrap();
    assert!(query.is_forced_empty());
    let encoded = query.to_rpc(&compatible).unwrap();
    let decoded = ParamParser::new(&compatible)
        .unwrap()
        .parse::<TestEntity>(&encoded)
        .unwrap();
    assert!(decoded.is_forced_empty());
    assert_eq!(decoded.paging(), query.paging());
    let policy = QueryPolicy::new(
        ParamNames::default(),
        QueryLimits {
            max_raw_items: 1,
            ..Default::default()
        },
    )
    .unwrap();
    let field = FieldRef::<TestEntity, i64, ScalarKind>::new(FieldId(0));
    let query = QueryBuilder::new(&policy).eq(field, 1).build().unwrap();
    assert!(query.to_rpc(&policy).is_err());
}

#[derive(Debug)]
struct Increment;
impl RawParamFilter for Increment {
    fn filter(&self, _: &'static EntityMeta, input: &RawParams) -> Result<RawParams, ParamError> {
        RawParams::from_entries(
            input.entries().iter().map(|(key, value)| {
                (
                    key.clone(),
                    if key.rsplit('.').next() == Some("id") {
                        (value.parse::<i64>().unwrap() + 1).to_string()
                    } else {
                        value.clone()
                    },
                )
            }),
            &QueryLimits::default(),
        )
    }
}

#[test]
fn rpc_rejects_filters_that_change_canonical_query_values() {
    let policy = QueryPolicy::default().with_raw_param_filter(Increment);
    let field = FieldRef::<TestEntity, i64, ScalarKind>::new(FieldId(0));
    let query = QueryBuilder::new(&policy).eq(field, 1).build().unwrap();
    assert!(query.to_rpc(&policy).is_err());
}

#[test]
fn rpc_parameter_snapshot_keeps_order_and_repeated_values() {
    let policy = QueryPolicy::default();
    let query = ParamParser::new(&policy)
        .unwrap()
        .parse::<TestEntity>(&RawParams::parse("id-op=il&id=1&id=2&size=3&page=2").unwrap())
        .unwrap();
    assert_eq!(
        query.to_rpc(&policy).unwrap().to_query_string(),
        "q0.id-op=il&q0.id=1&q0.id=2&gexpr=q0&size=3&page=2"
    );
}

#[test]
fn rpc_does_not_add_group_depth_to_an_already_valid_query() {
    let policy = QueryPolicy::new(
        ParamNames::default(),
        QueryLimits {
            max_group_depth: 2,
            ..Default::default()
        },
    )
    .unwrap();
    let parser = ParamParser::new(&policy).unwrap();
    let query = parser
        .parse::<TestEntity>(&RawParams::parse("a.id=1&b.id=2&c.id=3&gexpr=(a|b)%26c").unwrap())
        .unwrap();
    let raw = query.to_rpc(&policy).unwrap();
    assert_eq!(
        raw.entries()
            .iter()
            .find(|(key, _)| key == "gexpr")
            .unwrap()
            .1,
        "(q0|q1)&q3"
    );
    assert!(parser.parse::<TestEntity>(&raw).is_ok());
}

fn assert_rpc_values<E: SearchEntity>(policy: &QueryPolicy, input: &str) {
    let parser = ParamParser::new(policy).unwrap();
    let query = parser
        .parse::<E>(&RawParams::parse(input).unwrap())
        .unwrap();
    let decoded = parser.parse::<E>(&query.to_rpc(policy).unwrap()).unwrap();
    let conditions = |query: &UserQuery<E>| {
        query
            .predicate()
            .nodes()
            .iter()
            .filter_map(|node| match node {
                PredicateNode::Condition(condition) => Some(condition.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(conditions(&query), conditions(&decoded));
}

#[test]
fn rpc_preserves_scalar_types_and_business_boolean_names() {
    let policy = QueryPolicy::default();
    assert_rpc_values::<BaseTyped>(
        &policy,
        "flag=true&count32=-42&count64=9007199254740993&ratio32=0.5&ratio64=1e-10&title=%E4%B8%AD%26%2B&blob=AQID",
    );
    let boolean = policy.clone().with_codec(BooleanCodec {
        name: "business",
        field: "flag",
        true_name: "enabled",
        false_name: "disabled",
    });
    assert_rpc_values::<BaseTyped>(&boolean, "flag=enabled");
    #[cfg(feature = "decimal")]
    assert_rpc_values::<DecimalTyped>(&policy, "amount=9007199254740993.123456789");
    #[cfg(feature = "uuid")]
    assert_rpc_values::<UuidTyped>(&policy, "external=550e8400-e29b-41d4-a716-446655440000");
}
