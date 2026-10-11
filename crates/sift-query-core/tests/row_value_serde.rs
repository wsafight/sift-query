#![cfg(feature = "serde")]

use sift_query_core::RowValue;

#[test]
fn i64_is_serialized_without_javascript_precision_loss() {
    let value = RowValue::I64(9_007_199_254_740_993);
    assert_eq!(
        serde_json::to_value(value).unwrap(),
        serde_json::json!("9007199254740993")
    );
}

#[test]
fn bytes_use_unpadded_base64url() {
    let value = RowValue::Bytes(vec![0xfb, 0xff]);
    assert_eq!(
        serde_json::to_value(value).unwrap(),
        serde_json::json!("-_8")
    );
}

#[test]
fn non_finite_float_is_rejected() {
    assert!(serde_json::to_string(&RowValue::F64(f64::NAN)).is_err());
}

#[cfg(feature = "decimal")]
#[test]
fn decimal_is_serialized_as_a_string() {
    let value = RowValue::Decimal(rust_decimal::Decimal::new(1250, 2));
    assert_eq!(
        serde_json::to_value(value).unwrap(),
        serde_json::json!("12.50")
    );
}

#[cfg(feature = "uuid")]
#[test]
fn uuid_is_serialized_as_a_string() {
    let value =
        RowValue::Uuid(uuid::Uuid::parse_str("123e4567-e89b-12d3-a456-426614174000").unwrap());
    assert_eq!(
        serde_json::to_value(value).unwrap(),
        serde_json::json!("123e4567-e89b-12d3-a456-426614174000")
    );
}

#[cfg(feature = "json")]
#[test]
fn json_keeps_its_native_shape() {
    let value = RowValue::Json(serde_json::json!({"ok": true}));
    assert_eq!(
        serde_json::to_value(value).unwrap(),
        serde_json::json!({"ok": true})
    );
}
