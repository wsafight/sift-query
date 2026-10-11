use super::*;

// ---------------------------------------------------------------------------
// validate()
// ---------------------------------------------------------------------------

#[test]
fn validate_accepts_a_well_formed_scalar_entity() {
    assert_eq!(meta("User", "app_user u", ONE_FIELD).validate(), Ok(()));
}

#[test]
fn validate_rejects_blank_names_and_sources() {
    for (entity_name, from) in [("", "app_user u"), ("User", ""), ("User", "   ")] {
        let error = meta(entity_name, from, ONE_FIELD).validate().unwrap_err();
        assert_eq!(
            error,
            ConfigError::Invalid("entity_name and from must not be empty".to_owned())
        );
    }
}

#[test]
fn validate_requires_contiguous_field_ids() {
    static GAP: &[FieldMeta] = &[
        FieldMeta {
            id: FieldId(0),
            public_name: "id",
            column: "t.id",
            result_alias: Some("id"),
            value_type: ValueType::I64,
            kind: FieldKind::Scalar,
            filterable: true,
            selectable: true,
            sortable: true,
            operators: OperatorSet(&[Operator::Eq]),
        },
        FieldMeta {
            id: FieldId(5),
            public_name: "name",
            column: "t.name",
            result_alias: Some("name"),
            value_type: ValueType::Text,
            kind: FieldKind::Scalar,
            filterable: true,
            selectable: true,
            sortable: true,
            operators: OperatorSet(&[Operator::Eq]),
        },
    ];
    assert_eq!(
        meta("User", "app_user u", GAP).validate().unwrap_err(),
        ConfigError::Invalid("field name has non-contiguous FieldId 5".to_owned())
    );
}

#[test]
fn validate_rejects_scalar_columns_on_grouped_entities() {
    static GROUPED: &[FieldMeta] = &[
        FieldMeta {
            id: FieldId(0),
            public_name: "courseId",
            column: "s.course_id",
            result_alias: Some("courseId"),
            value_type: ValueType::I64,
            kind: FieldKind::GroupKey,
            filterable: true,
            selectable: true,
            sortable: true,
            operators: OperatorSet(&[Operator::Eq]),
        },
        FieldMeta {
            id: FieldId(1),
            public_name: "score",
            column: "s.score",
            result_alias: Some("score"),
            value_type: ValueType::I32,
            kind: FieldKind::Scalar,
            filterable: true,
            selectable: true,
            sortable: true,
            operators: OperatorSet(&[Operator::Eq]),
        },
    ];
    assert_eq!(
        meta("Score", "student_score s", GROUPED)
            .validate()
            .unwrap_err(),
        ConfigError::Invalid("grouped entities cannot return Scalar fields".to_owned())
    );
}

#[test]
fn validate_allows_filter_only_scalars_on_grouped_entities() {
    static GROUPED: &[FieldMeta] = &[
        FieldMeta {
            id: FieldId(0),
            public_name: "courseId",
            column: "s.course_id",
            result_alias: Some("courseId"),
            value_type: ValueType::I64,
            kind: FieldKind::GroupKey,
            filterable: true,
            selectable: true,
            sortable: true,
            operators: OperatorSet(&[Operator::Eq]),
        },
        FieldMeta {
            id: FieldId(1),
            public_name: "tenant",
            column: "s.tenant_id",
            result_alias: None,
            value_type: ValueType::I64,
            kind: FieldKind::FilterOnly,
            filterable: true,
            selectable: false,
            sortable: false,
            operators: OperatorSet(&[Operator::Eq]),
        },
    ];
    assert_eq!(meta("Score", "student_score s", GROUPED).validate(), Ok(()));
}
