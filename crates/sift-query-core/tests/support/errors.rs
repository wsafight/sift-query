use super::shared::*;

// ---------------------------------------------------------------------------
// Entities used for configuration-error paths.
// ---------------------------------------------------------------------------

/// Entity whose `stable_order` is empty, which must reject pagination.
pub(super) static UNSTABLE_FIELDS: &[FieldMeta] = &[FieldMeta {
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

pub(super) static UNSTABLE_META: EntityMeta = EntityMeta {
    entity_name: "UnstableEntity",
    from: "app_user u",
    base_where: None,
    fixed_having: None,
    distinct: false,
    fields: UNSTABLE_FIELDS,
    default_order: &[],
    stable_order: &[],
    limits: LIMITS,
    schema_fingerprint: 21,
};

#[derive(Debug)]
pub struct UnstableEntity;

impl SearchEntity for UnstableEntity {
    const META: &'static EntityMeta = &UNSTABLE_META;
}

/// Entity with a `distinct` base relation and a non-projected default order.
pub(super) static DISTINCT_FIELDS: &[FieldMeta] = &[
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
        operators: OperatorSet(OPS_I64),
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
        operators: OperatorSet(OPS_TEXT),
    },
];

pub(super) static DISTINCT_META: EntityMeta = EntityMeta {
    entity_name: "DistinctEntity",
    from: "app_user u",
    base_where: None,
    fixed_having: None,
    distinct: true,
    fields: DISTINCT_FIELDS,
    default_order: DEFAULT_ORDER,
    stable_order: DEFAULT_ORDER,
    limits: LIMITS,
    schema_fingerprint: 22,
};

#[derive(Debug)]
pub struct DistinctEntity;

impl SearchEntity for DistinctEntity {
    const META: &'static EntityMeta = &DISTINCT_META;
}

/// Entity whose schema is incompatible with `TestEntity`, for mismatch checks.
pub(super) static OTHER_FIELDS: &[FieldMeta] = &[FieldMeta {
    id: FieldId(0),
    public_name: "id",
    column: "o.id",
    result_alias: Some("id"),
    value_type: ValueType::I64,
    kind: FieldKind::Scalar,
    filterable: true,
    selectable: true,
    sortable: true,
    operators: OperatorSet(OPS_I64),
}];

pub(super) static OTHER_META: EntityMeta = EntityMeta {
    entity_name: "OtherEntity",
    from: "other o",
    base_where: None,
    fixed_having: None,
    distinct: false,
    fields: OTHER_FIELDS,
    default_order: DEFAULT_ORDER,
    stable_order: DEFAULT_ORDER,
    limits: LIMITS,
    schema_fingerprint: 23,
};

#[derive(Debug)]
pub struct OtherEntity;

impl SearchEntity for OtherEntity {
    const META: &'static EntityMeta = &OTHER_META;
}

/// Entity whose public field name is not a valid parameter name.
pub(super) static INVALID_NAME_FIELDS: &[FieldMeta] = &[FieldMeta {
    id: FieldId(0),
    public_name: "1bad",
    column: "u.id",
    result_alias: Some("id"),
    value_type: ValueType::I64,
    kind: FieldKind::Scalar,
    filterable: true,
    selectable: true,
    sortable: true,
    operators: OperatorSet(OPS_I64),
}];

pub(super) static INVALID_NAME_META: EntityMeta = EntityMeta {
    entity_name: "InvalidNameEntity",
    from: "app_user u",
    base_where: None,
    fixed_having: None,
    distinct: false,
    fields: INVALID_NAME_FIELDS,
    default_order: DEFAULT_ORDER,
    stable_order: DEFAULT_ORDER,
    limits: LIMITS,
    schema_fingerprint: 24,
};

#[derive(Debug)]
pub struct InvalidNameEntity;

impl SearchEntity for InvalidNameEntity {
    const META: &'static EntityMeta = &INVALID_NAME_META;
}
