use super::*;

// ---------------------------------------------------------------------------
// Complexity budgets
// ---------------------------------------------------------------------------

#[test]
fn parser_enforces_every_complexity_budget() {
    assert_eq!(
        rich_with(
            &with_limits(|limits| limits.max_conditions = 1),
            "id=1&name=A"
        )
        .unwrap_err(),
        ParamError::TooComplex("condition count")
    );

    assert_eq!(
        rich_with(
            &with_limits(|limits| {
                limits.max_conditions = 4;
                limits.max_logical_values = 1;
            }),
            "id=1&id=2&id-op=il"
        )
        .unwrap_err(),
        ParamError::TooComplex("logical value count")
    );

    assert_eq!(
        rich_with(
            &with_limits(|limits| {
                limits.max_conditions = 4;
                limits.max_values_per_condition = 1;
            }),
            "id=1&id=2&id-op=il"
        )
        .unwrap_err(),
        ParamError::TooComplex("values per condition")
    );

    assert_eq!(
        rich_with(
            &with_limits(|limits| {
                limits.max_conditions = 8;
                limits.max_bind_slots = 2;
            }),
            "id=1&name=A&age=2"
        )
        .unwrap_err(),
        ParamError::TooComplex("database bind slot count")
    );

    assert_eq!(
        rich_with(
            &with_limits(|limits| limits.max_sort_fields = 1),
            "sort=id,name"
        )
        .unwrap_err(),
        ParamError::TooComplex("sort field count")
    );

    assert_eq!(
        rich_with(
            &with_limits(|limits| limits.max_group_expr_bytes = 2),
            "A.id=1&gexpr=AAAA"
        )
        .unwrap_err(),
        ParamError::TooComplex("gexpr length")
    );
}

#[test]
fn zero_arity_operators_consume_no_bind_slots() {
    // With one bind slot, a single zero-arity condition plus one value
    // condition still fits, because IS NULL binds nothing.
    let policy = with_limits(|limits| {
        limits.max_conditions = 8;
        limits.max_bind_slots = 1;
    });
    assert!(rich_with(&policy, "id-op=nl&name=A").is_ok());
    assert!(rich_with(&policy, "id-op=at&name=A").is_ok());
    // In-list conditions consume exactly one slot regardless of arity.
    assert!(rich_with(&policy, "id=1&id=2&id-op=il").is_ok());
    // Two bound values, however, need two slots.
    assert_eq!(
        rich_with(&policy, "id=1&name=A").unwrap_err(),
        ParamError::TooComplex("database bind slot count")
    );
}
