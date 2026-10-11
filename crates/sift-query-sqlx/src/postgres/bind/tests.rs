use super::*;
use sift_query_core::ScalarValue;

fn scalars() -> Vec<PgBindValue> {
    let mut binds = vec![
        PgBindValue::Scalar(ScalarValue::Bool(true)),
        PgBindValue::Scalar(ScalarValue::I32(1)),
        PgBindValue::Scalar(ScalarValue::I64(2)),
        PgBindValue::Scalar(ScalarValue::F32(1.5)),
        PgBindValue::Scalar(ScalarValue::F64(2.5)),
        PgBindValue::Scalar(ScalarValue::Text("a".to_owned())),
        PgBindValue::Scalar(ScalarValue::Bytes(vec![1])),
    ];
    #[cfg(feature = "decimal")]
    binds.push(PgBindValue::Scalar(ScalarValue::Decimal(
        rust_decimal::Decimal::ONE,
    )));
    #[cfg(feature = "chrono")]
    {
        binds.push(PgBindValue::Scalar(ScalarValue::Date(
            chrono::NaiveDate::from_ymd_opt(2026, 10, 8).unwrap(),
        )));
        binds.push(PgBindValue::Scalar(ScalarValue::Time(
            chrono::NaiveTime::from_hms_opt(1, 2, 3).unwrap(),
        )));
        binds.push(PgBindValue::Scalar(ScalarValue::DateTime(
            chrono::NaiveDate::from_ymd_opt(2026, 10, 8)
                .unwrap()
                .and_hms_opt(1, 2, 3)
                .unwrap(),
        )));
        binds.push(PgBindValue::Scalar(ScalarValue::Timestamp(
            chrono::DateTime::parse_from_rfc3339("2026-10-08T01:02:03Z")
                .unwrap()
                .to_utc(),
        )));
    }
    #[cfg(feature = "uuid")]
    binds.push(PgBindValue::Scalar(ScalarValue::Uuid(uuid::Uuid::nil())));
    binds
}

fn one(value: ScalarValue, element_type: ValueType) -> PgBindValue {
    PgBindValue::Array(TypedList::new(element_type, vec![value]).unwrap())
}

#[test]
fn every_supported_scalar_type_binds() {
    assert!(build_arguments(&scalars()).is_ok());
    assert!(build_arguments(&[]).is_ok());
}

#[test]
fn every_supported_array_type_binds() {
    let mut arrays = vec![
        one(ScalarValue::Bool(true), ValueType::Bool),
        one(ScalarValue::I32(1), ValueType::I32),
        one(ScalarValue::I64(1), ValueType::I64),
        one(ScalarValue::F32(1.0), ValueType::F32),
        one(ScalarValue::F64(1.0), ValueType::F64),
        one(ScalarValue::Text("a".to_owned()), ValueType::Text),
    ];
    #[cfg(feature = "decimal")]
    arrays.push(one(
        ScalarValue::Decimal(rust_decimal::Decimal::ONE),
        ValueType::Decimal,
    ));
    #[cfg(feature = "chrono")]
    {
        arrays.push(one(
            ScalarValue::Date(chrono::NaiveDate::from_ymd_opt(2026, 10, 8).unwrap()),
            ValueType::Date,
        ));
        arrays.push(one(
            ScalarValue::Time(chrono::NaiveTime::from_hms_opt(1, 2, 3).unwrap()),
            ValueType::Time,
        ));
        arrays.push(one(
            ScalarValue::DateTime(
                chrono::NaiveDate::from_ymd_opt(2026, 10, 8)
                    .unwrap()
                    .and_hms_opt(1, 2, 3)
                    .unwrap(),
            ),
            ValueType::DateTime,
        ));
        arrays.push(one(
            ScalarValue::Timestamp(
                chrono::DateTime::parse_from_rfc3339("2026-10-08T01:02:03Z")
                    .unwrap()
                    .to_utc(),
            ),
            ValueType::Timestamp,
        ));
    }
    #[cfg(feature = "uuid")]
    arrays.push(one(ScalarValue::Uuid(uuid::Uuid::nil()), ValueType::Uuid));
    assert!(build_arguments(&arrays).is_ok());
}

#[test]
fn byte_and_json_arrays_are_rejected() {
    let bytes = PgBindValue::Array(
        TypedList::new(ValueType::Bytes, vec![ScalarValue::Bytes(vec![1])]).unwrap(),
    );
    assert!(matches!(
        build_arguments(&[bytes]),
        Err(PgSearchError::Bind(message)) if message.contains("bytea")
    ));

    #[cfg(feature = "json")]
    {
        let json = PgBindValue::Array(TypedList {
            element_type: ValueType::Json,
            values: vec![ScalarValue::Text("{}".to_owned())],
        });
        assert!(matches!(
            build_arguments(&[json]),
            Err(PgSearchError::Bind(message)) if message.contains("JSON")
        ));
    }
}

#[test]
fn a_heterogeneous_array_is_rejected() {
    // `TypedList` guards homogeneity, so the conversion guard is reached
    // with a list whose declared element type disagrees with its values.
    let list = TypedList {
        element_type: ValueType::I64,
        values: vec![ScalarValue::Text("a".to_owned())],
    };
    assert!(matches!(
        build_arguments(&[PgBindValue::Array(list)]),
        Err(PgSearchError::Bind(message)) if message.contains("heterogeneous")
    ));
}

#[test]
fn an_unsupported_scalar_variant_is_rejected() {
    // `RowValue`-only variants never appear in `ScalarValue`, so the guard
    // is exercised through a type that is not bindable as a scalar.
    let list = TypedList {
        element_type: ValueType::Bytes,
        values: vec![ScalarValue::Bytes(vec![1])],
    };
    assert!(build_arguments(&[PgBindValue::Array(list)]).is_err());
}
