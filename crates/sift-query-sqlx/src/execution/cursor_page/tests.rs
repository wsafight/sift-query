use super::*;
use crate::{PgExecutionPolicy, PgSearchError, PgSiftQuery};
use sift_query_core::{
    CursorCodec, EntityLimits, EntityMeta, FieldId, FieldKind, FieldMeta, Operator, OperatorSet,
    OrderMeta, ParamError, ParamParser, QueryPolicy, QueryTarget, RawParams, ScalarValue,
    SearchProjection, ServerScope, SortOrder, ValueType,
};
use sqlx::postgres::PgPoolOptions;

const OPS: &[Operator] = &[Operator::Eq];
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
    operators: OperatorSet(OPS),
}];
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
    schema_fingerprint: 91,
};

struct User;
impl SearchEntity for User {
    const META: &'static EntityMeta = &META;
}

/// A row that cannot supply cursor values, which is what an entity without
/// sortable projected fields looks like at runtime.
struct NoCursor;

impl SearchEntity for NoCursor {
    const META: &'static EntityMeta = &META;
}

struct Projection;
impl SearchProjection<User> for Projection {
    const FIELDS: &'static [sift_query_core::ProjectionField] =
        &[sift_query_core::ProjectionField {
            source: FieldId(0),
            result_alias: "id",
        }];
}

struct ProjectionWithoutCursor;
impl SearchProjection<User> for ProjectionWithoutCursor {
    const FIELDS: &'static [sift_query_core::ProjectionField] =
        &[sift_query_core::ProjectionField {
            source: FieldId(0),
            result_alias: "id",
        }];
}

fn search() -> PgSiftQuery {
    let pool = PgPoolOptions::new()
        .connect_lazy("postgres://localhost:1/unused")
        .unwrap();
    PgSiftQuery::with_policy(
        pool,
        QueryPolicy::default().with_cursor_codec(CursorCodec::new([9u8; 32]).unwrap()),
        PgExecutionPolicy::default(),
    )
    .unwrap()
}

fn cursor_plan(query: &str) -> QueryPlan<User> {
    let policy = QueryPolicy::default().with_cursor_codec(CursorCodec::new([9u8; 32]).unwrap());
    let raw = RawParams::parse(query).unwrap();
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

#[tokio::test]
async fn a_non_cursor_plan_never_produces_a_cursor() {
    let search = search();
    let plan = cursor_plan("");
    assert!(search.entity_cursor(&plan, None).unwrap().is_none());
    assert!(search.map_cursor(&plan, None).unwrap().is_none());
    assert!(
        search
            .projection_cursor::<User, Projection>(&plan, None)
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn a_cursor_page_without_rows_produces_no_cursor() {
    let search = search();
    let plan = cursor_plan("cursor=&size=5");
    assert!(search.entity_cursor(&plan, None).unwrap().is_none());
    assert!(search.map_cursor(&plan, None).unwrap().is_none());
    assert!(
        search
            .projection_cursor::<User, Projection>(&plan, None)
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn an_entity_that_cannot_supply_a_cursor_value_is_reported() {
    let search = search();
    let policy = QueryPolicy::default().with_cursor_codec(CursorCodec::new([9u8; 32]).unwrap());
    let raw = RawParams::parse("cursor=&size=5").unwrap();
    let parsed = ParamParser::new(&policy)
        .unwrap()
        .parse::<NoCursor>(&raw)
        .unwrap();
    let plan = QueryPlan::new(
        &parsed,
        &ServerScope::unrestricted(),
        QueryTarget::TypedPage,
    )
    .unwrap();
    let row = NoCursor;
    let error = search.entity_cursor(&plan, Some(&row)).unwrap_err();
    assert!(
        matches!(error, PgSearchError::Param(ParamError::InvalidValue { ref reason, .. })
            if reason.contains("cannot extract a cursor ordering value")),
        "{error:?}"
    );
}

#[tokio::test]
async fn a_map_row_missing_the_ordering_field_is_reported() {
    let search = search();
    let plan = cursor_plan("cursor=&size=5");
    let mut row = crate::RowMap::new();
    row.insert("other".to_owned(), sift_query_core::RowValue::I64(1));
    let error = search.map_cursor(&plan, Some(&row)).unwrap_err();
    assert!(
        matches!(error, PgSearchError::Param(ParamError::InvalidValue { ref reason, .. })
            if reason.contains("is not projected")),
        "{error:?}"
    );
}

#[tokio::test]
async fn a_map_row_supplies_its_cursor_value() {
    let search = search();
    let plan = cursor_plan("cursor=&size=5");
    let mut row = crate::RowMap::new();
    row.insert("id".to_owned(), sift_query_core::RowValue::I64(4));
    let token = search.map_cursor(&plan, Some(&row)).unwrap().unwrap();
    let codec = CursorCodec::new([9u8; 32]).unwrap();
    assert_eq!(
        codec.decode::<User>(&token, &plan.order).unwrap().values(),
        &[CursorValue::Scalar(ScalarValue::I64(4))]
    );
}

#[tokio::test]
async fn a_null_map_value_becomes_a_null_cursor_key() {
    let search = search();
    let plan = cursor_plan("cursor=&size=5");
    let mut row = crate::RowMap::new();
    row.insert("id".to_owned(), sift_query_core::RowValue::Null);
    let token = search.map_cursor(&plan, Some(&row)).unwrap().unwrap();
    let codec = CursorCodec::new([9u8; 32]).unwrap();
    assert_eq!(
        codec.decode::<User>(&token, &plan.order).unwrap().values(),
        &[CursorValue::Null]
    );
}

#[tokio::test]
async fn a_projection_that_cannot_supply_a_cursor_value_is_reported() {
    let search = search();
    let plan = cursor_plan("cursor=&size=5");
    let error = search
        .projection_cursor::<User, ProjectionWithoutCursor>(&plan, Some(&ProjectionWithoutCursor))
        .unwrap_err();
    assert!(
        matches!(error, PgSearchError::Param(ParamError::InvalidValue { ref reason, .. })
            if reason.contains("projection cannot extract a cursor ordering value")),
        "{error:?}"
    );
}
