use super::*;
use sift_query_core::AggregateFn;

fn every_value_type() -> Vec<ValueType> {
    let mut types = vec![
        ValueType::Bool,
        ValueType::I32,
        ValueType::I64,
        ValueType::F32,
        ValueType::F64,
        ValueType::Text,
        ValueType::Bytes,
    ];
    #[cfg(feature = "decimal")]
    types.push(ValueType::Decimal);
    #[cfg(feature = "chrono")]
    types.extend([
        ValueType::Date,
        ValueType::Time,
        ValueType::DateTime,
        ValueType::Timestamp,
    ]);
    #[cfg(feature = "uuid")]
    types.push(ValueType::Uuid);
    #[cfg(feature = "json")]
    types.push(ValueType::Json);
    types
}

#[test]
fn array_support_covers_every_enabled_scalar_type() {
    for value_type in every_value_type() {
        let expected = !matches!(value_type, ValueType::Bytes | ValueType::Json);
        assert_eq!(
            supports_array(value_type),
            expected,
            "unexpected array support for {value_type:?}"
        );
    }
}

#[test]
fn sum_support_matches_postgres_aggregate_semantics() {
    for value_type in every_value_type() {
        let expected = match value_type {
            ValueType::I32 | ValueType::F32 | ValueType::F64 => true,
            #[cfg(feature = "decimal")]
            ValueType::I64 | ValueType::Decimal => true,
            _ => false,
        };
        assert_eq!(
            supports_aggregate(AggregateFn::Sum, value_type),
            expected,
            "unexpected sum support for {value_type:?}"
        );
    }
}

#[test]
fn avg_support_matches_postgres_aggregate_semantics() {
    for value_type in every_value_type() {
        let expected = match value_type {
            ValueType::F32 | ValueType::F64 => true,
            #[cfg(feature = "decimal")]
            ValueType::I32 | ValueType::I64 | ValueType::Decimal => true,
            _ => false,
        };
        assert_eq!(
            supports_aggregate(AggregateFn::Avg, value_type),
            expected,
            "unexpected avg support for {value_type:?}"
        );
    }
}

#[test]
fn min_and_max_support_every_orderable_type() {
    for value_type in every_value_type() {
        // Orderable scalar types only: no boolean, bytea, or jsonb.
        let expected = match value_type {
            ValueType::I32 | ValueType::I64 | ValueType::F32 | ValueType::F64 | ValueType::Text => {
                true
            }
            #[cfg(feature = "decimal")]
            ValueType::Decimal => true,
            #[cfg(feature = "chrono")]
            ValueType::Date | ValueType::Time | ValueType::DateTime | ValueType::Timestamp => true,
            #[cfg(feature = "uuid")]
            ValueType::Uuid => true,
            _ => false,
        };
        assert_eq!(
            supports_aggregate(AggregateFn::Min, value_type),
            expected,
            "min over {value_type:?}"
        );
        assert_eq!(
            supports_aggregate(AggregateFn::Max, value_type),
            expected,
            "max over {value_type:?}"
        );
    }
}

#[test]
fn custom_aggregates_are_always_supported() {
    let custom = AggregateFn::Custom(
        sift_query_core::CustomAggregate::new("x", "count({field})", ValueType::I64).unwrap(),
    );
    for value_type in every_value_type() {
        assert!(supports_aggregate(custom, value_type));
    }
}

#[test]
fn aggregate_rendering_substitutes_the_field() {
    assert_eq!(render_aggregate(AggregateFn::Sum, "t.v"), "sum(t.v)");
    assert_eq!(render_aggregate(AggregateFn::Avg, "t.v"), "avg(t.v)");
    assert_eq!(render_aggregate(AggregateFn::Min, "t.v"), "min(t.v)");
    assert_eq!(render_aggregate(AggregateFn::Max, "t.v"), "max(t.v)");
    let custom = AggregateFn::Custom(
        sift_query_core::CustomAggregate::new(
            "spread",
            "max({field}) - min({field})",
            ValueType::I64,
        )
        .unwrap(),
    );
    assert_eq!(render_aggregate(custom, "t.v"), "max(t.v) - min(t.v)");
}
