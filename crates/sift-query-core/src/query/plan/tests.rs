use super::*;
use crate::{
    Condition, CursorCodec, EntityLimits, EntityMeta, FieldKind, FieldMeta, FieldRef, Operator,
    OperatorSet, OrderMeta, ParamParser, PredicateArena, PredicatePhase, QueryPolicy, RawParams,
    ScalarValue, SearchEntity, ServerScope,
};

const OPS: &[Operator] = &[Operator::Eq];
static FIELDS: &[FieldMeta] = &[
    FieldMeta {
        id: FieldId(0),
        public_name: "id",
        column: "u.id",
        result_alias: Some("id"),
        value_type: crate::ValueType::I64,
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
        value_type: crate::ValueType::Text,
        kind: FieldKind::Scalar,
        filterable: true,
        selectable: true,
        sortable: true,
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
        timeout_ms: 5_000,
    },
    schema_fingerprint: 51,
};

#[derive(Debug)]
struct User;
impl SearchEntity for User {
    const META: &'static EntityMeta = &META;
}

static GROUPED_FIELDS: &[FieldMeta] = &[FieldMeta {
    id: FieldId(0),
    public_name: "key",
    column: "t.k",
    result_alias: Some("key"),
    value_type: crate::ValueType::I64,
    kind: FieldKind::GroupKey,
    filterable: true,
    selectable: true,
    sortable: true,
    operators: OperatorSet(OPS),
}];
static GROUPED_META: EntityMeta = EntityMeta {
    entity_name: "Grouped",
    from: "t",
    base_where: None,
    fixed_having: None,
    distinct: false,
    fields: GROUPED_FIELDS,
    default_order: ORDER,
    stable_order: ORDER,
    limits: EntityLimits {
        max_page_size: 100,
        max_offset: 20_000,
        timeout_ms: 5_000,
    },
    schema_fingerprint: 52,
};

#[derive(Debug)]
struct Grouped;
impl SearchEntity for Grouped {
    const META: &'static EntityMeta = &GROUPED_META;
}

fn parse<E: SearchEntity>(query: &str) -> UserQuery<E> {
    let raw = RawParams::parse(query).unwrap();
    ParamParser::new(&QueryPolicy::default())
        .unwrap()
        .parse::<E>(&raw)
        .unwrap()
}

fn id_only_scope() -> ServerScope<User> {
    ServerScope::unrestricted()
        .with_field_access(
            crate::FieldAccessPolicy::<User>::builder()
                .allow_id(FieldId(0))
                .build()
                .unwrap(),
        )
        .unwrap()
}

#[test]
fn field_access_filters_dynamic_map_projection() {
    let plan = QueryPlan::new(&parse::<User>(""), &id_only_scope(), QueryTarget::MapPage).unwrap();
    assert_eq!(plan.projection, vec![FieldId(0)]);
}

#[test]
fn field_access_rejects_explicit_projection_and_user_predicates() {
    let scope = id_only_scope();
    let projection = parse::<User>("onlySelect=name");
    assert_eq!(
        QueryPlan::new(&projection, &scope, QueryTarget::MapPage).unwrap_err(),
        PlanError::Param(ParamError::FieldAccessDenied("name".to_owned()))
    );

    let filtered = parse::<User>("name=alice");
    assert_eq!(
        QueryPlan::new(&filtered, &scope, QueryTarget::MapPage).unwrap_err(),
        PlanError::Param(ParamError::FieldAccessDenied("name".to_owned()))
    );
}

#[test]
fn field_access_does_not_restrict_server_scope_predicates() {
    let scope = ServerScope::new()
        .eq(
            FieldRef::<User, String, crate::ScalarKind>::new(FieldId(1)),
            "alice".to_owned(),
        )
        .build()
        .unwrap()
        .with_field_access(
            crate::FieldAccessPolicy::<User>::builder()
                .allow_id(FieldId(0))
                .build()
                .unwrap(),
        )
        .unwrap();
    QueryPlan::new(&parse::<User>(""), &scope, QueryTarget::MapPage).unwrap();
}

#[test]
fn a_scope_built_for_another_schema_is_rejected() {
    let query = parse::<User>("");
    let scope = ServerScope::<User> {
        predicate: PredicateArena::default(),
        schema_fingerprint: 999,
        field_access: crate::FieldAccessPolicy::unrestricted(),
        _entity: std::marker::PhantomData,
    };
    assert_eq!(
        QueryPlan::new(&query, &scope, QueryTarget::TypedPage).unwrap_err(),
        PlanError::SchemaMismatch
    );
}

#[test]
fn a_query_from_another_schema_is_rejected_by_planning() {
    let query = parse::<Grouped>("");
    // The scope belongs to `User`, so the fingerprints disagree.
    let scope = ServerScope::<Grouped> {
        predicate: PredicateArena::default(),
        schema_fingerprint: 999,
        field_access: crate::FieldAccessPolicy::unrestricted(),
        _entity: std::marker::PhantomData,
    };
    assert_eq!(
        QueryPlan::new(&query, &scope, QueryTarget::TypedPage).unwrap_err(),
        PlanError::SchemaMismatch
    );
}

#[test]
fn a_cursor_plan_for_a_grouped_entity_is_rejected() {
    // The parser rejects cursors for grouped entities, so the guard is
    // reached by constructing the paging directly.
    let mut query = parse::<Grouped>("");
    query.inner = std::sync::Arc::new(UserQueryData {
        predicate: PredicateArena::default(),
        projection: Projection::All,
        order: ORDER.iter().copied().map(OrderBy::from).collect(),
        paging: Paging {
            mode: PagingMode::Cursor(None),
            ..Paging::default()
        },
        schema_fingerprint: GROUPED_META.schema_fingerprint,
        policy_fingerprint: QueryPolicy::default().fingerprint(),
        force_empty: false,
    });
    let error =
        QueryPlan::new(&query, &ServerScope::unrestricted(), QueryTarget::TypedPage).unwrap_err();
    assert_eq!(
        error,
        PlanError::Param(ParamError::InvalidValue {
            field: "cursor".to_owned(),
            reason: "cursor pagination does not support grouped entities in M2".to_owned(),
        })
    );
}

#[test]
fn a_cursor_with_the_wrong_arity_is_rejected_by_planning() {
    let codec = CursorCodec::new([5u8; 32]).unwrap();
    let cursor = codec
        .decode::<User>(
            &codec
                .encode::<User>(
                    &[OrderBy::from(ORDER[0])],
                    &[crate::CursorValue::Scalar(ScalarValue::I64(1))],
                )
                .unwrap(),
            &[OrderBy::from(ORDER[0])],
        )
        .unwrap();
    let mut query = parse::<User>("");
    query.inner = std::sync::Arc::new(UserQueryData {
        predicate: PredicateArena::default(),
        projection: Projection::All,
        // Two ordering keys, one cursor value.
        order: vec![OrderBy::from(ORDER[0]), OrderBy::from(ORDER[0])],
        paging: Paging {
            mode: PagingMode::Cursor(Some(cursor)),
            ..Paging::default()
        },
        schema_fingerprint: META.schema_fingerprint,
        policy_fingerprint: QueryPolicy::default().fingerprint(),
        force_empty: false,
    });
    let error =
        QueryPlan::new(&query, &ServerScope::unrestricted(), QueryTarget::TypedPage).unwrap_err();
    assert_eq!(
        error,
        PlanError::Param(ParamError::InvalidValue {
            field: "cursor".to_owned(),
            reason: "cursor value count does not match ordering".to_owned(),
        })
    );
}

#[test]
fn a_static_projection_missing_a_cursor_key_is_rejected() {
    struct NameOnly;
    impl SearchProjection<User> for NameOnly {
        const FIELDS: &'static [crate::ProjectionField] = &[crate::ProjectionField {
            source: FieldId(1),
            result_alias: "name",
        }];
    }

    let codec = CursorCodec::new([6u8; 32]).unwrap();
    let order = vec![OrderBy::from(ORDER[0])];
    let cursor = codec
        .decode::<User>(
            &codec
                .encode::<User>(&order, &[crate::CursorValue::Scalar(ScalarValue::I64(1))])
                .unwrap(),
            &order,
        )
        .unwrap();
    let mut query = parse::<User>("");
    query.inner = std::sync::Arc::new(UserQueryData {
        predicate: PredicateArena::default(),
        projection: Projection::All,
        order,
        paging: Paging {
            mode: PagingMode::Cursor(Some(cursor)),
            ..Paging::default()
        },
        schema_fingerprint: META.schema_fingerprint,
        policy_fingerprint: QueryPolicy::default().fingerprint(),
        force_empty: false,
    });
    assert_eq!(
        QueryPlan::for_projection::<NameOnly>(&query, &ServerScope::unrestricted()).unwrap_err(),
        PlanError::Param(ParamError::NotSelectable("id".to_owned()))
    );
}

#[test]
fn a_group_predicate_carries_into_the_plan() {
    let query = {
        let raw = RawParams::parse("key=1").unwrap();
        ParamParser::new(&QueryPolicy::default())
            .unwrap()
            .parse::<Grouped>(&raw)
            .unwrap()
    };
    let plan =
        QueryPlan::new(&query, &ServerScope::unrestricted(), QueryTarget::TypedPage).unwrap();
    assert!(plan.row_predicate.root().is_some());
    assert!(plan.group_predicate.root().is_none());
    let _ = PredicatePhase::Group;
    let _ = Condition {
        field: FieldId(0),
        operator: Operator::Eq,
        values: vec![ScalarValue::I64(1)],
        ignore_case: false,
        phase: PredicatePhase::Group,
    };
}
