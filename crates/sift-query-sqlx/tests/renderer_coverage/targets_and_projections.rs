use super::support::*;

// ---------------------------------------------------------------------------
// Targets and projections
// ---------------------------------------------------------------------------

#[test]
fn list_rendering_rejects_a_non_list_target() {
    let count_plan = plan::<User>("", QueryTarget::Count);
    assert_eq!(
        PgRenderer.render_list(&count_plan).unwrap_err(),
        RenderError::WrongTarget
    );
    let sum_plan = plan::<User>("", QueryTarget::Sum);
    assert_eq!(
        PgRenderer.render_list(&sum_plan).unwrap_err(),
        RenderError::WrongTarget
    );
}

#[test]
fn every_list_target_renders() {
    for target in [
        QueryTarget::TypedPage,
        QueryTarget::MapPage,
        QueryTarget::TypedList,
        QueryTarget::First,
    ] {
        let rendered = PgRenderer.render_list(&plan::<User>("", target)).unwrap();
        assert!(
            rendered.sql.starts_with("SELECT u.id AS \"id\""),
            "{target:?}"
        );
    }
}

#[test]
fn dynamic_projections_are_rendered_for_map_targets() {
    let rendered = PgRenderer
        .render_list(&plan::<User>("onlySelect=name", QueryTarget::MapPage))
        .unwrap();
    assert!(rendered.sql.starts_with("SELECT u.name AS \"name\""));

    // Counting a dynamic projection renders no aliases.
    let counted = PgRenderer
        .render_count(&plan::<User>("onlySelect=name", QueryTarget::Count))
        .unwrap();
    assert!(counted.sql.contains("FROM app_user u"));
}

#[test]
fn an_empty_projection_is_reported_rather_than_rendered() {
    // A projection that survives planning but renders to nothing.
    let mut empty = plan::<User>("", QueryTarget::TypedPage);
    empty.projection.clear();
    assert_eq!(
        PgRenderer.render_list(&empty).unwrap_err(),
        RenderError::InvalidCondition("empty projection".to_owned())
    );
}

#[test]
fn unknown_projected_field_ids_are_reported() {
    let mut broken = plan::<User>("", QueryTarget::TypedPage);
    broken.projection = vec![FieldId(99)];
    assert_eq!(
        PgRenderer.render_list(&broken).unwrap_err(),
        RenderError::UnknownField(99)
    );

    let mut broken_order = plan::<User>("", QueryTarget::TypedPage);
    broken_order.order = vec![sift_query_core::OrderBy {
        field: FieldId(99),
        direction: SortOrder::Asc,
        nulls: sift_query_core::NullOrder::Last,
    }];
    assert_eq!(
        PgRenderer.render_list(&broken_order).unwrap_err(),
        RenderError::UnknownField(99)
    );
}

pub(super) static NO_ALIAS: &[FieldMeta] = &[FieldMeta {
    id: FieldId(0),
    public_name: "id",
    column: "u.id",
    result_alias: None,
    value_type: ValueType::I64,
    kind: FieldKind::Scalar,
    filterable: true,
    selectable: true,
    sortable: true,
    operators: OperatorSet(ID_OPS),
}];
pub(super) static NO_ALIAS_META: EntityMeta = EntityMeta {
    entity_name: "NoAlias",
    from: "app_user u",
    base_where: None,
    fixed_having: None,
    distinct: false,
    fields: NO_ALIAS,
    default_order: ORDER,
    stable_order: ORDER,
    limits: EntityLimits {
        max_page_size: 100,
        max_offset: 20_000,
        timeout_ms: 5_000,
    },
    schema_fingerprint: 35,
};
pub(super) struct NoAlias;
impl SearchEntity for NoAlias {
    const META: &'static EntityMeta = &NO_ALIAS_META;
}

#[test]
fn a_projected_field_without_an_alias_cannot_be_named() {
    let mut broken = plan::<NoAlias>("", QueryTarget::TypedPage);
    broken.projection = vec![FieldId(0)];
    assert_eq!(
        PgRenderer.render_list(&broken).unwrap_err(),
        RenderError::MissingAlias("id".to_owned())
    );
}
