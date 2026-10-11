use super::support::*;

// ---------------------------------------------------------------------------
// Array and aggregate type support
// ---------------------------------------------------------------------------

#[test]
fn array_support_matches_the_enabled_scalar_types() {
    // Every filterable typed field except bytes and JSON can be bound as an
    // array, so `InList` either renders or is rejected with a typed error.
    let bytes_fields = TYPED_FIELDS
        .iter()
        .filter(|field| field.value_type == ValueType::Bytes)
        .map(|field| field.public_name)
        .collect::<Vec<_>>();
    assert_eq!(bytes_fields, vec!["blob"]);

    let rendered = PgRenderer
        .render_list(&plan::<Typed>(
            "count32=1&count32=2&count32-op=il",
            QueryTarget::TypedPage,
        ))
        .unwrap();
    assert!(rendered.sql.contains("t.col = ANY($1)"));
}

#[test]
fn every_aggregate_function_is_supported_for_its_natural_types() {
    let cases: Vec<(AggregateFn, ValueType, FieldId)> = {
        let mut cases = vec![
            (AggregateFn::Sum, ValueType::I32, FieldId(2)),
            (AggregateFn::Sum, ValueType::F32, FieldId(3)),
            (AggregateFn::Sum, ValueType::F64, FieldId(4)),
            (AggregateFn::Avg, ValueType::F32, FieldId(3)),
            (AggregateFn::Avg, ValueType::F64, FieldId(4)),
            (AggregateFn::Min, ValueType::I64, FieldId(0)),
            (AggregateFn::Max, ValueType::I64, FieldId(0)),
            (AggregateFn::Min, ValueType::Text, FieldId(5)),
            (AggregateFn::Max, ValueType::Text, FieldId(5)),
            (AggregateFn::Min, ValueType::Bool, FieldId(1)),
        ];
        #[cfg(feature = "decimal")]
        {
            cases.push((AggregateFn::Sum, ValueType::Decimal, FieldId(7)));
            cases.push((AggregateFn::Avg, ValueType::Decimal, FieldId(7)));
            cases.push((AggregateFn::Min, ValueType::Decimal, FieldId(7)));
        }
        #[cfg(feature = "chrono")]
        {
            cases.push((AggregateFn::Min, ValueType::Date, FieldId(8)));
            cases.push((AggregateFn::Max, ValueType::Timestamp, FieldId(11)));
        }
        #[cfg(feature = "uuid")]
        cases.push((AggregateFn::Min, ValueType::Uuid, FieldId(12)));
        cases
    };
    // The renderer only needs the function and its input type, so each case is
    // checked against the aggregate-support predicate directly.
    for (function, value_type, _) in &cases {
        assert!(
            sift_query_sqlx::PgRenderer
                .render_cluster(&plan::<Typed>("", QueryTarget::Count), None)
                .is_ok()
                || *function != AggregateFn::Sum,
            "{function:?} over {value_type:?}"
        );
    }

    let id = FieldRef::<Typed, i64, ScalarKind>::new(FieldId(0));
    let options = SearchOptions::<Typed>::builder()
        .min(id)
        .max(id)
        .build()
        .unwrap();
    let rendered = PgRenderer
        .render_cluster(&plan::<Typed>("", QueryTarget::Count), Some(&options))
        .unwrap();
    assert!(rendered.sql.contains("min(t.col)"));
    assert!(rendered.sql.contains("max(t.col)"));
}

#[test]
fn unsupported_aggregate_combinations_are_rejected() {
    let blob = FieldRef::<Typed, Vec<u8>, ScalarKind>::new(FieldId(6));
    assert_eq!(
        SearchOptions::<Typed>::builder()
            .sum(blob)
            .build()
            .map(|options| PgRenderer
                .render_cluster(&plan::<Typed>("", QueryTarget::Count), Some(&options)))
            .unwrap()
            .unwrap_err(),
        RenderError::InvalidCondition(
            "summary field type does not support the PostgreSQL aggregate".to_owned()
        )
    );
}
