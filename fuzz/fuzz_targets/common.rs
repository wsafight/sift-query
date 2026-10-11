#![allow(dead_code)]
use sift_query_core::*;
static FIELDS: &[FieldMeta] = &[FieldMeta {
    id: FieldId(0),
    public_name: "id",
    column: "id",
    result_alias: Some("id"),
    value_type: ValueType::I64,
    kind: FieldKind::Scalar,
    filterable: true,
    selectable: true,
    sortable: true,
    operators: OperatorSet(&[Operator::Eq]),
}];
static ORDER: &[OrderMeta] = &[OrderMeta {
    field: FieldId(0),
    direction: SortOrder::Asc,
}];
pub struct Entity;
impl SearchEntity for Entity {
    const META: &'static EntityMeta = &EntityMeta {
        entity_name: "Fuzz",
        from: "items",
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
        schema_fingerprint: 42,
    };
}
