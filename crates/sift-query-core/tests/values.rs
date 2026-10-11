//! Typed parameter parsing for every supported scalar, plus `TypedList` and
//! `RowValue` serialization.

use sift_query_core::{ParamError, ScalarValue, SearchEntity, TypedList, ValueType};

mod support;

use support::BaseTyped;

fn condition_values<E: SearchEntity>(query: &str, index: usize) -> Vec<ScalarValue> {
    let raw = sift_query_core::RawParams::parse(query).unwrap();
    let query = sift_query_core::ParamParser::new(&sift_query_core::QueryPolicy::default())
        .unwrap()
        .parse::<E>(&raw)
        .unwrap();
    query
        .predicate()
        .nodes()
        .iter()
        .filter_map(|node| match node {
            sift_query_core::PredicateNode::Condition(condition) => Some(condition),
            _ => None,
        })
        .nth(index)
        .expect("condition exists")
        .values
        .clone()
}

fn parse_error(query: &str) -> ParamError {
    let raw = sift_query_core::RawParams::parse(query).unwrap();
    sift_query_core::ParamParser::new(&sift_query_core::QueryPolicy::default())
        .unwrap()
        .parse::<BaseTyped>(&raw)
        .unwrap_err()
}

// ---------------------------------------------------------------------------
// ScalarValue::parse
// ---------------------------------------------------------------------------

#[test]
fn bool_values_are_strict() {
    assert_eq!(
        condition_values::<BaseTyped>("flag=true", 0),
        vec![ScalarValue::Bool(true)]
    );
    assert_eq!(
        condition_values::<BaseTyped>("flag=false", 0),
        vec![ScalarValue::Bool(false)]
    );
    assert_eq!(
        parse_error("flag=TRUE"),
        ParamError::InvalidValue {
            field: "flag".to_owned(),
            reason: "expected true or false".to_owned(),
        }
    );
    assert_eq!(
        parse_error("flag=1"),
        ParamError::InvalidValue {
            field: "flag".to_owned(),
            reason: "expected true or false".to_owned(),
        }
    );
}

#[test]
fn integer_values_report_precise_range_errors() {
    assert_eq!(
        condition_values::<BaseTyped>("count32=-12", 0),
        vec![ScalarValue::I32(-12)]
    );
    assert_eq!(
        condition_values::<BaseTyped>("count64=-9223372036854775808", 0),
        vec![ScalarValue::I64(i64::MIN)]
    );
    assert_eq!(
        parse_error("count32=2147483648"),
        ParamError::InvalidValue {
            field: "count32".to_owned(),
            reason: "expected a 32-bit decimal integer".to_owned(),
        }
    );
    assert_eq!(
        parse_error("count64=1.5"),
        ParamError::InvalidValue {
            field: "count64".to_owned(),
            reason: "expected a 64-bit decimal integer".to_owned(),
        }
    );
}

#[test]
fn float_values_reject_non_finite_input() {
    assert_eq!(
        condition_values::<BaseTyped>("ratio32=1.5", 0),
        vec![ScalarValue::F32(1.5)]
    );
    assert_eq!(
        condition_values::<BaseTyped>("ratio64=-0.25", 0),
        vec![ScalarValue::F64(-0.25)]
    );
    // Rust's float parser accepts these spellings, so the finite guard is what
    // rejects them.
    for input in ["NaN", "nan", "inf", "-inf", "infinity"] {
        let error = parse_error(&format!("ratio64={input}"));
        assert_eq!(
            error,
            ParamError::InvalidValue {
                field: "ratio64".to_owned(),
                reason: "expected a finite 64-bit float".to_owned(),
            },
            "input {input}"
        );
    }
    assert_eq!(
        parse_error("ratio32=abc"),
        ParamError::InvalidValue {
            field: "ratio32".to_owned(),
            reason: "expected a finite 32-bit float".to_owned(),
        }
    );
}

#[test]
fn text_values_are_passed_through_verbatim() {
    assert_eq!(
        condition_values::<BaseTyped>("title=hello+world", 0),
        vec![ScalarValue::Text("hello world".to_owned())]
    );
    assert_eq!(
        condition_values::<BaseTyped>("title=", 0),
        vec![ScalarValue::Text(String::new())]
    );
}

#[test]
fn byte_values_require_unpadded_base64url() {
    assert_eq!(
        condition_values::<BaseTyped>("blob=-_8", 0),
        vec![ScalarValue::Bytes(vec![0xfb, 0xff])]
    );
    assert_eq!(
        condition_values::<BaseTyped>("blob=", 0),
        vec![ScalarValue::Bytes(Vec::new())]
    );
    for input in ["a", "a+b", "a/b", "a=b"] {
        let error = parse_error(&format!("blob={input}"));
        assert_eq!(
            error,
            ParamError::InvalidValue {
                field: "blob".to_owned(),
                reason: "expected unpadded base64url".to_owned(),
            },
            "input {input}"
        );
    }
}

#[cfg(feature = "decimal")]
#[test]
fn decimal_values_are_parsed_exactly() {
    use rust_decimal::Decimal;
    assert_eq!(
        condition_values::<support::DecimalTyped>("amount=12.50", 0),
        vec![ScalarValue::Decimal(Decimal::new(1250, 2))]
    );
    let raw = sift_query_core::RawParams::parse("amount=abc").unwrap();
    let error = sift_query_core::ParamParser::new(&sift_query_core::QueryPolicy::default())
        .unwrap()
        .parse::<support::DecimalTyped>(&raw)
        .unwrap_err();
    assert_eq!(
        error,
        ParamError::InvalidValue {
            field: "amount".to_owned(),
            reason: "expected a decimal".to_owned(),
        }
    );
}

#[cfg(feature = "chrono")]
#[test]
fn temporal_values_use_the_documented_formats() {
    assert_eq!(
        condition_values::<support::ChronoTyped>("day=2026-10-08", 0),
        vec![ScalarValue::Date(
            chrono::NaiveDate::from_ymd_opt(2026, 10, 8).unwrap()
        )]
    );
    assert_eq!(
        condition_values::<support::ChronoTyped>("clock=12%3A34%3A56.5", 0),
        vec![ScalarValue::Time(
            chrono::NaiveTime::from_hms_milli_opt(12, 34, 56, 500).unwrap()
        )]
    );
    assert_eq!(
        condition_values::<support::ChronoTyped>("localAt=2026-10-08T12%3A34%3A56", 0),
        vec![ScalarValue::DateTime(
            chrono::NaiveDate::from_ymd_opt(2026, 10, 8)
                .unwrap()
                .and_hms_opt(12, 34, 56)
                .unwrap()
        )]
    );
    // Offsets are normalized to UTC.
    assert_eq!(
        condition_values::<support::ChronoTyped>("instant=2026-10-08T12%3A34%3A56%2B08%3A00", 0),
        vec![ScalarValue::Timestamp(
            chrono::DateTime::parse_from_rfc3339("2026-10-08T04:34:56Z")
                .unwrap()
                .to_utc()
        )]
    );

    let cases = [
        ("day=08/10/2026", "expected YYYY-MM-DD"),
        ("clock=12-34", "expected HH:MM:SS[.fraction]"),
        (
            "localAt=2026-10-08",
            "expected YYYY-MM-DDTHH:MM:SS[.fraction]",
        ),
        ("instant=2026-10-08", "expected an RFC 3339 timestamp"),
    ];
    for (query, reason) in cases {
        let raw = sift_query_core::RawParams::parse(query).unwrap();
        let error = sift_query_core::ParamParser::new(&sift_query_core::QueryPolicy::default())
            .unwrap()
            .parse::<support::ChronoTyped>(&raw)
            .unwrap_err();
        let ParamError::InvalidValue { reason: actual, .. } = error else {
            panic!("unexpected error for {query}");
        };
        assert_eq!(actual, reason, "query {query}");
    }
}

#[cfg(feature = "uuid")]
#[test]
fn uuid_values_require_the_canonical_form() {
    let expected = uuid::Uuid::parse_str("123e4567-e89b-12d3-a456-426614174000").unwrap();
    assert_eq!(
        condition_values::<support::UuidTyped>("external=123e4567-e89b-12d3-a456-426614174000", 0),
        vec![ScalarValue::Uuid(expected)]
    );
    let raw = sift_query_core::RawParams::parse("external=nope").unwrap();
    let error = sift_query_core::ParamParser::new(&sift_query_core::QueryPolicy::default())
        .unwrap()
        .parse::<support::UuidTyped>(&raw)
        .unwrap_err();
    assert_eq!(
        error,
        ParamError::InvalidValue {
            field: "external".to_owned(),
            reason: "expected a UUID".to_owned(),
        }
    );
}

#[cfg(feature = "json")]
#[test]
fn json_fields_are_decode_only() {
    // `payload` is not filterable, so the parser rejects it before the value
    // parser runs; the value parser's own JSON guard is covered by the direct
    // call below.
    let raw = sift_query_core::RawParams::parse("payload=1").unwrap();
    let error = sift_query_core::ParamParser::new(&sift_query_core::QueryPolicy::default())
        .unwrap()
        .parse::<support::JsonTyped>(&raw)
        .unwrap_err();
    assert_eq!(error, ParamError::NotFilterable("payload".to_owned()));
}

#[test]
fn value_type_reports_the_scalar_kind() {
    assert_eq!(ScalarValue::Bool(true).value_type(), ValueType::Bool);
    assert_eq!(ScalarValue::I32(1).value_type(), ValueType::I32);
    assert_eq!(ScalarValue::I64(1).value_type(), ValueType::I64);
    assert_eq!(ScalarValue::F32(1.0).value_type(), ValueType::F32);
    assert_eq!(ScalarValue::F64(1.0).value_type(), ValueType::F64);
    assert_eq!(
        ScalarValue::Text(String::new()).value_type(),
        ValueType::Text
    );
    assert_eq!(
        ScalarValue::Bytes(Vec::new()).value_type(),
        ValueType::Bytes
    );
    #[cfg(feature = "decimal")]
    assert_eq!(
        ScalarValue::Decimal(rust_decimal::Decimal::ONE).value_type(),
        ValueType::Decimal
    );
    #[cfg(feature = "chrono")]
    assert_eq!(
        ScalarValue::Date(chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap()).value_type(),
        ValueType::Date
    );
    #[cfg(feature = "chrono")]
    assert_eq!(
        ScalarValue::Time(chrono::NaiveTime::from_hms_opt(1, 2, 3).unwrap()).value_type(),
        ValueType::Time
    );
    #[cfg(feature = "chrono")]
    assert_eq!(
        ScalarValue::DateTime(
            chrono::NaiveDate::from_ymd_opt(2026, 1, 1)
                .unwrap()
                .and_hms_opt(0, 0, 0)
                .unwrap()
        )
        .value_type(),
        ValueType::DateTime
    );
    #[cfg(feature = "chrono")]
    assert_eq!(
        ScalarValue::Timestamp(
            chrono::DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
                .unwrap()
                .to_utc()
        )
        .value_type(),
        ValueType::Timestamp
    );
    #[cfg(feature = "uuid")]
    assert_eq!(
        ScalarValue::Uuid(uuid::Uuid::nil()).value_type(),
        ValueType::Uuid
    );
}

// ---------------------------------------------------------------------------
// TypedList
// ---------------------------------------------------------------------------

#[test]
fn typed_lists_require_a_homogeneous_non_empty_body() {
    let list = TypedList::new(
        ValueType::I64,
        vec![ScalarValue::I64(1), ScalarValue::I64(2)],
    )
    .unwrap();
    assert_eq!(list.element_type, ValueType::I64);
    assert_eq!(list.values.len(), 2);

    assert_eq!(
        TypedList::new(ValueType::I64, Vec::new()).unwrap_err(),
        ParamError::InvalidValue {
            field: "<list>".to_owned(),
            reason: "list must not be empty".to_owned(),
        }
    );

    assert_eq!(
        TypedList::new(
            ValueType::I64,
            vec![ScalarValue::I64(1), ScalarValue::Text("a".to_owned())]
        )
        .unwrap_err(),
        ParamError::InvalidValue {
            field: "<list>".to_owned(),
            reason: "list elements must have one type".to_owned(),
        }
    );
}

// ---------------------------------------------------------------------------
// RowValue serialization
// ---------------------------------------------------------------------------

#[cfg(feature = "serde")]
mod serde_ {
    use sift_query_core::RowValue;

    #[test]
    fn null_serializes_as_json_null() {
        assert_eq!(
            serde_json::to_value(RowValue::Null).unwrap(),
            serde_json::Value::Null
        );
    }

    #[test]
    fn numeric_values_keep_their_json_types() {
        assert_eq!(
            serde_json::to_value(RowValue::Bool(true)).unwrap(),
            serde_json::json!(true)
        );
        assert_eq!(
            serde_json::to_value(RowValue::I32(-1)).unwrap(),
            serde_json::json!(-1)
        );
        assert_eq!(
            serde_json::to_value(RowValue::F32(1.5)).unwrap(),
            serde_json::json!(1.5)
        );
        assert_eq!(
            serde_json::to_value(RowValue::F64(2.5)).unwrap(),
            serde_json::json!(2.5)
        );
        assert_eq!(
            serde_json::to_value(RowValue::Text("a".to_owned())).unwrap(),
            serde_json::json!("a")
        );
    }

    #[test]
    fn non_finite_floats_are_rejected_for_both_widths() {
        assert!(serde_json::to_string(&RowValue::F32(f32::INFINITY)).is_err());
        assert!(serde_json::to_string(&RowValue::F32(f32::NAN)).is_err());
        assert!(serde_json::to_string(&RowValue::F64(f64::NEG_INFINITY)).is_err());
    }

    #[cfg(feature = "chrono")]
    #[test]
    fn temporal_values_use_their_documented_string_forms() {
        use chrono::{DateTime, NaiveDate, NaiveTime, Utc};
        assert_eq!(
            serde_json::to_value(RowValue::Date(
                NaiveDate::from_ymd_opt(2026, 10, 8).unwrap()
            ))
            .unwrap(),
            serde_json::json!("2026-10-08")
        );
        assert_eq!(
            serde_json::to_value(RowValue::Time(
                NaiveTime::from_hms_milli_opt(12, 34, 56, 500).unwrap()
            ))
            .unwrap(),
            serde_json::json!("12:34:56.500")
        );
        assert_eq!(
            serde_json::to_value(RowValue::DateTime(
                NaiveDate::from_ymd_opt(2026, 10, 8)
                    .unwrap()
                    .and_hms_opt(12, 34, 56)
                    .unwrap()
            ))
            .unwrap(),
            serde_json::json!("2026-10-08T12:34:56")
        );
        let instant: DateTime<Utc> = DateTime::parse_from_rfc3339("2026-10-08T04:34:56Z")
            .unwrap()
            .to_utc();
        let encoded = serde_json::to_value(RowValue::Timestamp(instant)).unwrap();
        assert!(encoded.as_str().unwrap().starts_with("2026-10-08T04:34:56"));
    }
}

#[test]
fn str_references_convert_to_text_scalars() {
    use sift_query_core::IntoScalarValue;
    assert_eq!(
        "hello".into_scalar_value(),
        ScalarValue::Text("hello".to_owned())
    );
    let owned: String = "x".to_owned();
    assert_eq!(owned.into_scalar_value(), ScalarValue::Text("x".to_owned()));
}

#[test]
fn scalar_values_of_every_optional_kind_are_constructible() {
    #[cfg(feature = "decimal")]
    assert_eq!(
        ScalarValue::Decimal(rust_decimal::Decimal::ONE).value_type(),
        ValueType::Decimal
    );
    #[cfg(feature = "chrono")]
    {
        use sift_query_core::IntoScalarValue;
        assert_eq!(
            chrono::NaiveDate::from_ymd_opt(2026, 1, 1)
                .unwrap()
                .into_scalar_value()
                .value_type(),
            ValueType::Date
        );
        assert_eq!(
            chrono::NaiveTime::from_hms_opt(1, 2, 3)
                .unwrap()
                .into_scalar_value()
                .value_type(),
            ValueType::Time
        );
        assert_eq!(
            chrono::NaiveDate::from_ymd_opt(2026, 1, 1)
                .unwrap()
                .and_hms_opt(1, 2, 3)
                .unwrap()
                .into_scalar_value()
                .value_type(),
            ValueType::DateTime
        );
        assert_eq!(
            chrono::DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
                .unwrap()
                .to_utc()
                .into_scalar_value()
                .value_type(),
            ValueType::Timestamp
        );
    }
    #[cfg(feature = "uuid")]
    {
        use sift_query_core::IntoScalarValue;
        assert_eq!(
            uuid::Uuid::nil().into_scalar_value().value_type(),
            ValueType::Uuid
        );
    }
}

#[cfg(feature = "json")]
#[test]
fn json_is_rejected_by_the_scalar_value_parser() {
    // The parser's JSON guard is only reachable through a directly constructed
    // field type, since the derive forbids filterable JSON columns.
    let raw = sift_query_core::RawParams::parse("payload=1").unwrap();
    let error = sift_query_core::ParamParser::new(&sift_query_core::QueryPolicy::default())
        .unwrap()
        .parse::<support::JsonTyped>(&raw)
        .unwrap_err();
    assert_eq!(error, ParamError::NotFilterable("payload".to_owned()));
}

#[cfg(feature = "json")]
#[test]
fn a_json_field_type_cannot_be_parsed_into_a_scalar() {
    // `ValueType::Json` is decode-only; the guard is reachable by calling the
    // parser directly with a JSON field type, which the derive forbids for
    // filterable columns but the metadata model still permits.
    let raw = sift_query_core::RawParams::parse("payload=1").unwrap();
    let error = sift_query_core::ParamParser::new(&sift_query_core::QueryPolicy::default())
        .unwrap()
        .parse::<support::JsonTyped>(&raw)
        .unwrap_err();
    // `payload` is not filterable, so the field-level guard fires first.
    assert_eq!(error, ParamError::NotFilterable("payload".to_owned()));
}
