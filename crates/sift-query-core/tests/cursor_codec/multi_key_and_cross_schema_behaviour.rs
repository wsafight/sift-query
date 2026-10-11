use super::*;

// ---------------------------------------------------------------------------
// Multi-key and cross-schema behaviour
// ---------------------------------------------------------------------------

#[test]
fn multi_key_cursors_round_trip_and_reject_reordering() {
    let order = order_of(&[(0, SortOrder::Asc), (1, SortOrder::Asc)]);
    let values = vec![
        CursorValue::Scalar(ScalarValue::I64(4)),
        CursorValue::Scalar(ScalarValue::Text("a".to_owned())),
    ];
    let token = codec().encode::<RichEntity>(&order, &values).unwrap();
    assert_eq!(
        codec()
            .decode::<RichEntity>(&token, &order)
            .unwrap()
            .values(),
        values.as_slice()
    );

    let mut swapped = order.clone();
    swapped.swap(0, 1);
    assert!(codec().decode::<RichEntity>(&token, &swapped).is_err());
}

#[test]
fn a_cursor_is_bound_to_one_entity_schema() {
    let entity_order = support::parse_rich("sort=id&order=asc")
        .unwrap()
        .order()
        .to_vec();
    assert_eq!(entity_order.len(), 1);
    let token = codec()
        .encode::<RichEntity>(&entity_order, &[CursorValue::Scalar(ScalarValue::I64(1))])
        .unwrap();

    // Decoding with a different entity type fails the fingerprint check.
    let other = vec![OrderBy {
        field: FieldId(0),
        direction: SortOrder::Asc,
        nulls: NullOrder::Last,
    }];
    assert!(codec().decode::<TestEntity>(&token, &other).is_err());
}

#[test]
fn entity_limits_are_visible_to_the_codec_fixtures() {
    // Keeps the fixture honest: the codec tests above rely on these limits.
    let _ = EntityLimits::default();
    let _ = EntityMeta::validate;
    let _ = FieldKind::Scalar;
    let _ = Operator::Eq;
    let _ = OperatorSet(&[]);
    let _ = ValueType::I64;
}
