use super::*;

// ---------------------------------------------------------------------------
// SearchOptions
// ---------------------------------------------------------------------------

#[test]
fn summary_options_cover_every_aggregate_function() {
    let id = FieldRef::<RichEntity, i64, ScalarKind>::new(sift_query_core::FieldId(0));
    let name = FieldRef::<RichEntity, String, ScalarKind>::new(sift_query_core::FieldId(1));
    let custom =
        CustomAggregate::new("spread", "max({field}) - min({field})", ValueType::I64).unwrap();
    let options = SearchOptions::<RichEntity>::builder()
        .sum(id)
        .avg(id)
        .min(id)
        .max(id)
        .custom(id, custom)
        .min(name)
        .max(name)
        .build()
        .unwrap();
    assert_eq!(options.requests().len(), 7);
    assert_eq!(options.requests()[0].input_type, ValueType::I64);
    assert_eq!(options.requests()[5].input_type, ValueType::Text);
}

#[test]
fn summary_options_reject_duplicates_and_overflow() {
    let id = FieldRef::<RichEntity, i64, ScalarKind>::new(sift_query_core::FieldId(0));
    let age = FieldRef::<RichEntity, i32, ScalarKind>::new(sift_query_core::FieldId(2));
    assert_eq!(
        SearchOptions::<RichEntity>::builder()
            .sum(id)
            .sum(id)
            .build()
            .unwrap_err(),
        ParamError::DuplicateParameter("summary".to_owned())
    );

    // The same field with a different function is not a duplicate.
    assert!(
        SearchOptions::<RichEntity>::builder()
            .sum(id)
            .min(id)
            .build()
            .is_ok()
    );

    let error = SearchOptions::<RichEntity>::builder()
        .sum(id)
        .avg(id)
        .min(id)
        .max(id)
        .sum(age)
        .avg(age)
        .min(age)
        .max(age)
        .sum(age)
        .build()
        .unwrap_err();
    assert_eq!(error, ParamError::TooComplex("summary count"));
}

#[test]
fn summary_options_reject_unknown_fields() {
    let unknown = FieldRef::<RichEntity, i64, ScalarKind>::new(sift_query_core::FieldId(99));
    assert_eq!(
        SearchOptions::<RichEntity>::builder()
            .sum(unknown)
            .build()
            .unwrap_err(),
        ParamError::UnknownField("#99".to_owned())
    );
}

#[test]
fn custom_aggregates_reject_unsafe_templates() {
    assert!(CustomAggregate::new("bad name", "count({field})", ValueType::I64).is_err());
    assert!(CustomAggregate::new("", "count({field})", ValueType::I64).is_err());
    assert!(CustomAggregate::new("x", "count(*)", ValueType::I64).is_err());
    assert!(CustomAggregate::new("x", "count({field}) --", ValueType::I64).is_err());
    assert!(CustomAggregate::new("x", "count({field}) /*", ValueType::I64).is_err());
    assert!(CustomAggregate::new("x", "count({field}) $1", ValueType::I64).is_err());
    assert!(CustomAggregate::new("x", "count({field});", ValueType::I64).is_err());
    assert!(CustomAggregate::new("x", "count({})", ValueType::I64).is_err());

    let aggregate =
        CustomAggregate::new("count_nonnull", "count({field})", ValueType::I64).unwrap();
    assert_eq!(aggregate.name(), "count_nonnull");
    assert_eq!(aggregate.template(), "count({field})");
    assert_eq!(aggregate.output_type(), ValueType::I64);
}

#[test]
fn custom_operators_validate_their_placeholders() {
    use sift_query_core::CustomOperator;
    assert!(CustomOperator::new("", "{field} = {0}", 1).is_err());
    assert!(CustomOperator::new("a b", "{field} = {0}", 1).is_err());
    assert!(CustomOperator::new("x", "{0} = {0}", 1).is_err());
    assert!(CustomOperator::new("x", "{field} = {0", 1).is_err());
    assert!(CustomOperator::new("x", "{field} = {1}", 1).is_err());
    assert!(CustomOperator::new("x", "{field} = {a}", 1).is_err());
    // A two-arity template must reference both slots.
    assert!(CustomOperator::new("x", "{field} between {0} and {0}", 2).is_err());
    let operator = CustomOperator::new("x", "{field} between {0} and {1}", 2).unwrap();
    assert_eq!(operator.arity(), 2);
    assert_eq!(operator.name(), "x");
}

#[test]
fn operator_metadata_is_consistent() {
    use sift_query_core::Operator;
    let all = [
        Operator::Eq,
        Operator::Ne,
        Operator::Gt,
        Operator::Ge,
        Operator::Lt,
        Operator::Le,
        Operator::Between,
        Operator::NotBetween,
        Operator::InList,
        Operator::NotIn,
        Operator::IsNull,
        Operator::NotNull,
        Operator::Empty,
        Operator::NotEmpty,
        Operator::StartWith,
        Operator::EndWith,
        Operator::Contain,
        Operator::OrLike,
        Operator::NotLike,
        Operator::AlwaysTrue,
        Operator::AlwaysFalse,
    ];
    for operator in all {
        // Every operator round-trips through its short name.
        assert_eq!(Operator::parse(operator.short_name()), Some(operator));
        assert!(operator.accepts_arity(match operator {
            Operator::Between | Operator::NotBetween => 2,
            Operator::InList | Operator::NotIn | Operator::OrLike => 3,
            Operator::IsNull
            | Operator::NotNull
            | Operator::Empty
            | Operator::NotEmpty
            | Operator::AlwaysTrue
            | Operator::AlwaysFalse => 0,
            _ => 1,
        }));
    }
    assert_eq!(Operator::parse("zz"), None);
    assert_eq!(Operator::Between.expected_arity(), "2");
    assert_eq!(Operator::InList.expected_arity(), "1..N");
    assert_eq!(Operator::IsNull.expected_arity(), "0");
    assert_eq!(Operator::Eq.expected_arity(), "1");
    assert!(Operator::Eq.supports_ignore_case());
    assert!(!Operator::Between.supports_ignore_case());
}

#[test]
fn typed_fixture_entities_are_reachable() {
    // Keeps the fixture module honest for the feature-gated entities.
    assert_eq!(BaseTyped::META.fields.len(), 7);
    assert_eq!(TestEntity::META.entity_name, "TestEntity");
    assert_eq!(support::CursorEntity::META.schema_fingerprint, 2);
}

#[test]
fn group_expressions_are_normalised_by_absorption() {
    // `A & (A | B)` absorbs to `A`. The referenced-name check runs on the
    // parsed tree, so `B` still counts as referenced and the expression is
    // accepted; simplification then drops the redundant branch.
    let query = parse_rich("A.id=1&B.name=a&gexpr=A%26%28A%7CB%29").unwrap();
    let root = query.predicate().root().unwrap();
    assert!(matches!(
        query.predicate().node(root),
        Some(PredicateNode::Condition(_))
    ));

    // `A | (A & B)` absorbs the same way.
    let query = parse_rich("A.id=1&B.name=a&gexpr=A%7C%28A%26B%29").unwrap();
    let root = query.predicate().root().unwrap();
    assert!(matches!(
        query.predicate().node(root),
        Some(PredicateNode::Condition(_))
    ));

    // Duplicated terms collapse to one.
    let query = parse_rich("A.id=1&gexpr=A%26A").unwrap();
    let root = query.predicate().root().unwrap();
    assert!(matches!(
        query.predicate().node(root),
        Some(PredicateNode::Condition(_))
    ));
}

#[test]
fn group_names_must_be_identifiers() {
    let error = rich_error("A.id=1&gexpr=A%20%26%20%40");
    assert!(
        matches!(error, ParamError::InvalidGroupExpr(ref message) if message.contains("expected group name")),
        "{error:?}"
    );
}

#[test]
fn entity_field_names_are_validated_before_parsing() {
    // A field whose public name is not an identifier is rejected outright,
    // which protects the parameter grammar from fixture mistakes.
    let raw = RawParams::parse("").unwrap();
    assert_eq!(
        ParamParser::new(&QueryPolicy::default())
            .unwrap()
            .parse::<InvalidNameEntity>(&raw)
            .unwrap_err(),
        ParamError::InvalidName("1bad".to_owned())
    );
}

#[test]
fn a_long_group_name_is_rejected() {
    // The name grammar allows alphanumerics, but the length cap is 64 bytes.
    let long = "a".repeat(65);
    let error = rich_error(&format!("A.id=1&gexpr={long}"));
    assert!(
        matches!(error, ParamError::InvalidGroupExpr(ref message) if message.contains("invalid group name")),
        "{error:?}"
    );
}

#[test]
fn indexed_and_repeated_forms_conflict_in_both_directions() {
    // Repeated values first, then an indexed one.
    assert_eq!(
        rich_error("id=1&id-0=2"),
        ParamError::ConflictingParameters("id".to_owned())
    );
    // Indexed values first, then a repeated one.
    assert_eq!(
        rich_error("id-0=1&id=2"),
        ParamError::ConflictingParameters("id".to_owned())
    );
}
