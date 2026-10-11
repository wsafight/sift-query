use super::*;
use crate::PgRenderer;
use sift_query_core::{
    CursorCodec, EntityLimits, EntityMeta, FieldId, FieldKind, FieldMeta, NullOrder, Operator,
    OperatorSet, OrderBy, ParamParser, QueryPolicy, QueryTarget, RawParams, ScalarValue,
    SearchEntity, ServerScope, SortOrder, ValueType,
};

static FIELDS: &[FieldMeta] = &[FieldMeta {
    id: FieldId(0),
    public_name: "id",
    column: "u.id",
    result_alias: Some("id"),
    value_type: ValueType::I64,
    kind: FieldKind::Scalar,
    filterable: true,
    selectable: true,
    sortable: true,
    operators: OperatorSet(&[Operator::Eq]),
}];
static ORDER: &[sift_query_core::OrderMeta] = &[sift_query_core::OrderMeta {
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
    schema_fingerprint: 81,
};

struct User;
impl SearchEntity for User {
    const META: &'static EntityMeta = &META;
}

/// Builds a cursor plan, then pins the ordering and cursor key to the
/// requested direction/null combination. Only the renderer is exercised,
/// so combinations the parser would not accept from parameters alone are
/// reachable here.
fn plan_for(direction: SortOrder, nulls: NullOrder, value: CursorValue) -> QueryPlan<User> {
    let codec = CursorCodec::new([3u8; 32]).unwrap();
    let policy = QueryPolicy::default().with_cursor_codec(codec.clone());
    let raw = RawParams::parse("cursor=&size=5").unwrap();
    let parsed = ParamParser::new(&policy)
        .unwrap()
        .parse::<User>(&raw)
        .unwrap();
    let mut plan = QueryPlan::new(
        &parsed,
        &ServerScope::unrestricted(),
        QueryTarget::TypedPage,
    )
    .unwrap();

    let order = vec![OrderBy {
        field: FieldId(0),
        direction,
        nulls,
    }];
    let token = codec.encode::<User>(&order, &[value]).unwrap();
    let cursor = codec.decode::<User>(&token, &order).unwrap();
    plan.order = order;
    plan.paging.mode = sift_query_core::PagingMode::Cursor(Some(cursor));
    plan
}

fn render(direction: SortOrder, nulls: NullOrder, value: CursorValue) -> String {
    PgRenderer
        .render_list(&plan_for(direction, nulls, value))
        .unwrap()
        .sql
        .to_string()
}

#[test]
fn ascending_with_nulls_last_places_a_null_key_at_the_end() {
    let sql = render(
        SortOrder::Asc,
        NullOrder::Last,
        CursorValue::Scalar(ScalarValue::I64(4)),
    );
    assert!(sql.contains("(u.id > $1 OR u.id IS NULL)"), "{sql}");
}

#[test]
fn descending_with_nulls_first_uses_a_strict_comparison() {
    let sql = render(
        SortOrder::Desc,
        NullOrder::First,
        CursorValue::Scalar(ScalarValue::I64(4)),
    );
    assert!(sql.contains("u.id < $1"), "{sql}");
}

#[test]
fn ascending_with_nulls_first_uses_a_strict_comparison() {
    let sql = render(
        SortOrder::Asc,
        NullOrder::First,
        CursorValue::Scalar(ScalarValue::I64(4)),
    );
    assert!(sql.contains("u.id > $1"), "{sql}");
}

#[test]
fn descending_with_nulls_last_keeps_nulls_in_view() {
    let sql = render(
        SortOrder::Desc,
        NullOrder::Last,
        CursorValue::Scalar(ScalarValue::I64(4)),
    );
    assert!(sql.contains("(u.id < $1 OR u.id IS NULL)"), "{sql}");
}

#[test]
fn a_null_key_with_nulls_first_skips_the_null_bucket() {
    let sql = render(SortOrder::Desc, NullOrder::First, CursorValue::Null);
    assert!(sql.contains("u.id IS NOT NULL"), "{sql}");
}

#[test]
fn a_null_key_with_nulls_last_has_nothing_left_to_scan() {
    let sql = render(SortOrder::Asc, NullOrder::Last, CursorValue::Null);
    assert!(sql.contains("1 = 0"), "{sql}");
}
