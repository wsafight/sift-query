use super::shared::*;

// ---------------------------------------------------------------------------
// Grouped entity used by the protocol tests.
// ---------------------------------------------------------------------------

pub(super) static FIELDS: &[FieldMeta] = &[
    FieldMeta {
        id: FieldId(0),
        public_name: "id",
        column: "u.id",
        result_alias: Some("id"),
        value_type: ValueType::I64,
        kind: FieldKind::GroupKey,
        filterable: true,
        selectable: true,
        sortable: true,
        operators: OperatorSet(OPS_I64),
    },
    FieldMeta {
        id: FieldId(1),
        public_name: "name",
        column: "u.name",
        result_alias: Some("name"),
        value_type: ValueType::Text,
        kind: FieldKind::GroupKey,
        filterable: true,
        selectable: true,
        sortable: true,
        operators: OperatorSet(OPS_TEXT),
    },
    FieldMeta {
        id: FieldId(2),
        public_name: "score",
        column: "sum(u.score)",
        result_alias: Some("score"),
        value_type: ValueType::I32,
        kind: FieldKind::Aggregate,
        filterable: true,
        selectable: true,
        sortable: true,
        operators: OperatorSet(OPS_I32),
    },
];

pub(super) static META: EntityMeta = EntityMeta {
    entity_name: "TestEntity",
    from: "app_user u",
    base_where: None,
    fixed_having: None,
    distinct: false,
    fields: FIELDS,
    default_order: DEFAULT_ORDER,
    stable_order: DEFAULT_ORDER,
    limits: LIMITS,
    schema_fingerprint: 1,
};

#[derive(Debug)]
pub struct TestEntity;

impl SearchEntity for TestEntity {
    const META: &'static EntityMeta = &META;
}

pub fn parse(query: &str) -> Result<UserQuery<TestEntity>, ParamError> {
    let raw = RawParams::parse(query)?;
    ParamParser::new(&QueryPolicy::default())?.parse(&raw)
}

// ---------------------------------------------------------------------------
// Minimal scalar entity for cursor pagination.
// ---------------------------------------------------------------------------

pub(super) static CURSOR_FIELDS: &[FieldMeta] = &[FieldMeta {
    id: FieldId(0),
    public_name: "id",
    column: "u.id",
    result_alias: Some("id"),
    value_type: ValueType::I64,
    kind: FieldKind::Scalar,
    filterable: true,
    selectable: true,
    sortable: true,
    operators: OperatorSet(OPS_I64),
}];

pub(super) static CURSOR_META: EntityMeta = EntityMeta {
    entity_name: "CursorEntity",
    from: "app_user u",
    base_where: None,
    fixed_having: None,
    distinct: false,
    fields: CURSOR_FIELDS,
    default_order: DEFAULT_ORDER,
    stable_order: DEFAULT_ORDER,
    limits: LIMITS,
    schema_fingerprint: 2,
};

#[derive(Debug)]
pub struct CursorEntity;

impl SearchEntity for CursorEntity {
    const META: &'static EntityMeta = &CURSOR_META;
}

pub(super) static RICH_FIELDS: &[FieldMeta] = &[
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
        operators: OperatorSet(OPS_RICH_I64),
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
        operators: OperatorSet(OPS_RICH_TEXT),
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
        operators: OperatorSet(OPS_RICH_I32),
    },
    FieldMeta {
        id: FieldId(3),
        public_name: "active",
        column: "u.active",
        result_alias: Some("active"),
        value_type: ValueType::Bool,
        kind: FieldKind::Scalar,
        filterable: true,
        selectable: true,
        sortable: true,
        operators: OperatorSet(OPS_BOOL),
    },
    FieldMeta {
        id: FieldId(4),
        public_name: "score",
        column: "u.score",
        result_alias: Some("score"),
        value_type: ValueType::F64,
        kind: FieldKind::Scalar,
        filterable: true,
        selectable: true,
        sortable: true,
        operators: OperatorSet(OPS_F64),
    },
    FieldMeta {
        id: FieldId(5),
        public_name: "secret",
        column: "u.secret",
        result_alias: None,
        value_type: ValueType::Text,
        kind: FieldKind::Scalar,
        filterable: true,
        selectable: false,
        sortable: false,
        operators: OperatorSet(OPS_TEXT),
    },
    FieldMeta {
        id: FieldId(6),
        public_name: "computed",
        column: "u.computed",
        result_alias: Some("computed"),
        value_type: ValueType::I32,
        kind: FieldKind::Scalar,
        filterable: false,
        selectable: true,
        sortable: false,
        operators: OperatorSet(OPS_I32),
    },
    FieldMeta {
        id: FieldId(7),
        public_name: "tenant",
        column: "u.tenant_id",
        result_alias: None,
        value_type: ValueType::I64,
        kind: FieldKind::FilterOnly,
        filterable: true,
        selectable: false,
        sortable: false,
        operators: OperatorSet(OPS_RICH_I64),
    },
];

pub(super) static RICH_META: EntityMeta = EntityMeta {
    entity_name: "RichEntity",
    from: "app_user u",
    base_where: None,
    fixed_having: None,
    distinct: false,
    fields: RICH_FIELDS,
    default_order: DEFAULT_ORDER,
    stable_order: DEFAULT_ORDER,
    limits: LIMITS,
    schema_fingerprint: 3,
};

#[derive(Debug)]
pub struct RichEntity;

impl SearchEntity for RichEntity {
    const META: &'static EntityMeta = &RICH_META;
}

pub fn parse_rich(query: &str) -> Result<UserQuery<RichEntity>, ParamError> {
    let raw = RawParams::parse(query)?;
    ParamParser::new(&QueryPolicy::default())?.parse(&raw)
}

pub fn rich_plan(query: &str) -> UserQuery<RichEntity> {
    parse_rich(query).expect("query should parse")
}
