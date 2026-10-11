use sift_query_core::{
    Condition, CustomOperator, FieldId, Operator, ParamError, PredicateArena, PredicateNode,
    PredicatePhase, ScalarValue,
};

fn condition(phase: PredicatePhase) -> PredicateArena {
    PredicateArena::from_condition(Condition {
        field: FieldId(0),
        operator: Operator::Eq,
        values: vec![ScalarValue::I32(1)],
        ignore_case: false,
        phase,
    })
}

#[test]
fn and_can_be_partitioned_across_phases() {
    let arena = PredicateArena::all([
        condition(PredicatePhase::Row),
        condition(PredicatePhase::Group),
    ]);
    let (row, group) = arena.partition().unwrap();
    assert!(row.root().is_some());
    assert!(group.root().is_some());
}

#[test]
fn or_rejects_mixed_phases() {
    let arena = PredicateArena::any([
        condition(PredicatePhase::Row),
        condition(PredicatePhase::Group),
    ]);
    assert_eq!(arena.partition(), Err(ParamError::MixedPredicatePhase));
}

#[test]
fn custom_operator_templates_are_restricted() {
    let valid = CustomOperator::new("multiple_of", "mod({field}, {0}) = 0", 1).unwrap();
    assert_eq!(valid.name(), "multiple_of");
    assert!(CustomOperator::new("unsafe", "{field} = {0}; drop table users", 1).is_err());
    assert!(CustomOperator::new("missing", "{field} > 0", 1).is_err());
}

#[test]
fn exists_templates_require_a_correlated_select() {
    let exists = CustomOperator::exists(
        "has_order",
        "EXISTS (SELECT 1 FROM orders o WHERE o.user_id = {field} AND o.state = {0})",
        1,
    )
    .unwrap();
    assert!(exists.is_exists());
    assert!(!exists.is_not_exists());

    let not_exists = CustomOperator::not_exists(
        "without_order",
        "NOT EXISTS (SELECT 1 FROM orders o WHERE o.user_id = {field})",
        0,
    )
    .unwrap();
    assert!(not_exists.is_not_exists());
    assert!(CustomOperator::exists("bad", "id = {field}", 0).is_err());
}

#[test]
fn predicate_field_exclusion_preserves_other_conditions() {
    let other = PredicateArena::from_condition(Condition {
        field: FieldId(1),
        operator: Operator::Eq,
        values: vec![ScalarValue::I32(2)],
        ignore_case: false,
        phase: PredicatePhase::Row,
    });
    let filtered = PredicateArena::any([condition(PredicatePhase::Row), other]);
    let without_first = filtered.without_field(FieldId(0));
    assert_eq!(without_first.nodes().len(), 1);
    assert!(matches!(
        without_first.node(without_first.root().unwrap()),
        Some(PredicateNode::Condition(condition)) if condition.field == FieldId(1)
    ));
}

#[test]
fn empty_arenas_have_no_root() {
    assert!(PredicateArena::empty().root().is_none());
    assert!(PredicateArena::default().root().is_none());
    assert_eq!(PredicateArena::empty().nodes().len(), 0);
    assert_eq!(PredicateArena::empty(), PredicateArena::default());
}

#[test]
fn custom_operators_report_their_short_name_and_arity_label() {
    let operator = CustomOperator::new("near", "abs({field} - {0}) < 1", 1).unwrap();
    let custom = Operator::Custom(operator);
    assert_eq!(custom.short_name(), "near");
    assert_eq!(custom.expected_arity(), "custom");
    assert!(custom.accepts_arity(1));
    assert!(!custom.accepts_arity(2));
    assert!(!custom.supports_ignore_case());
    assert_eq!(Operator::parse("near"), None);
}

#[test]
fn combining_many_arenas_collapses_single_and_empty_inputs() {
    let one = condition(PredicatePhase::Row);
    assert_eq!(PredicateArena::all([one.clone()]).nodes().len(), 1);
    assert!(PredicateArena::all(Vec::new()).root().is_none());
    assert!(PredicateArena::any(Vec::new()).root().is_none());
}

#[test]
fn group_phase_or_is_accepted() {
    let arena = PredicateArena::any([
        condition(PredicatePhase::Group),
        condition(PredicatePhase::Group),
    ]);
    let (row, group) = arena.partition().unwrap();
    assert!(row.root().is_none());
    assert!(group.root().is_some());
}

#[test]
fn and_nodes_merge_mixed_phases_into_the_group_half() {
    let arena = PredicateArena::all([
        condition(PredicatePhase::Row),
        PredicateArena::all([
            condition(PredicatePhase::Group),
            condition(PredicatePhase::Group),
        ]),
    ]);
    let (row, group) = arena.partition().unwrap();
    assert!(row.root().is_some());
    assert!(group.root().is_some());
}

#[test]
fn nested_and_inside_or_rejects_mixed_phases() {
    let arena = PredicateArena::any([
        PredicateArena::all([
            condition(PredicatePhase::Row),
            condition(PredicatePhase::Group),
        ]),
        condition(PredicatePhase::Row),
    ]);
    assert_eq!(arena.partition(), Err(ParamError::MixedPredicatePhase));
}
