use super::*;

#[test]
fn summary_output_types_follow_the_aggregate_semantics() {
    assert_eq!(
        summary_output_type(AggregateFn::Sum, ValueType::I32).unwrap(),
        ValueType::I64
    );
    assert_eq!(
        summary_output_type(AggregateFn::Sum, ValueType::F32).unwrap(),
        ValueType::F32
    );
    assert_eq!(
        summary_output_type(AggregateFn::Sum, ValueType::F64).unwrap(),
        ValueType::F64
    );
    #[cfg(feature = "decimal")]
    {
        assert_eq!(
            summary_output_type(AggregateFn::Sum, ValueType::I64).unwrap(),
            ValueType::Decimal
        );
        assert_eq!(
            summary_output_type(AggregateFn::Sum, ValueType::Decimal).unwrap(),
            ValueType::Decimal
        );
        assert_eq!(
            summary_output_type(AggregateFn::Avg, ValueType::I32).unwrap(),
            ValueType::Decimal
        );
        assert_eq!(
            summary_output_type(AggregateFn::Avg, ValueType::I64).unwrap(),
            ValueType::Decimal
        );
        assert_eq!(
            summary_output_type(AggregateFn::Avg, ValueType::Decimal).unwrap(),
            ValueType::Decimal
        );
    }
    assert_eq!(
        summary_output_type(AggregateFn::Avg, ValueType::F32).unwrap(),
        ValueType::F64
    );
    assert_eq!(
        summary_output_type(AggregateFn::Avg, ValueType::F64).unwrap(),
        ValueType::F64
    );
    assert_eq!(
        summary_output_type(AggregateFn::Min, ValueType::Text).unwrap(),
        ValueType::Text
    );
    assert_eq!(
        summary_output_type(AggregateFn::Max, ValueType::Bool).unwrap(),
        ValueType::Bool
    );

    let custom = AggregateFn::Custom(
        sift_query_core::CustomAggregate::new("x", "count({field})", ValueType::I64).unwrap(),
    );
    assert_eq!(
        summary_output_type(custom, ValueType::Text).unwrap(),
        ValueType::I64
    );
}

#[test]
fn unsupported_summary_combinations_are_reported() {
    let error = summary_output_type(AggregateFn::Sum, ValueType::Text).unwrap_err();
    assert!(
        matches!(error, PgSearchError::Decode(DecodeError { ref column, .. }) if column == "summary"),
        "{error:?}"
    );
    assert!(summary_output_type(AggregateFn::Avg, ValueType::Bool).is_err());
    assert!(summary_output_type(AggregateFn::Sum, ValueType::Bytes).is_err());
    assert!(summary_output_type(AggregateFn::Avg, ValueType::Text).is_err());
}
