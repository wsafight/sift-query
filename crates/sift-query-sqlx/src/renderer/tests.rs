use super::*;
use sift_query_core::{
    Condition, EntityLimits, EntityMeta, FieldMeta, Operator, OperatorSet, ParamParser,
    PredicateArena, QueryPolicy, RawParams, SearchEntity, ServerScope,
};

const OPS: &[Operator] = &[
    Operator::Eq,
    Operator::Between,
    Operator::Contain,
    Operator::OrLike,
];

static FIELDS: &[FieldMeta] = &[
    FieldMeta {
        id: FieldId(0),
        public_name: "id",
        column: "u.id",
        result_alias: Some("id"),
        value_type: sift_query_core::ValueType::I64,
        kind: FieldKind::Scalar,
        filterable: true,
        selectable: true,
        sortable: true,
        operators: OperatorSet(OPS),
    },
    FieldMeta {
        id: FieldId(1),
        public_name: "name",
        column: "u.name",
        result_alias: Some("name"),
        value_type: sift_query_core::ValueType::Text,
        kind: FieldKind::Scalar,
        filterable: true,
        selectable: true,
        sortable: true,
        operators: OperatorSet(OPS),
    },
];

static ORDER: &[sift_query_core::OrderMeta] = &[sift_query_core::OrderMeta {
    field: FieldId(0),
    direction: sift_query_core::SortOrder::Asc,
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
        timeout_ms: 5_000,
    },
    schema_fingerprint: 71,
};

struct User;
impl SearchEntity for User {
    const META: &'static EntityMeta = &META;
}

fn plan(query: &str) -> QueryPlan<User> {
    let raw = RawParams::parse(query).unwrap();
    let policy = QueryPolicy::default();
    let parsed = ParamParser::new(&policy)
        .unwrap()
        .parse::<User>(&raw)
        .unwrap();
    QueryPlan::new(
        &parsed,
        &ServerScope::unrestricted(),
        QueryTarget::TypedPage,
    )
    .unwrap()
}

#[test]
fn a_condition_without_a_value_is_reported() {
    // `eq` parsed normally always has one value; an empty value list is
    // rejected rather than rendered.
    let mut plan = plan("id=1");
    let condition = PredicateArena::from_condition(Condition {
        field: FieldId(0),
        operator: Operator::Eq,
        values: Vec::new(),
        ignore_case: false,
        phase: sift_query_core::PredicatePhase::Row,
    });
    plan.row_predicate = condition;
    let error = PgRenderer.render_list(&plan).unwrap_err();
    assert_eq!(
        error,
        RenderError::InvalidCondition("id needs one value".to_owned())
    );
}

#[test]
fn between_with_the_wrong_arity_is_reported() {
    for operator in [Operator::Between, Operator::NotBetween] {
        let mut plan = plan("id=1");
        plan.row_predicate = PredicateArena::from_condition(Condition {
            field: FieldId(0),
            operator,
            values: vec![ScalarValue::I64(1)],
            ignore_case: false,
            phase: sift_query_core::PredicatePhase::Row,
        });
        let error = PgRenderer.render_list(&plan).unwrap_err();
        assert_eq!(
            error,
            RenderError::InvalidCondition("id needs two values".to_owned())
        );
    }
}

#[test]
fn text_operators_reject_non_text_values() {
    for operator in [
        Operator::StartWith,
        Operator::EndWith,
        Operator::Contain,
        Operator::NotLike,
        Operator::OrLike,
    ] {
        let mut plan = plan("name=a&name-op=ct");
        plan.row_predicate = PredicateArena::from_condition(Condition {
            field: FieldId(1),
            operator,
            values: vec![ScalarValue::I64(1)],
            ignore_case: false,
            phase: sift_query_core::PredicatePhase::Row,
        });
        let error = PgRenderer.render_list(&plan).unwrap_err();
        assert!(
            matches!(error, RenderError::InvalidCondition(ref message)
                if message.contains("text")),
            "{operator:?}: {error:?}"
        );
    }
}

#[test]
fn an_unknown_operator_is_reported() {
    // `Empty` on a text field is a value-less operator the renderer does
    // support; a custom operator with an unbound placeholder is not.
    let operator = sift_query_core::CustomOperator::new("mod", "mod({field}, {0})", 1).unwrap();
    let mut plan = plan("id=1");
    plan.row_predicate = PredicateArena::from_condition(Condition {
        field: FieldId(0),
        operator: Operator::Custom(operator),
        // No values, so `{0}` stays unreplaced and the template is emitted
        // as-is rather than silently dropped.
        values: Vec::new(),
        ignore_case: false,
        phase: sift_query_core::PredicatePhase::Row,
    });
    let rendered = PgRenderer.render_list(&plan).unwrap();
    assert!(rendered.sql.contains("mod(u.id, {0})"));
}

#[test]
fn an_or_like_with_a_non_text_pattern_is_reported() {
    let mut plan = plan("name=a&name-op=ol");
    plan.row_predicate = PredicateArena::from_condition(Condition {
        field: FieldId(1),
        operator: Operator::OrLike,
        values: vec![ScalarValue::I64(1)],
        ignore_case: false,
        phase: sift_query_core::PredicatePhase::Row,
    });
    let error = PgRenderer.render_list(&plan).unwrap_err();
    assert!(
        matches!(error, RenderError::InvalidCondition(ref message)
            if message.contains("text patterns")),
        "{error:?}"
    );
}

#[test]
fn grouped_having_joins_multiple_aggregate_conditions() {
    // Two group-phase conditions render as one HAVING clause with a comma
    // separated GROUP BY.
    let plan = plan("id=1&id=2&id-op=bt");
    let rendered = PgRenderer.render_list(&plan).unwrap();
    assert!(rendered.sql.contains("u.id BETWEEN $1 AND $2"));
}

#[test]
fn group_by_lists_every_group_key_with_commas() {
    use sift_query_core::{FieldKind, ValueType};
    static GROUPED: &[FieldMeta] = &[
        FieldMeta {
            id: FieldId(0),
            public_name: "a",
            column: "t.a",
            result_alias: Some("a"),
            value_type: ValueType::I64,
            kind: FieldKind::GroupKey,
            filterable: true,
            selectable: true,
            sortable: true,
            operators: OperatorSet(OPS),
        },
        FieldMeta {
            id: FieldId(1),
            public_name: "b",
            column: "t.b",
            result_alias: Some("b"),
            value_type: ValueType::I64,
            kind: FieldKind::GroupKey,
            filterable: true,
            selectable: true,
            sortable: true,
            operators: OperatorSet(OPS),
        },
    ];
    static GROUPED_META: EntityMeta = EntityMeta {
        entity_name: "Grouped",
        from: "t",
        base_where: None,
        fixed_having: None,
        distinct: false,
        fields: GROUPED,
        default_order: ORDER,
        stable_order: ORDER,
        limits: EntityLimits {
            max_page_size: 100,
            max_offset: 20_000,
            timeout_ms: 5_000,
        },
        schema_fingerprint: 72,
    };
    struct Grouped;
    impl SearchEntity for Grouped {
        const META: &'static EntityMeta = &GROUPED_META;
    }
    let raw = RawParams::parse("").unwrap();
    let policy = QueryPolicy::default();
    let parsed = ParamParser::new(&policy)
        .unwrap()
        .parse::<Grouped>(&raw)
        .unwrap();
    let built = QueryPlan::new(
        &parsed,
        &ServerScope::unrestricted(),
        QueryTarget::TypedPage,
    )
    .unwrap();
    let rendered = PgRenderer.render_list(&built).unwrap();
    assert!(
        rendered.sql.contains("GROUP BY t.a, t.b"),
        "{}",
        rendered.sql
    );
}

#[test]
fn window_ordering_lists_every_key_with_commas() {
    let plan = plan("orderBy=id:asc,name:desc");
    let rendered = PgRenderer.render_window_list(&plan).unwrap();
    assert!(
        rendered
            .sql
            .contains("ORDER BY u.id ASC NULLS LAST, u.name DESC NULLS FIRST"),
        "{}",
        rendered.sql
    );
}
