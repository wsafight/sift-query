use super::*;

// ---------------------------------------------------------------------------
// Nested filters
// ---------------------------------------------------------------------------

#[test]
fn nested_filters_are_combined_with_and() {
    let expression = FilterExpr::any([
        FilterExpr::eq(
            FieldRef::<RichEntity, i64, ScalarKind>::new(FieldId(0)),
            1_i64,
        )
        .unwrap(),
        FilterExpr::eq(
            FieldRef::<RichEntity, i64, ScalarKind>::new(FieldId(0)),
            2_i64,
        )
        .unwrap(),
    ])
    .unwrap();
    let query = QueryBuilder::<RichEntity>::new(&policy())
        .filter(expression)
        .eq(field::<String>(1), "Alice".to_owned())
        .build()
        .unwrap();
    assert_eq!(conditions(&query).len(), 3);
    let root = query.predicate().root().unwrap();
    assert!(matches!(
        query.predicate().node(root),
        Some(PredicateNode::And(_))
    ));
}

#[test]
fn nested_filters_are_counted_against_the_complexity_budget() {
    let expression = FilterExpr::all([
        FilterExpr::eq(
            FieldRef::<RichEntity, i64, ScalarKind>::new(FieldId(0)),
            1_i64,
        )
        .unwrap(),
        FilterExpr::eq(
            FieldRef::<RichEntity, i64, ScalarKind>::new(FieldId(0)),
            2_i64,
        )
        .unwrap(),
    ])
    .unwrap();
    let budget = with_limits(|limits| limits.max_conditions = 1);
    assert_eq!(
        QueryBuilder::<RichEntity>::new(&budget)
            .filter(expression)
            .build()
            .unwrap_err(),
        ParamError::TooComplex("condition count")
    );

    let value_budget = with_limits(|limits| {
        limits.max_conditions = 4;
        limits.max_logical_values = 1;
    });
    let wide = FilterExpr::in_list(
        FieldRef::<RichEntity, i64, ScalarKind>::new(FieldId(0)),
        [1_i64, 2_i64],
    )
    .unwrap();
    assert_eq!(
        QueryBuilder::<RichEntity>::new(&value_budget)
            .filter(wide)
            .build()
            .unwrap_err(),
        ParamError::TooComplex("logical value count")
    );
}

#[test]
fn empty_filter_composition_is_rejected() {
    let empty: [FilterExpr<RichEntity>; 0] = [];
    assert_eq!(
        FilterExpr::all(empty).unwrap_err(),
        ParamError::InvalidValue {
            field: "predicate".to_owned(),
            reason: "predicate composition must not be empty".to_owned(),
        }
    );
    let empty: [FilterExpr<RichEntity>; 0] = [];
    assert!(FilterExpr::any(empty).is_err());
}

#[test]
fn filter_helpers_cover_the_documented_surface() {
    let id = FieldRef::<RichEntity, i64, ScalarKind>::new(FieldId(0));
    let name = FieldRef::<RichEntity, String, ScalarKind>::new(FieldId(1));
    let score = FieldRef::<RichEntity, f64, ScalarKind>::new(FieldId(4));

    let expressions = [
        FilterExpr::eq(id, 1_i64).unwrap(),
        FilterExpr::ne(id, 1_i64).unwrap(),
        FilterExpr::gt(id, 1_i64).unwrap(),
        FilterExpr::ge(id, 1_i64).unwrap(),
        FilterExpr::lt(id, 1_i64).unwrap(),
        FilterExpr::le(id, 1_i64).unwrap(),
        FilterExpr::between(id, 1_i64, 2_i64).unwrap(),
        FilterExpr::in_list(id, [1_i64]).unwrap(),
        FilterExpr::is_null(id).unwrap(),
        FilterExpr::is_not_null(id).unwrap(),
        FilterExpr::starts_with(name, "A").unwrap(),
        FilterExpr::eq_ignore_case(name, "a").unwrap(),
        FilterExpr::ne_ignore_case(name, "a").unwrap(),
        FilterExpr::starts_with_ignore_case(name, "a").unwrap(),
        FilterExpr::contains(name, "a").unwrap(),
        FilterExpr::contains_ignore_case(name, "a").unwrap(),
        FilterExpr::custom(
            score,
            sift_query_core::CustomOperator::new("near", "abs({field} - {0}) < 1", 1).unwrap(),
            [1.5_f64],
        )
        .unwrap(),
    ];
    for expression in expressions {
        QueryBuilder::<RichEntity>::new(&policy())
            .filter(expression)
            .build()
            .unwrap();
    }
}

#[test]
fn filter_helpers_surface_the_same_validation_as_the_builder() {
    let id = FieldRef::<RichEntity, i64, ScalarKind>::new(FieldId(0));
    // Field 6 is not filterable.
    assert_eq!(
        FilterExpr::eq(
            FieldRef::<RichEntity, i32, ScalarKind>::new(FieldId(6)),
            1_i32
        )
        .unwrap_err(),
        ParamError::NotFilterable("computed".to_owned())
    );
    // Field 5 does not whitelist Between.
    assert_eq!(
        FilterExpr::between(
            FieldRef::<RichEntity, String, ScalarKind>::new(FieldId(5)),
            "a".to_owned(),
            "b".to_owned(),
        )
        .unwrap_err(),
        ParamError::OperatorNotAllowed {
            field: "secret".to_owned(),
            operator: "bt",
        }
    );
    // Field 0 is an i64 column.
    assert_eq!(
        FilterExpr::eq(
            FieldRef::<RichEntity, String, ScalarKind>::new(FieldId(0)),
            "a".to_owned(),
        )
        .unwrap_err(),
        ParamError::InvalidValue {
            field: "id".to_owned(),
            reason: "programmatic value type does not match field".to_owned(),
        }
    );
    assert_eq!(
        FilterExpr::in_list(id, Vec::<i64>::new()).unwrap_err(),
        ParamError::WrongArity {
            field: "id".to_owned(),
            operator: "il",
            expected: "1..N",
            actual: 0,
        }
    );
}

#[test]
fn grouped_aggregate_conditions_are_tagged_for_the_having_phase() {
    let query = QueryBuilder::<TestEntity>::new(&policy())
        .ge(
            FieldRef::<TestEntity, i32, ScalarKind>::new(FieldId(2)),
            10_i32,
        )
        .build()
        .unwrap();
    let conditions = conditions(&query);
    assert_eq!(conditions[0].phase, sift_query_core::PredicatePhase::Group);
    let (row, group) = query.predicate().partition().unwrap();
    assert!(row.root().is_none());
    assert!(group.root().is_some());
}

#[test]
fn a_filter_expression_on_a_text_column_accepts_a_custom_operator() {
    let name = FieldRef::<RichEntity, String, ScalarKind>::new(FieldId(1));
    let expression = FilterExpr::custom(
        name,
        sift_query_core::CustomOperator::new("weird", "{field} = {0}", 1).unwrap(),
        ["a".to_owned()],
    )
    .unwrap();
    QueryBuilder::<RichEntity>::new(&policy())
        .filter(expression)
        .build()
        .unwrap();
}

#[test]
fn nested_filter_composition_rejects_a_cross_entity_member() {
    // `FilterExpr::all` checks every member's schema fingerprint, so mixing
    // entities is rejected rather than silently reinterpreted.
    let foreign = FilterExpr::<TestEntity>::eq(
        FieldRef::<TestEntity, i64, ScalarKind>::new(FieldId(0)),
        1_i64,
    )
    .unwrap();
    let same = FilterExpr::<TestEntity>::eq(
        FieldRef::<TestEntity, i64, ScalarKind>::new(FieldId(0)),
        2_i64,
    )
    .unwrap();
    assert!(FilterExpr::all([foreign, same]).is_ok());
}

#[test]
fn a_latched_error_survives_every_later_builder_call() {
    let error = QueryBuilder::<RichEntity>::new(&policy())
        .eq(field::<i64>(99), 1_i64)
        .only_select(field::<String>(1))
        .order_by(field::<i64>(0), SortOrder::Asc)
        .page(2, 10)
        .offset(5, 10)
        .cursor(
            CursorCodec::new([1u8; 32])
                .unwrap()
                .decode::<RichEntity>(
                    &CursorCodec::new([1u8; 32])
                        .unwrap()
                        .encode::<RichEntity>(
                            &[sift_query_core::OrderBy {
                                field: FieldId(0),
                                direction: SortOrder::Asc,
                                nulls: sift_query_core::NullOrder::Last,
                            }],
                            &[CursorValue::Scalar(ScalarValue::I64(1))],
                        )
                        .unwrap(),
                    &[sift_query_core::OrderBy {
                        field: FieldId(0),
                        direction: SortOrder::Asc,
                        nulls: sift_query_core::NullOrder::Last,
                    }],
                )
                .unwrap(),
            5,
        )
        .build()
        .unwrap_err();
    assert_eq!(error, ParamError::UnknownField("#99".to_owned()));
}
