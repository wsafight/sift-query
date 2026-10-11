use super::support::*;

// ---------------------------------------------------------------------------
// Predicate tree shapes
// ---------------------------------------------------------------------------

#[test]
fn nested_boolean_groups_render_with_the_right_separators() {
    let policy = QueryPolicy::default();
    let alternatives = FilterExpr::any([
        FilterExpr::eq(FieldRef::<User, i64, ScalarKind>::new(FieldId(0)), 1_i64).unwrap(),
        FilterExpr::eq(FieldRef::<User, i64, ScalarKind>::new(FieldId(0)), 2_i64).unwrap(),
    ])
    .unwrap();
    let query = QueryBuilder::<User>::new(&policy)
        .filter(alternatives)
        .eq(
            FieldRef::<User, String, ScalarKind>::new(FieldId(1)),
            "A".to_owned(),
        )
        .build()
        .unwrap();
    let built =
        QueryPlan::new(&query, &ServerScope::unrestricted(), QueryTarget::TypedPage).unwrap();
    let rendered = PgRenderer.render_list(&built).unwrap();
    assert!(
        rendered
            .sql
            .contains("(u.name = $1 AND (u.id = $2 OR u.id = $3))"),
        "{}",
        rendered.sql
    );
}

#[test]
fn server_scopes_render_before_the_user_predicate() {
    let policy = QueryPolicy::default();
    let scope = ServerScope::<User>::new()
        .eq(
            FieldRef::<User, i64, RowPhaseKindImpl>::new(FieldId(0)),
            7_i64,
        )
        .build()
        .unwrap();
    let raw = RawParams::parse("name=A").unwrap();
    let query = ParamParser::new(&policy)
        .unwrap()
        .parse::<User>(&raw)
        .unwrap();
    let built = QueryPlan::new(&query, &scope, QueryTarget::TypedPage).unwrap();
    let rendered = PgRenderer.render_list(&built).unwrap();
    assert!(
        rendered
            .sql
            .contains("WHERE u.deleted = false AND u.id = $1 AND u.name = $2"),
        "{}",
        rendered.sql
    );
}

/// Marker satisfying `RowPhaseKind` for server-scope helpers.
struct RowPhaseKindImpl;

impl RowPhaseKind for RowPhaseKindImpl {}
