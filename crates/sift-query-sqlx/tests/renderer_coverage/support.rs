//! Renderer coverage for every SQL shape, including the shapes the PostgreSQL
//! execution tests do not reach: DISTINCT summaries, SUM over DISTINCT, custom
//! aggregates, ignore-case text matching, and multi-key cursors.

pub(super) use sift_query_core::{
    AggregateFn, CustomAggregate, CustomOperator, EntityLimits, EntityMeta, FieldId, FieldKind,
    FieldMeta, FieldRef, FilterExpr, Operator, OperatorSet, OrderMeta, ParamParser, QueryBuilder,
    QueryPlan, QueryPolicy, QueryTarget, RawParams, RowPhaseKind, ScalarKind, ScalarValue,
    SearchEntity, SearchOptions, ServerScope, SortOrder, ValueType,
};
pub(super) use sift_query_sqlx::{PgBindValue, PgRenderer, RenderError};

pub(super) const ID_OPS: &[Operator] = &[
    Operator::Eq,
    Operator::Ne,
    Operator::Gt,
    Operator::Ge,
    Operator::Lt,
    Operator::Le,
    Operator::Between,
    Operator::NotBetween,
    Operator::InList,
    Operator::NotIn,
    Operator::IsNull,
    Operator::NotNull,
    Operator::Empty,
    Operator::NotEmpty,
    Operator::AlwaysTrue,
    Operator::AlwaysFalse,
];
pub(super) const TEXT_OPS: &[Operator] = &[
    Operator::Eq,
    Operator::Ne,
    Operator::StartWith,
    Operator::EndWith,
    Operator::Contain,
    Operator::OrLike,
    Operator::NotLike,
    Operator::Empty,
    Operator::NotEmpty,
];

pub(super) static FIELDS: &[FieldMeta] = &[
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
        operators: OperatorSet(ID_OPS),
    },
    FieldMeta {
        id: FieldId(1),
        public_name: "name",
        column: "u.name",
        result_alias: Some("name"),
        value_type: ValueType::Text,
        kind: FieldKind::Scalar,
        filterable: true,
        selectable: true,
        sortable: true,
        operators: OperatorSet(TEXT_OPS),
    },
    FieldMeta {
        id: FieldId(2),
        public_name: "age",
        column: "u.age",
        result_alias: Some("age"),
        value_type: ValueType::I32,
        kind: FieldKind::Scalar,
        filterable: true,
        selectable: true,
        sortable: true,
        operators: OperatorSet(ID_OPS),
    },
];

pub(super) static ORDER: &[OrderMeta] = &[OrderMeta {
    field: FieldId(0),
    direction: SortOrder::Asc,
}];

pub(super) static META: EntityMeta = EntityMeta {
    entity_name: "User",
    from: "app_user u",
    base_where: Some("u.deleted = false"),
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
    schema_fingerprint: 31,
};

#[derive(Debug)]
pub(super) struct User;

impl SearchEntity for User {
    const META: &'static EntityMeta = &META;
}

pub(super) static DISTINCT_META: EntityMeta = EntityMeta {
    distinct: true,
    schema_fingerprint: 32,
    ..META
};

#[derive(Debug)]
pub(super) struct DistinctUser;

impl SearchEntity for DistinctUser {
    const META: &'static EntityMeta = &DISTINCT_META;
}

/// Grouped entity with a fixed HAVING clause, to cover that branch.
pub(super) static GROUPED_FIELDS: &[FieldMeta] = &[
    FieldMeta {
        id: FieldId(0),
        public_name: "courseId",
        column: "s.course_id",
        result_alias: Some("course_id"),
        value_type: ValueType::I64,
        kind: FieldKind::GroupKey,
        filterable: true,
        selectable: true,
        sortable: true,
        operators: OperatorSet(ID_OPS),
    },
    FieldMeta {
        id: FieldId(1),
        public_name: "total",
        column: "sum(s.score)",
        result_alias: Some("total"),
        value_type: ValueType::I64,
        kind: FieldKind::Aggregate,
        filterable: true,
        selectable: true,
        sortable: true,
        operators: OperatorSet(ID_OPS),
    },
];

pub(super) static GROUPED_ORDER: &[OrderMeta] = &[OrderMeta {
    field: FieldId(0),
    direction: SortOrder::Asc,
}];

pub(super) static GROUPED_META: EntityMeta = EntityMeta {
    entity_name: "Score",
    from: "student_score s",
    base_where: None,
    fixed_having: Some("count(*) > 0"),
    distinct: false,
    fields: GROUPED_FIELDS,
    default_order: GROUPED_ORDER,
    stable_order: GROUPED_ORDER,
    limits: EntityLimits {
        max_page_size: 100,
        max_offset: 20_000,
        timeout_ms: 5_000,
    },
    schema_fingerprint: 33,
};

#[derive(Debug)]
pub(super) struct Score;

impl SearchEntity for Score {
    const META: &'static EntityMeta = &GROUPED_META;
}

/// Entity whose fields cover the value types the array/aggregate guards inspect.
#[macro_export]
macro_rules! typed_field {
    ($id:literal, $name:literal, $value_type:expr) => {
        FieldMeta {
            id: FieldId($id),
            public_name: $name,
            column: "t.col",
            result_alias: Some($name),
            value_type: $value_type,
            kind: FieldKind::Scalar,
            filterable: true,
            selectable: true,
            sortable: true,
            operators: OperatorSet(ID_OPS),
        }
    };
}

pub(super) static TYPED_FIELDS: &[FieldMeta] = &[
    typed_field!(0, "id", ValueType::I64),
    typed_field!(1, "flag", ValueType::Bool),
    typed_field!(2, "count32", ValueType::I32),
    typed_field!(3, "ratio32", ValueType::F32),
    typed_field!(4, "ratio64", ValueType::F64),
    typed_field!(5, "title", ValueType::Text),
    typed_field!(6, "blob", ValueType::Bytes),
    #[cfg(feature = "decimal")]
    typed_field!(7, "amount", ValueType::Decimal),
    #[cfg(feature = "chrono")]
    typed_field!(8, "day", ValueType::Date),
    #[cfg(feature = "chrono")]
    typed_field!(9, "clock", ValueType::Time),
    #[cfg(feature = "chrono")]
    typed_field!(10, "localAt", ValueType::DateTime),
    #[cfg(feature = "chrono")]
    typed_field!(11, "instant", ValueType::Timestamp),
    #[cfg(feature = "uuid")]
    typed_field!(12, "external", ValueType::Uuid),
    #[cfg(feature = "json")]
    typed_field!(13, "payload", ValueType::Json),
];

pub(super) static TYPED_META: EntityMeta = EntityMeta {
    entity_name: "Typed",
    from: "app_typed t",
    base_where: None,
    fixed_having: None,
    distinct: false,
    fields: TYPED_FIELDS,
    default_order: ORDER,
    stable_order: ORDER,
    limits: EntityLimits {
        max_page_size: 100,
        max_offset: 20_000,
        timeout_ms: 5_000,
    },
    schema_fingerprint: 34,
};

#[derive(Debug)]
pub(super) struct Typed;

impl SearchEntity for Typed {
    const META: &'static EntityMeta = &TYPED_META;
}

/// Grouped entity whose stable order field is not selectable, which lets a
/// plan reach the renderer with an ordering the projection omits.
pub(super) fn plan<E: SearchEntity>(query: &str, target: QueryTarget) -> QueryPlan<E> {
    let raw = RawParams::parse(query).unwrap();
    let policy = QueryPolicy::default();
    let query = ParamParser::new(&policy).unwrap().parse::<E>(&raw).unwrap();
    QueryPlan::new(&query, &ServerScope::unrestricted(), target).unwrap()
}

pub(super) fn plan_with<E: SearchEntity>(
    query: &str,
    target: QueryTarget,
    policy: &QueryPolicy,
) -> QueryPlan<E> {
    let raw = RawParams::parse(query).unwrap();
    let query = ParamParser::new(policy).unwrap().parse::<E>(&raw).unwrap();
    QueryPlan::new(&query, &ServerScope::unrestricted(), target).unwrap()
}

// ---------------------------------------------------------------------------
