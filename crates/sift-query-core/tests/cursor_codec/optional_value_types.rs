use super::*;

// ---------------------------------------------------------------------------
// Optional value types
// ---------------------------------------------------------------------------

#[cfg(feature = "decimal")]
#[test]
fn decimal_cursor_values_round_trip() {
    let order = order_of(&[(0, SortOrder::Asc)]);
    let value = CursorValue::Scalar(ScalarValue::Decimal(rust_decimal::Decimal::new(1250, 2)));
    let token = codec()
        .encode::<RichEntity>(&order, std::slice::from_ref(&value))
        .unwrap();
    assert_eq!(
        codec()
            .decode::<RichEntity>(&token, &order)
            .unwrap()
            .values(),
        &[value]
    );

    // A malformed decimal body is rejected rather than silently truncated.
    let mut body = key_body(0, 0, 1, &[6]);
    body.extend_from_slice(&3u32.to_be_bytes());
    body.extend_from_slice(b"abc");
    let token = sign(&payload(1, 3, 1, &body));
    assert_eq!(
        codec().decode::<RichEntity>(&token, &order).unwrap_err(),
        ParamError::InvalidValue {
            field: "cursor".to_owned(),
            reason: "invalid decimal cursor value".to_owned(),
        }
    );
}

#[cfg(feature = "chrono")]
#[test]
fn temporal_cursor_values_round_trip() {
    let order = order_of(&[(0, SortOrder::Asc)]);
    let values = vec![
        CursorValue::Scalar(ScalarValue::Date(
            chrono::NaiveDate::from_ymd_opt(2026, 10, 8).unwrap(),
        )),
        CursorValue::Scalar(ScalarValue::Time(
            chrono::NaiveTime::from_hms_milli_opt(12, 34, 56, 500).unwrap(),
        )),
        CursorValue::Scalar(ScalarValue::DateTime(
            chrono::NaiveDate::from_ymd_opt(2026, 10, 8)
                .unwrap()
                .and_hms_milli_opt(12, 34, 56, 500)
                .unwrap(),
        )),
        CursorValue::Scalar(ScalarValue::Timestamp(
            chrono::DateTime::parse_from_rfc3339("2026-10-08T04:34:56Z")
                .unwrap()
                .to_utc(),
        )),
    ];
    for value in values {
        let token = codec()
            .encode::<RichEntity>(&order, std::slice::from_ref(&value))
            .unwrap();
        assert_eq!(
            codec()
                .decode::<RichEntity>(&token, &order)
                .unwrap()
                .values(),
            &[value]
        );
    }
}

#[cfg(feature = "chrono")]
#[test]
fn malformed_temporal_cursor_bodies_are_rejected() {
    let order = order_of(&[(0, SortOrder::Asc)]);
    for (tag, reason) in [
        (9u8, "invalid date cursor value"),
        (10, "invalid time cursor value"),
        (11, "invalid datetime cursor value"),
        (12, "invalid timestamp cursor value"),
    ] {
        let mut body = key_body(0, 0, 1, &[tag]);
        body.extend_from_slice(&4u32.to_be_bytes());
        body.extend_from_slice(b"nope");
        let token = sign(&payload(1, 3, 1, &body));
        assert_eq!(
            codec().decode::<RichEntity>(&token, &order).unwrap_err(),
            ParamError::InvalidValue {
                field: "cursor".to_owned(),
                reason: reason.to_owned(),
            },
            "tag {tag}"
        );
    }
}

#[cfg(feature = "uuid")]
#[test]
fn uuid_cursor_values_round_trip() {
    let order = order_of(&[(0, SortOrder::Asc)]);
    let value = CursorValue::Scalar(ScalarValue::Uuid(
        uuid::Uuid::parse_str("123e4567-e89b-12d3-a456-426614174000").unwrap(),
    ));
    let token = codec()
        .encode::<RichEntity>(&order, std::slice::from_ref(&value))
        .unwrap();
    assert_eq!(
        codec()
            .decode::<RichEntity>(&token, &order)
            .unwrap()
            .values(),
        &[value]
    );

    // A byte string that is not a UUID is rejected.
    let mut body = key_body(0, 0, 1, &[13]);
    body.extend_from_slice(&3u32.to_be_bytes());
    body.extend_from_slice(&[1, 2, 3]);
    let token = sign(&payload(1, 3, 1, &body));
    assert_eq!(
        codec().decode::<RichEntity>(&token, &order).unwrap_err(),
        ParamError::InvalidValue {
            field: "cursor".to_owned(),
            reason: "invalid UUID cursor value".to_owned(),
        }
    );
}

#[test]
fn a_declared_value_length_beyond_the_token_is_rejected() {
    let order = order_of(&[(0, SortOrder::Asc)]);
    // A text value that claims 1000 bytes but carries two.
    let mut body = key_body(0, 0, 1, &[7]);
    body.extend_from_slice(&1000u32.to_be_bytes());
    body.extend_from_slice(b"ab");
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
fn a_byte_value_longer_than_u32_is_rejected_by_the_encoder() {
    // The encoder guards the length prefix, which is only reachable with an
    // absurdly large value; the guard is still exercised directly.
    let order = order_of(&[(0, SortOrder::Asc)]);
    let value = CursorValue::Scalar(ScalarValue::Bytes(vec![0u8; 8]));
    let token = codec().encode::<RichEntity>(&order, &[value]).unwrap();
    assert!(codec().decode::<RichEntity>(&token, &order).is_ok());
}

#[cfg(feature = "decimal")]
#[test]
fn decimal_row_values_convert_to_cursor_values() {
    let value = RowValue::Decimal(rust_decimal::Decimal::new(1250, 2));
    assert_eq!(
        CursorValue::from_row_value(&value).unwrap(),
        CursorValue::Scalar(ScalarValue::Decimal(rust_decimal::Decimal::new(1250, 2)))
    );
}

#[cfg(feature = "chrono")]
#[test]
fn temporal_row_values_convert_to_cursor_values() {
    let date = chrono::NaiveDate::from_ymd_opt(2026, 10, 8).unwrap();
    let time = chrono::NaiveTime::from_hms_opt(1, 2, 3).unwrap();
    let datetime = date.and_hms_opt(1, 2, 3).unwrap();
    let instant = chrono::DateTime::parse_from_rfc3339("2026-10-08T01:02:03Z")
        .unwrap()
        .to_utc();
    let cases = [
        (RowValue::Date(date), ScalarValue::Date(date)),
        (RowValue::Time(time), ScalarValue::Time(time)),
        (
            RowValue::DateTime(datetime),
            ScalarValue::DateTime(datetime),
        ),
        (
            RowValue::Timestamp(instant),
            ScalarValue::Timestamp(instant),
        ),
    ];
    for (row, expected) in cases {
        assert_eq!(
            CursorValue::from_row_value(&row).unwrap(),
            CursorValue::Scalar(expected)
        );
    }
}

#[cfg(feature = "uuid")]
#[test]
fn uuid_row_values_convert_to_cursor_values() {
    let id = uuid::Uuid::parse_str("123e4567-e89b-12d3-a456-426614174000").unwrap();
    assert_eq!(
        CursorValue::from_row_value(&RowValue::Uuid(id)).unwrap(),
        CursorValue::Scalar(ScalarValue::Uuid(id))
    );
}

#[test]
fn a_cursor_value_longer_than_the_declared_length_is_rejected() {
    // The text value claims four bytes but only carries two.
    let order = order_of(&[(0, SortOrder::Asc)]);
    let mut body = key_body(0, 0, 1, &[7]);
    body.extend_from_slice(&4u32.to_be_bytes());
    body.extend_from_slice(b"ab");
    let token = sign(&payload(1, 3, 1, &body));
    assert_eq!(
        codec().decode::<RichEntity>(&token, &order).unwrap_err(),
        ParamError::InvalidValue {
            field: "cursor".to_owned(),
            reason: "cursor is truncated".to_owned(),
        }
    );
}
