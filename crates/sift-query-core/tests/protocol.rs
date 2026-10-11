mod support;

use sift_query_core::{
    CompatibilityPolicy, FieldId, ParamError, ParamNames, ParamParser, QueryLimits, QueryPolicy,
    RawParams,
};

use support::parse;

#[test]
fn repeated_values_require_a_matching_operator() {
    assert!(matches!(
        parse("id=1&id=2"),
        Err(ParamError::WrongArity { .. })
    ));
    let query = parse("id=1&id=2&id-op=il").unwrap();
    assert!(query.predicate().root().is_some());
}

#[test]
fn indexed_and_repeated_values_cannot_mix() {
    assert!(matches!(
        parse("id=1&id-0=2&id-op=il"),
        Err(ParamError::ConflictingParameters(_))
    ));
}

#[test]
fn mixed_phase_or_is_rejected() {
    let error = parse("A.id=1&B.score=10&B.score-op=ge&gexpr=A%7CB").unwrap_err();
    assert_eq!(error, ParamError::MixedPredicatePhase);
}

#[test]
fn unused_group_is_rejected() {
    let error = parse("A.id=1&B.name=a&gexpr=A").unwrap_err();
    assert_eq!(error, ParamError::UnusedGroup("B".to_owned()));
}

#[test]
fn computed_offset_is_bounded() {
    assert_eq!(
        parse("page=202&size=100").unwrap_err(),
        ParamError::PagingOutOfRange("computed page offset".to_owned())
    );
}

#[test]
fn stable_order_is_appended() {
    let query = parse("sort=name&order=desc").unwrap();
    assert_eq!(query.order().len(), 2);
    assert_eq!(query.order()[1].field, FieldId(0));
}

#[test]
fn duplicate_controls_and_projection_conflicts_are_rejected() {
    assert!(matches!(
        parse("page=1&page=2"),
        Err(ParamError::DuplicateParameter(_))
    ));
    assert!(matches!(
        parse("onlySelect=id&selectExclude=name"),
        Err(ParamError::ConflictingParameters(_))
    ));
}

#[test]
fn operator_arity_is_strict() {
    assert!(matches!(
        parse("id=1&id-op=bt"),
        Err(ParamError::WrongArity { .. })
    ));
    assert!(matches!(
        parse("id=1&id-op=nl"),
        Err(ParamError::WrongArity { .. })
    ));
}

#[test]
fn ignore_case_is_restricted_to_text_operators() {
    assert!(matches!(
        parse("id=1&id-ic=true"),
        Err(ParamError::IgnoreCaseNotAllowed(_))
    ));
}

#[test]
fn custom_control_names_are_instance_scoped() {
    let names = ParamNames {
        page: "p".to_owned(),
        ..ParamNames::default()
    };
    let policy = QueryPolicy::new(names, QueryLimits::default()).unwrap();
    let raw = RawParams::parse("p=2&size=10").unwrap();
    let query = ParamParser::new(&policy)
        .unwrap()
        .parse::<support::TestEntity>(&raw)
        .unwrap();
    assert_eq!(query.paging().offset, 10);
}

#[test]
fn group_expression_applies_idempotence_and_absorption() {
    let duplicate = parse("A.id=1&gexpr=A%7CA").unwrap();
    assert_eq!(duplicate.predicate().nodes().len(), 1);

    let absorbed = parse("A.id=1&B.name=x&gexpr=A%7C%28A%26B%29").unwrap();
    assert_eq!(absorbed.predicate().nodes().len(), 1);
}

#[test]
fn named_bean_searcher_compatibility_is_explicit() {
    let strict = parse("unknown=1").unwrap_err();
    assert!(matches!(strict, ParamError::UnknownField(_)));

    let policy = QueryPolicy::bean_searcher_4812();
    assert_eq!(
        policy.compatibility(),
        CompatibilityPolicy::BeanSearcher4812
    );
    let ignored = ParamParser::new(&policy)
        .unwrap()
        .parse::<support::TestEntity>(&RawParams::parse("unknown=1").unwrap())
        .unwrap();
    assert!(ignored.predicate().root().is_none());

    let empty = ParamParser::new(&policy)
        .unwrap()
        .parse::<support::TestEntity>(&RawParams::parse("page=0").unwrap())
        .unwrap();
    assert!(empty.is_forced_empty());
}
