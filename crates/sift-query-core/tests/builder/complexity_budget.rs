use super::*;

// ---------------------------------------------------------------------------
// Complexity budget
// ---------------------------------------------------------------------------

#[test]
fn complexity_budget_covers_conditions_values_sorts_and_ordering() {
    let condition_budget = with_limits(|limits| limits.max_conditions = 1);
    assert_eq!(
        QueryBuilder::<RichEntity>::new(&condition_budget)
            .eq(field::<i64>(0), 1_i64)
            .eq(field::<i64>(0), 2_i64)
            .build()
            .unwrap_err(),
        ParamError::TooComplex("condition count")
    );

    let value_budget = with_limits(|limits| {
        limits.max_conditions = 4;
        limits.max_logical_values = 2;
    });
    assert_eq!(
        QueryBuilder::<RichEntity>::new(&value_budget)
            .in_list(field::<i64>(0), [1_i64, 2_i64, 3_i64])
            .build()
            .unwrap_err(),
        ParamError::TooComplex("logical value count")
    );

    let sort_budget = with_limits(|limits| limits.max_sort_fields = 1);
    assert_eq!(
        QueryBuilder::<RichEntity>::new(&sort_budget)
            .order_by(field::<i64>(0), SortOrder::Asc)
            .order_by(field::<String>(1), SortOrder::Asc)
            .build()
            .unwrap_err(),
        ParamError::TooComplex("sort field count")
    );

    let per_condition = with_limits(|limits| limits.max_values_per_condition = 1);
    assert_eq!(
        QueryBuilder::<RichEntity>::new(&per_condition)
            .in_list(field::<i64>(0), [1_i64, 2_i64])
            .build()
            .unwrap_err(),
        ParamError::TooComplex("values per condition")
    );
}

#[test]
fn pagination_requires_a_stable_entity_order() {
    assert_eq!(
        QueryBuilder::<UnstableEntity>::new(&policy())
            .build()
            .unwrap_err(),
        ParamError::UnstablePagination
    );
}
