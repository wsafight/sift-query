use super::*;

// ---------------------------------------------------------------------------
// lookup helpers
// ---------------------------------------------------------------------------

#[test]
fn field_lookup_is_positional_and_name_based() {
    assert_eq!(
        RichEntity::META.field(FieldId(0)).unwrap().public_name,
        "id"
    );
    assert!(RichEntity::META.field(FieldId(7)).is_some());
    // FieldId indexes the slice directly, so an out-of-range id is unknown.
    assert!(RichEntity::META.field(FieldId(8)).is_none());
    assert!(RichEntity::META.field(FieldId(u16::MAX)).is_none());

    assert_eq!(
        RichEntity::META.resolve_field("name").unwrap().id,
        FieldId(1)
    );
    assert!(RichEntity::META.resolve_field("missing").is_none());
    assert!(RichEntity::META.resolve_field("").is_none());
    assert_eq!(
        RichEntity::META.resolve_field("name").unwrap().column,
        "u.name"
    );
}

#[test]
fn grouped_detection_covers_every_field_kind() {
    assert!(TestEntity::META.is_grouped());
    assert!(!RichEntity::META.is_grouped());
    assert!(!DistinctEntity::META.is_grouped());

    assert!(meta("G", "t", GROUP_KEY_ONLY).is_grouped());
    assert!(meta("G", "t", AGGREGATE_ONLY).is_grouped());
    assert!(!meta("G", "t", FILTER_ONLY).is_grouped());
}

static GROUP_KEY_ONLY: &[FieldMeta] = &[FieldMeta {
    id: FieldId(0),
    public_name: "k",
    column: "t.k",
    result_alias: Some("k"),
    value_type: ValueType::I64,
    kind: FieldKind::GroupKey,
    filterable: true,
    selectable: true,
    sortable: true,
    operators: OperatorSet(&[Operator::Eq]),
}];

static AGGREGATE_ONLY: &[FieldMeta] = &[FieldMeta {
    id: FieldId(0),
    public_name: "total",
    column: "sum(t.v)",
    result_alias: Some("total"),
    value_type: ValueType::I64,
    kind: FieldKind::Aggregate,
    filterable: true,
    selectable: true,
    sortable: true,
    operators: OperatorSet(&[Operator::Eq]),
}];

static FILTER_ONLY: &[FieldMeta] = &[FieldMeta {
    id: FieldId(0),
    public_name: "tenant",
    column: "t.tenant",
    result_alias: None,
    value_type: ValueType::I64,
    kind: FieldKind::FilterOnly,
    filterable: true,
    selectable: false,
    sortable: false,
    operators: OperatorSet(&[Operator::Eq]),
}];

#[test]
fn field_kind_and_value_type_helpers_are_consistent_with_the_fixtures() {
    assert_eq!(field(0, "a", FieldKind::Scalar).kind, FieldKind::Scalar);
    assert_ne!(FieldKind::Scalar, FieldKind::FilterOnly);
    assert_eq!(EntityLimits::default().max_page_size, 100);
    assert_eq!(EntityLimits::default().max_offset, 20_000);
    assert_eq!(EntityLimits::default().timeout_ms, 5_000);
    assert!(OperatorSet(&[Operator::Eq]).contains(Operator::Eq));
    assert!(!OperatorSet(&[Operator::Eq]).contains(Operator::Ne));
}
