use super::shared::*;

// ---------------------------------------------------------------------------
// Entities covering every optional scalar type, one per feature.
// ---------------------------------------------------------------------------

#[allow(unused_macros)]
macro_rules! feature_entity {
    ($name:ident, $meta:ident, $fields:ident, $order:ident, $fingerprint:literal, $($field:expr),* $(,)?) => {
        static $fields: &[FieldMeta] = &[$($field),*];

        static $meta: EntityMeta = EntityMeta {
            entity_name: stringify!($name),
            from: "app_typed t",
            base_where: None,
            fixed_having: None,
            distinct: false,
            fields: $fields,
            default_order: $order,
            stable_order: $order,
            limits: LIMITS,
            schema_fingerprint: $fingerprint,
        };

        #[derive(Debug)]
        pub struct $name;

        impl SearchEntity for $name {
            const META: &'static EntityMeta = &$meta;
        }
    };
}

pub(super) static TYPED_ORDER: &[OrderMeta] = &[OrderMeta {
    field: FieldId(0),
    direction: SortOrder::Asc,
}];

pub(super) static BASE_TYPED_FIELDS: &[FieldMeta] = &[
    FieldMeta {
        id: FieldId(0),
        public_name: "flag",
        column: "t.flag",
        result_alias: Some("flag"),
        value_type: ValueType::Bool,
        kind: FieldKind::Scalar,
        filterable: true,
        selectable: true,
        sortable: true,
        operators: OperatorSet(OPS_BOOL),
    },
    FieldMeta {
        id: FieldId(1),
        public_name: "count32",
        column: "t.count32",
        result_alias: Some("count32"),
        value_type: ValueType::I32,
        kind: FieldKind::Scalar,
        filterable: true,
        selectable: true,
        sortable: true,
        operators: OperatorSet(OPS_I32),
    },
    FieldMeta {
        id: FieldId(2),
        public_name: "count64",
        column: "t.count64",
        result_alias: Some("count64"),
        value_type: ValueType::I64,
        kind: FieldKind::Scalar,
        filterable: true,
        selectable: true,
        sortable: true,
        operators: OperatorSet(OPS_I64),
    },
    FieldMeta {
        id: FieldId(3),
        public_name: "ratio32",
        column: "t.ratio32",
        result_alias: Some("ratio32"),
        value_type: ValueType::F32,
        kind: FieldKind::Scalar,
        filterable: true,
        selectable: true,
        sortable: true,
        operators: OperatorSet(OPS_F64),
    },
    FieldMeta {
        id: FieldId(4),
        public_name: "ratio64",
        column: "t.ratio64",
        result_alias: Some("ratio64"),
        value_type: ValueType::F64,
        kind: FieldKind::Scalar,
        filterable: true,
        selectable: true,
        sortable: true,
        operators: OperatorSet(OPS_F64),
    },
    FieldMeta {
        id: FieldId(5),
        public_name: "title",
        column: "t.title",
        result_alias: Some("title"),
        value_type: ValueType::Text,
        kind: FieldKind::Scalar,
        filterable: true,
        selectable: true,
        sortable: true,
        operators: OperatorSet(OPS_RICH_TEXT),
    },
    FieldMeta {
        id: FieldId(6),
        public_name: "blob",
        column: "t.blob",
        result_alias: Some("blob"),
        value_type: ValueType::Bytes,
        kind: FieldKind::Scalar,
        filterable: true,
        selectable: true,
        sortable: true,
        operators: OperatorSet(OPS_BYTES),
    },
];

pub(super) static BASE_TYPED_META: EntityMeta = EntityMeta {
    entity_name: "BaseTyped",
    from: "app_typed t",
    base_where: None,
    fixed_having: None,
    distinct: false,
    fields: BASE_TYPED_FIELDS,
    default_order: TYPED_ORDER,
    stable_order: TYPED_ORDER,
    limits: LIMITS,
    schema_fingerprint: 11,
};

#[derive(Debug)]
pub struct BaseTyped;

impl SearchEntity for BaseTyped {
    const META: &'static EntityMeta = &BASE_TYPED_META;
}

#[cfg(feature = "decimal")]
feature_entity!(
    DecimalTyped,
    DECIMAL_META,
    DECIMAL_FIELDS,
    TYPED_ORDER,
    12,
    FieldMeta {
        id: FieldId(0),
        public_name: "amount",
        column: "t.amount",
        result_alias: Some("amount"),
        value_type: ValueType::Decimal,
        kind: FieldKind::Scalar,
        filterable: true,
        selectable: true,
        sortable: true,
        operators: OperatorSet(OPS_F64),
    },
);

#[cfg(feature = "chrono")]
feature_entity!(
    ChronoTyped,
    CHRONO_META,
    CHRONO_FIELDS,
    TYPED_ORDER,
    13,
    FieldMeta {
        id: FieldId(0),
        public_name: "day",
        column: "t.day",
        result_alias: Some("day"),
        value_type: ValueType::Date,
        kind: FieldKind::Scalar,
        filterable: true,
        selectable: true,
        sortable: true,
        operators: OperatorSet(OPS_F64),
    },
    FieldMeta {
        id: FieldId(1),
        public_name: "clock",
        column: "t.clock",
        result_alias: Some("clock"),
        value_type: ValueType::Time,
        kind: FieldKind::Scalar,
        filterable: true,
        selectable: true,
        sortable: true,
        operators: OperatorSet(OPS_F64),
    },
    FieldMeta {
        id: FieldId(2),
        public_name: "localAt",
        column: "t.local_at",
        result_alias: Some("localAt"),
        value_type: ValueType::DateTime,
        kind: FieldKind::Scalar,
        filterable: true,
        selectable: true,
        sortable: true,
        operators: OperatorSet(OPS_F64),
    },
    FieldMeta {
        id: FieldId(3),
        public_name: "instant",
        column: "t.instant",
        result_alias: Some("instant"),
        value_type: ValueType::Timestamp,
        kind: FieldKind::Scalar,
        filterable: true,
        selectable: true,
        sortable: true,
        operators: OperatorSet(OPS_F64),
    },
);

#[cfg(feature = "uuid")]
feature_entity!(
    UuidTyped,
    UUID_META,
    UUID_FIELDS,
    TYPED_ORDER,
    14,
    FieldMeta {
        id: FieldId(0),
        public_name: "external",
        column: "t.external",
        result_alias: Some("external"),
        value_type: ValueType::Uuid,
        kind: FieldKind::Scalar,
        filterable: true,
        selectable: true,
        sortable: true,
        operators: OperatorSet(OPS_F64),
    },
);

#[cfg(feature = "json")]
feature_entity!(
    JsonTyped,
    JSON_META,
    JSON_FIELDS,
    TYPED_ORDER,
    15,
    FieldMeta {
        id: FieldId(0),
        public_name: "payload",
        column: "t.payload",
        result_alias: Some("payload"),
        value_type: ValueType::Json,
        kind: FieldKind::Scalar,
        filterable: false,
        selectable: true,
        sortable: true,
        operators: OperatorSet(OPS_EMPTY),
    },
);
