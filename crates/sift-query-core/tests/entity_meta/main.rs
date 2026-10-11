//! `EntityMeta` validation and lookup, plus `QueryPlan` construction.

use sift_query_core::{
    ConfigError, EntityLimits, EntityMeta, FieldId, FieldKind, FieldMeta, FieldRef, Operator,
    OperatorSet, OrderMeta, PagingMode, ParamError, ParamParser, PlanError, Projection, QueryPlan,
    QueryPolicy, QueryTarget, RawParams, SearchEntity, SearchProjection as _, ServerScope,
    SortOrder, ValueType,
};

#[path = "../support/mod.rs"]
mod support;

use support::{DistinctEntity, RichEntity, TestEntity, UnstableEntity};

fn field(id: u16, public_name: &'static str, kind: FieldKind) -> FieldMeta {
    FieldMeta {
        id: FieldId(id),
        public_name,
        column: "t.col",
        result_alias: Some(public_name),
        value_type: ValueType::I64,
        kind,
        filterable: true,
        selectable: true,
        sortable: true,
        operators: OperatorSet(&[Operator::Eq]),
    }
}

fn meta(entity_name: &'static str, from: &'static str, fields: &'static [FieldMeta]) -> EntityMeta {
    EntityMeta {
        entity_name,
        from,
        base_where: None,
        fixed_having: None,
        distinct: false,
        fields,
        default_order: &[],
        stable_order: &[],
        limits: EntityLimits::default(),
        schema_fingerprint: 1,
    }
}

static ONE_FIELD: &[FieldMeta] = &[FieldMeta {
    id: FieldId(0),
    public_name: "id",
    column: "t.id",
    result_alias: Some("id"),
    value_type: ValueType::I64,
    kind: FieldKind::Scalar,
    filterable: true,
    selectable: true,
    sortable: true,
    operators: OperatorSet(&[Operator::Eq]),
}];

// ---------------------------------------------------------------------------

mod derived_trait_surface;
mod lookup_helpers;
mod queryplan_for_projection;
mod queryplan_new;
mod validate;
