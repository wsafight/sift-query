use super::support::*;

// ---------------------------------------------------------------------------
// Text operators and ignore-case
// ---------------------------------------------------------------------------

#[test]
fn ignore_case_equality_uses_lower_on_both_sides() {
    let rendered = PgRenderer
        .render_list(&plan::<User>(
            "name=Alice&name-op=eq&name-ic=true",
            QueryTarget::TypedPage,
        ))
        .unwrap();
    assert!(rendered.sql.contains("lower(u.name) = lower($1)"));

    let rendered = PgRenderer
        .render_list(&plan::<User>(
            "name=Alice&name-op=ne&name-ic=true",
            QueryTarget::TypedPage,
        ))
        .unwrap();
    assert!(rendered.sql.contains("lower(u.name) <> lower($1)"));
}

#[test]
fn ignore_case_text_matching_uses_ilike() {
    let cases = [
        (
            "name=Al&name-op=sw&name-ic=true",
            "u.name ILIKE $1 ESCAPE '\\'",
        ),
        (
            "name=ce&name-op=ew&name-ic=true",
            "u.name ILIKE $1 ESCAPE '\\'",
        ),
        (
            "name=li&name-op=ct&name-ic=true",
            "u.name ILIKE $1 ESCAPE '\\'",
        ),
        ("name=A&name-op=nk&name-ic=true", "u.name NOT ILIKE $1"),
        ("name=A%25&name-op=ol&name-ic=true", "(u.name ILIKE $1)"),
    ];
    for (query, expected) in cases {
        let rendered = PgRenderer
            .render_list(&plan::<User>(query, QueryTarget::TypedPage))
            .unwrap();
        assert!(
            rendered.sql.contains(expected),
            "expected {expected:?} in {:?}",
            rendered.sql
        );
    }
}

#[test]
fn empty_and_null_text_operators_render_without_binds() {
    let cases = [
        ("name-op=ey", "(u.name IS NULL OR u.name = '')"),
        ("name-op=ny", "(u.name IS NOT NULL AND u.name <> '')"),
        ("id-op=nl", "u.id IS NULL"),
        ("id-op=nn", "u.id IS NOT NULL"),
    ];
    for (query, expected) in cases {
        let rendered = PgRenderer
            .render_list(&plan::<User>(query, QueryTarget::TypedPage))
            .unwrap();
        assert!(rendered.sql.contains(expected), "query {query}");
        assert!(
            rendered.binds.is_empty() || rendered.binds.len() == 2,
            "query {query}"
        );
    }
}

#[test]
fn unsupported_operator_value_shapes_are_reported() {
    // Bytes cannot be used with an array bind.
    let rendered = PgRenderer
        .render_list(&plan::<Typed>("blob=AA&blob-op=il", QueryTarget::TypedPage))
        .unwrap_err();
    assert!(
        matches!(rendered, RenderError::UnsupportedBindType(_)),
        "{rendered:?}"
    );
}

#[test]
fn custom_operators_substitute_every_placeholder() {
    let operator =
        CustomOperator::new("between_exclusive", "{field} > {0} AND {field} < {1}", 2).unwrap();
    let field = FieldRef::<User, i64, ScalarKind>::new(FieldId(0));
    let expression = FilterExpr::custom(field, operator, [1_i64, 9_i64]).unwrap();
    let policy = QueryPolicy::default();
    let query = QueryBuilder::<User>::new(&policy)
        .filter(expression)
        .build()
        .unwrap();
    let built =
        QueryPlan::new(&query, &ServerScope::unrestricted(), QueryTarget::TypedPage).unwrap();
    let rendered = PgRenderer.render_list(&built).unwrap();
    assert!(
        rendered.sql.contains("u.id > $1 AND u.id < $2"),
        "{}",
        rendered.sql
    );
}
