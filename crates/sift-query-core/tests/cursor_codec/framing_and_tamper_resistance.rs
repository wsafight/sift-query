use super::*;

// ---------------------------------------------------------------------------
// Framing, signing, and tamper resistance
// ---------------------------------------------------------------------------

#[test]
fn short_signing_keys_are_rejected() {
    assert_eq!(
        CursorCodec::new([0u8; 31]).unwrap_err(),
        ConfigError::Invalid("cursor signing key must contain at least 32 bytes".to_owned())
    );
    assert!(CursorCodec::new([0u8; 32]).is_ok());
}

#[test]
fn codec_debug_output_never_leaks_the_key() {
    let rendered = format!("{:?}", codec());
    assert!(rendered.contains("REDACTED"));
    assert!(!rendered.contains("90"));
}

#[test]
fn encode_rejects_arity_mismatch_and_oversized_key_counts() {
    let order = order_of(&[(0, SortOrder::Asc), (1, SortOrder::Asc)]);
    assert_eq!(order.len(), 2);

    // Too few values for the ordering.
    assert_eq!(
        codec()
            .encode::<RichEntity>(&order, &[CursorValue::Scalar(ScalarValue::I64(1))])
            .unwrap_err(),
        ParamError::InvalidValue {
            field: "cursor".to_owned(),
            reason: "cursor value count does not match ordering".to_owned(),
        }
    );

    // A single-byte key count cannot describe more than 255 ordering keys.
    let wide_order = vec![order[0]; 256];
    let wide_values = vec![CursorValue::Scalar(ScalarValue::I64(1)); 256];
    assert_eq!(
        codec()
            .encode::<RichEntity>(&wide_order, &wide_values)
            .unwrap_err(),
        ParamError::InvalidValue {
            field: "cursor".to_owned(),
            reason: "cursor value count does not match ordering".to_owned(),
        }
    );
}

#[test]
fn round_trips_every_scalar_value_kind() {
    let values = vec![
        CursorValue::Null,
        CursorValue::Scalar(ScalarValue::Bool(true)),
        CursorValue::Scalar(ScalarValue::I32(-7)),
        CursorValue::Scalar(ScalarValue::I64(i64::MIN)),
        CursorValue::Scalar(ScalarValue::F32(-1.5)),
        CursorValue::Scalar(ScalarValue::F64(f64::MAX)),
        CursorValue::Scalar(ScalarValue::Text("héllo ✓".to_owned())),
        CursorValue::Scalar(ScalarValue::Bytes(vec![0x00, 0xff, 0x7f])),
    ];
    let single = order_of(&[(0, SortOrder::Asc)])[0];
    let order = vec![single; values.len()];

    let token = codec().encode::<RichEntity>(&order, &values).unwrap();
    let decoded = codec().decode::<RichEntity>(&token, &order).unwrap();
    assert_eq!(decoded.values(), values.as_slice());
}

#[test]
fn decode_rejects_a_non_base64_token() {
    let order = order_of(&[(0, SortOrder::Asc)]);
    assert_eq!(
        codec()
            .decode::<RichEntity>("not a cursor!!", &order)
            .unwrap_err(),
        ParamError::InvalidValue {
            field: "cursor".to_owned(),
            reason: "cursor is not valid base64url".to_owned(),
        }
    );
}

#[test]
fn decode_rejects_a_truncated_token() {
    let order = order_of(&[(0, SortOrder::Asc)]);
    let short = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode([0u8; 32]);
    assert_eq!(
        codec().decode::<RichEntity>(&short, &order).unwrap_err(),
        ParamError::InvalidValue {
            field: "cursor".to_owned(),
            reason: "cursor is truncated".to_owned(),
        }
    );
}

#[test]
fn decode_rejects_a_forged_signature() {
    let order = order_of(&[(0, SortOrder::Asc)]);
    let body = key_body(0, 0, 1, &[3, 0, 0, 0, 0, 0, 0, 0, 1]);
    let token = sign(&payload(1, 3, 1, &body));
    let mut bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(&token)
        .unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 0xff;
    let tampered = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes);
    assert_eq!(
        codec().decode::<RichEntity>(&tampered, &order).unwrap_err(),
        ParamError::InvalidValue {
            field: "cursor".to_owned(),
            reason: "cursor signature is invalid".to_owned(),
        }
    );
}

#[test]
fn decode_rejects_an_unsupported_version() {
    let order = order_of(&[(0, SortOrder::Asc)]);
    let body = key_body(0, 0, 1, &[3, 0, 0, 0, 0, 0, 0, 0, 1]);
    let token = sign(&payload(9, 3, 1, &body));
    assert_eq!(
        codec().decode::<RichEntity>(&token, &order).unwrap_err(),
        ParamError::InvalidValue {
            field: "cursor".to_owned(),
            reason: "unsupported cursor version".to_owned(),
        }
    );
}

#[test]
fn decode_rejects_a_token_from_another_schema() {
    let order = order_of(&[(0, SortOrder::Asc)]);
    let body = key_body(0, 0, 1, &[3, 0, 0, 0, 0, 0, 0, 0, 1]);
    let token = sign(&payload(1, 999, 1, &body));
    assert_eq!(
        codec().decode::<RichEntity>(&token, &order).unwrap_err(),
        ParamError::InvalidValue {
            field: "cursor".to_owned(),
            reason: "cursor belongs to another entity schema".to_owned(),
        }
    );
}

#[test]
fn decode_rejects_a_changed_ordering_length() {
    let order = order_of(&[(0, SortOrder::Asc)]);
    let body = key_body(0, 0, 1, &[3, 0, 0, 0, 0, 0, 0, 0, 1]);
    let token = sign(&payload(1, 3, 7, &body));
    assert_eq!(
        codec().decode::<RichEntity>(&token, &order).unwrap_err(),
        ParamError::InvalidValue {
            field: "cursor".to_owned(),
            reason: "cursor ordering length changed".to_owned(),
        }
    );
}

#[test]
fn decode_rejects_a_changed_ordering_item() {
    let order = order_of(&[(0, SortOrder::Asc)]);
    let body = key_body(1, 0, 1, &[3, 0, 0, 0, 0, 0, 0, 0, 1]);
    let token = sign(&payload(1, 3, 1, &body));
    assert_eq!(
        codec().decode::<RichEntity>(&token, &order).unwrap_err(),
        ParamError::InvalidValue {
            field: "cursor".to_owned(),
            reason: "cursor ordering changed".to_owned(),
        }
    );

    // Same field, but the direction flipped.
    let body = key_body(0, 1, 1, &[3, 0, 0, 0, 0, 0, 0, 0, 1]);
    let token = sign(&payload(1, 3, 1, &body));
    assert!(codec().decode::<RichEntity>(&token, &order).is_err());

    // Same field and direction, but the null ordering flipped.
    let body = key_body(0, 0, 0, &[3, 0, 0, 0, 0, 0, 0, 0, 1]);
    let token = sign(&payload(1, 3, 1, &body));
    assert!(codec().decode::<RichEntity>(&token, &order).is_err());
}

#[test]
fn decode_rejects_trailing_data() {
    let order = order_of(&[(0, SortOrder::Asc)]);
    let mut body = key_body(0, 0, 1, &[3, 0, 0, 0, 0, 0, 0, 0, 1]);
    body.push(0x00);
    let token = sign(&payload(1, 3, 1, &body));
    assert_eq!(
        codec().decode::<RichEntity>(&token, &order).unwrap_err(),
        ParamError::InvalidValue {
            field: "cursor".to_owned(),
            reason: "cursor contains trailing data".to_owned(),
        }
    );
}

#[test]
fn decode_rejects_an_unknown_value_tag() {
    let order = order_of(&[(0, SortOrder::Asc)]);
    let body = key_body(0, 0, 1, &[0x7f]);
    let token = sign(&payload(1, 3, 1, &body));
    assert_eq!(
        codec().decode::<RichEntity>(&token, &order).unwrap_err(),
        ParamError::InvalidValue {
            field: "cursor".to_owned(),
            reason: "unknown cursor value type".to_owned(),
        }
    );
}

#[test]
fn decode_rejects_a_non_utf8_text_value() {
    let order = order_of(&[(0, SortOrder::Asc)]);
    let mut body = key_body(0, 0, 1, &[7]);
    body.extend_from_slice(&2u32.to_be_bytes());
    body.extend_from_slice(&[0xff, 0xfe]);
    let token = sign(&payload(1, 3, 1, &body));
    assert_eq!(
        codec().decode::<RichEntity>(&token, &order).unwrap_err(),
        ParamError::InvalidValue {
            field: "cursor".to_owned(),
            reason: "cursor value is not UTF-8".to_owned(),
        }
    );
}

#[test]
fn decode_rejects_a_truncated_value_body() {
    let order = order_of(&[(0, SortOrder::Asc)]);
    // Tagged as an i64 but carrying only three bytes.
    let body = key_body(0, 0, 1, &[3, 0, 0, 0]);
    let token = sign(&payload(1, 3, 1, &body));
    assert_eq!(
        codec().decode::<RichEntity>(&token, &order).unwrap_err(),
        ParamError::InvalidValue {
            field: "cursor".to_owned(),
            reason: "cursor is truncated".to_owned(),
        }
    );
}

#[test]
fn decode_rejects_a_missing_value_tag() {
    let order = order_of(&[(0, SortOrder::Asc)]);
    let body = key_body(0, 0, 1, &[]);
    let token = sign(&payload(1, 3, 1, &body));
    assert_eq!(
        codec().decode::<RichEntity>(&token, &order).unwrap_err(),
        ParamError::InvalidValue {
            field: "cursor".to_owned(),
            reason: "cursor is truncated".to_owned(),
        }
    );
}

#[test]
fn null_values_are_encoded_as_a_single_tag() {
    let order = order_of(&[(0, SortOrder::Asc), (1, SortOrder::Asc)]);
    let token = codec()
        .encode::<RichEntity>(&order, &[CursorValue::Null, CursorValue::Null])
        .unwrap();
    let decoded = codec().decode::<RichEntity>(&token, &order).unwrap();
    assert_eq!(decoded.values(), &[CursorValue::Null, CursorValue::Null]);
}

#[test]
fn nullable_values_are_supported_by_the_cursor_value_trait() {
    use sift_query_core::IntoCursorValue;
    assert_eq!(
        Some(5_i64).to_cursor_value(),
        CursorValue::Scalar(ScalarValue::I64(5))
    );
    assert_eq!(None::<i64>.to_cursor_value(), CursorValue::Null);
    assert_eq!(
        true.to_cursor_value(),
        CursorValue::Scalar(ScalarValue::Bool(true))
    );
    assert_eq!(
        "x".to_owned().to_cursor_value(),
        CursorValue::Scalar(ScalarValue::Text("x".to_owned()))
    );
    assert_eq!(
        vec![1_u8].to_cursor_value(),
        CursorValue::Scalar(ScalarValue::Bytes(vec![1]))
    );
}

#[test]
fn row_values_convert_to_cursor_values() {
    let cases = [
        (RowValue::Null, CursorValue::Null),
        (
            RowValue::Bool(false),
            CursorValue::Scalar(ScalarValue::Bool(false)),
        ),
        (RowValue::I32(1), CursorValue::Scalar(ScalarValue::I32(1))),
        (RowValue::I64(2), CursorValue::Scalar(ScalarValue::I64(2))),
        (
            RowValue::F32(1.5),
            CursorValue::Scalar(ScalarValue::F32(1.5)),
        ),
        (
            RowValue::F64(2.5),
            CursorValue::Scalar(ScalarValue::F64(2.5)),
        ),
        (
            RowValue::Text("a".to_owned()),
            CursorValue::Scalar(ScalarValue::Text("a".to_owned())),
        ),
        (
            RowValue::Bytes(vec![9]),
            CursorValue::Scalar(ScalarValue::Bytes(vec![9])),
        ),
    ];
    for (row, expected) in cases {
        assert_eq!(CursorValue::from_row_value(&row).unwrap(), expected);
    }
}

#[cfg(feature = "json")]
#[test]
fn json_row_values_cannot_become_cursor_keys() {
    assert_eq!(
        CursorValue::from_row_value(&RowValue::Json(serde_json::json!({"a": 1}))).unwrap_err(),
        ParamError::InvalidValue {
            field: "cursor".to_owned(),
            reason: "JSON cannot be a cursor key".to_owned(),
        }
    );
}

#[test]
fn default_entity_has_no_cursor_value() {
    assert_eq!(TestEntity.cursor_value(FieldId(0)), None);
    assert_eq!(CursorEntity.cursor_value(FieldId(0)), None);
}

#[test]
fn null_ordering_follows_the_sort_direction() {
    assert_eq!(NullOrder::from(SortOrder::Asc), NullOrder::Last);
    assert_eq!(NullOrder::from(SortOrder::Desc), NullOrder::First);
    assert_eq!(SortOrder::parse("asc"), Some(SortOrder::Asc));
    assert_eq!(SortOrder::parse("desc"), Some(SortOrder::Desc));
    assert_eq!(SortOrder::parse("ASC"), None);
    assert_eq!(SortOrder::parse(""), None);
}

#[test]
fn order_meta_converts_to_order_by_with_the_matching_null_order() {
    let meta = sift_query_core::OrderMeta {
        field: FieldId(4),
        direction: SortOrder::Desc,
    };
    let order: OrderBy = meta.into();
    assert_eq!(order.field, FieldId(4));
    assert_eq!(order.direction, SortOrder::Desc);
    assert_eq!(order.nulls, NullOrder::First);
}
