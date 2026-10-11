use super::*;

// ---------------------------------------------------------------------------
// Happy paths
// ---------------------------------------------------------------------------

#[test]
fn every_operator_helper_builds_a_validated_condition() {
    let query = QueryBuilder::<RichEntity>::new(&policy())
        .eq(field::<i64>(0), 1_i64)
        .ne(field::<i64>(0), 2_i64)
        .gt(field::<i64>(0), 3_i64)
        .ge(field::<i64>(0), 4_i64)
        .lt(field::<i64>(0), 5_i64)
        .le(field::<i64>(0), 6_i64)
        .between(field::<i64>(0), 7_i64, 8_i64)
        .not_between(field::<i64>(0), 9_i64, 10_i64)
        .in_list(field::<i64>(0), [11_i64, 12_i64])
        .not_in(field::<i64>(0), [13_i64])
        .is_null(field::<i64>(0))
        .is_not_null(field::<i64>(0))
        .always_true(field::<i64>(0))
        .always_false(field::<i64>(0))
        .empty(field::<String>(1))
        .not_empty(field::<String>(1))
        .starts_with(field::<String>(1), "Al")
        .starts_with_ignore_case(field::<String>(1), "al")
        .ends_with(field::<String>(1), "ce")
        .contains(field::<String>(1), "li")
        .contains_ignore_case(field::<String>(1), "LI")
        .not_like(field::<String>(1), "A%")
        .or_like(field::<String>(1), ["A%", "B%"])
        .eq_ignore_case(field::<String>(1), "alice")
        .ne_ignore_case(field::<String>(1), "bob")
        .build()
        .unwrap();

    let conditions = conditions(&query);
    assert_eq!(conditions.len(), 25);
    assert_eq!(conditions[0].operator, Operator::Eq);
    assert_eq!(conditions[23].operator, Operator::Eq);
    assert!(conditions[23].ignore_case);
    assert_eq!(conditions[24].operator, Operator::Ne);
    assert_eq!(
        conditions[20].values,
        vec![ScalarValue::Text("LI".to_owned())]
    );
}

#[test]
fn custom_operator_bypasses_the_field_whitelist() {
    let operator = sift_query_core::CustomOperator::new("parity", "mod({field}, {0})", 1).unwrap();
    let query = QueryBuilder::<RichEntity>::new(&policy())
        .custom(field::<String>(1), operator, ["2".to_owned()])
        .build()
        .unwrap();
    let conditions = conditions(&query);
    assert_eq!(conditions[0].operator, Operator::Custom(operator));
}

#[test]
fn projection_helpers_resolve_to_ordered_field_ids() {
    let only = QueryBuilder::<RichEntity>::new(&policy())
        .only_select(field::<i32>(2))
        .only_select(field::<String>(1))
        .build()
        .unwrap();
    // `Only` keeps metadata declaration order, not call order.
    assert_eq!(
        only.projection(),
        &Projection::Fields(vec![FieldId(1), FieldId(2)])
    );

    let excluded = QueryBuilder::<RichEntity>::new(&policy())
        .exclude(field::<String>(1))
        .build()
        .unwrap();
    assert_eq!(
        excluded.projection(),
        &Projection::Fields(vec![
            FieldId(0),
            FieldId(2),
            FieldId(3),
            FieldId(4),
            FieldId(6)
        ])
    );

    let all = QueryBuilder::<RichEntity>::new(&policy()).build().unwrap();
    assert_eq!(all.projection(), &Projection::All);
}

#[test]
fn ordering_appends_the_entity_stable_order() {
    let query = QueryBuilder::<RichEntity>::new(&policy())
        .order_by(field::<String>(1), SortOrder::Desc)
        .build()
        .unwrap();
    let order = query.order();
    assert_eq!(order.len(), 2);
    assert_eq!(order[0].field, FieldId(1));
    assert_eq!(order[0].direction, SortOrder::Desc);
    // Descending order puts NULLs first.
    assert_eq!(order[0].nulls, sift_query_core::NullOrder::First);
    assert_eq!(order[1].field, FieldId(0));
}

#[test]
fn a_condition_added_after_an_error_never_overrides_it() {
    // The builder latches the first error and ignores later work.
    let error = QueryBuilder::<RichEntity>::new(&policy())
        .eq(field::<i64>(99), 1_i64)
        .eq(field::<i64>(0), 1_i64)
        .build()
        .unwrap_err();
    assert_eq!(error, ParamError::UnknownField("#99".to_owned()));
}
