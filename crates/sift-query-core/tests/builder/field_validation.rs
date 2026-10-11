use super::*;

// ---------------------------------------------------------------------------
// Field validation
// ---------------------------------------------------------------------------

#[test]
fn conditions_reject_unknown_unfilterable_and_mistyped_fields() {
    assert_eq!(
        QueryBuilder::<RichEntity>::new(&policy())
            .eq(field::<i64>(99), 1_i64)
            .build()
            .unwrap_err(),
        ParamError::UnknownField("#99".to_owned())
    );

    assert_eq!(
        QueryBuilder::<RichEntity>::new(&policy())
            .eq(field::<i32>(6), 1_i32)
            .build()
            .unwrap_err(),
        ParamError::NotFilterable("computed".to_owned())
    );

    // Field 0 is an i64 column; a Text value cannot be bound to it.
    assert_eq!(
        QueryBuilder::<RichEntity>::new(&policy())
            .eq(field::<String>(0), "one".to_owned())
            .build()
            .unwrap_err(),
        ParamError::InvalidValue {
            field: "id".to_owned(),
            reason: "programmatic value type does not match field".to_owned(),
        }
    );
}

#[test]
fn conditions_reject_disallowed_operators_and_arity_mismatches() {
    // `secret` only whitelists Eq and StartWith.
    assert_eq!(
        QueryBuilder::<RichEntity>::new(&policy())
            .between(field::<String>(5), "a".to_owned(), "b".to_owned())
            .build()
            .unwrap_err(),
        ParamError::OperatorNotAllowed {
            field: "secret".to_owned(),
            operator: "bt",
        }
    );

    assert_eq!(
        QueryBuilder::<RichEntity>::new(&policy())
            .in_list(field::<i64>(0), [])
            .build()
            .unwrap_err(),
        ParamError::WrongArity {
            field: "id".to_owned(),
            operator: "il",
            expected: "1..N",
            actual: 0,
        }
    );
}

#[test]
fn ignore_case_on_a_non_text_field_reports_the_value_type_mismatch() {
    // `eq_ignore_case` always produces a `Text` value, so a non-text column is
    // reported by the value-type guard, which runs before the ignore-case one.
    // Field 0 is an i64 column even though the typed `FieldRef` claims text.
    assert_eq!(
        QueryBuilder::<RichEntity>::new(&policy())
            .eq_ignore_case(field::<String>(0), "one")
            .build()
            .unwrap_err(),
        ParamError::InvalidValue {
            field: "id".to_owned(),
            reason: "programmatic value type does not match field".to_owned(),
        }
    );
}

#[test]
fn projection_helpers_reject_unselectable_and_unknown_fields() {
    assert_eq!(
        QueryBuilder::<RichEntity>::new(&policy())
            .only_select(field::<String>(5))
            .build()
            .unwrap_err(),
        ParamError::NotSelectable("secret".to_owned())
    );

    assert_eq!(
        QueryBuilder::<RichEntity>::new(&policy())
            .exclude(field::<i64>(99))
            .build()
            .unwrap_err(),
        ParamError::UnknownField("#99".to_owned())
    );

    assert_eq!(
        QueryBuilder::<RichEntity>::new(&policy())
            .only_select(field::<i32>(2))
            .only_select(field::<i32>(2))
            .build()
            .unwrap_err(),
        ParamError::DuplicateParameter("projection age".to_owned())
    );

    assert_eq!(
        QueryBuilder::<RichEntity>::new(&policy())
            .only_select(field::<i32>(2))
            .exclude(field::<String>(1))
            .build()
            .unwrap_err(),
        ParamError::ConflictingParameters("only-select and exclude projection".to_owned())
    );
}

#[test]
fn excluding_every_selectable_field_is_an_empty_projection() {
    assert_eq!(
        QueryBuilder::<RichEntity>::new(&policy())
            .exclude(field::<i64>(0))
            .exclude(field::<String>(1))
            .exclude(field::<i32>(2))
            .exclude(field::<bool>(3))
            .exclude(field::<f64>(4))
            .exclude(field::<i32>(6))
            .build()
            .unwrap_err(),
        ParamError::EmptyProjection
    );
}

#[test]
fn ordering_rejects_unknown_unsortable_and_duplicate_fields() {
    assert_eq!(
        QueryBuilder::<RichEntity>::new(&policy())
            .order_by(field::<i64>(99), SortOrder::Asc)
            .build()
            .unwrap_err(),
        ParamError::UnknownField("#99".to_owned())
    );

    assert_eq!(
        QueryBuilder::<RichEntity>::new(&policy())
            .order_by(field::<i32>(6), SortOrder::Asc)
            .build()
            .unwrap_err(),
        ParamError::NotSortable("computed".to_owned())
    );

    assert_eq!(
        QueryBuilder::<RichEntity>::new(&policy())
            .order_by(field::<i64>(0), SortOrder::Asc)
            .order_by(field::<i64>(0), SortOrder::Desc)
            .build()
            .unwrap_err(),
        ParamError::DuplicateParameter("sort id".to_owned())
    );
}
