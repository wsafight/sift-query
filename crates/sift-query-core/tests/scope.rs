//! `ServerScope` construction, composition, and limit enforcement.

use sift_query_core::{FieldRef, ParamError, PredicateNode, ScalarKind, ScopeLimits, ServerScope};

mod support;

use support::{RichEntity, TestEntity};

fn id() -> FieldRef<RichEntity, i64, ScalarKind> {
    FieldRef::new(sift_query_core::FieldId(0))
}

fn name() -> FieldRef<RichEntity, String, ScalarKind> {
    FieldRef::new(sift_query_core::FieldId(1))
}

fn age() -> FieldRef<RichEntity, i32, ScalarKind> {
    FieldRef::new(sift_query_core::FieldId(2))
}

fn conditions<E: sift_query_core::SearchEntity>(scope: &ServerScope<E>) -> usize {
    scope
        .predicate()
        .nodes()
        .iter()
        .filter(|node| matches!(node, PredicateNode::Condition(_)))
        .count()
}

#[test]
fn unrestricted_scope_has_no_predicate() {
    let scope = ServerScope::<RichEntity>::unrestricted();
    assert!(scope.predicate().root().is_none());
    assert_eq!(scope.predicate().nodes().len(), 0);
}

#[test]
fn scope_provider_builds_a_scope_from_trusted_context() {
    let tenant = 42_i64;
    let provider = move |value: &i64| ServerScope::<RichEntity>::new().eq(id(), *value).build();
    let scope =
        sift_query_core::ScopeProvider::<i64, RichEntity>::scope(&provider, &tenant).unwrap();
    assert_eq!(conditions(&scope), 1);
}

#[test]
fn every_scope_helper_builds_a_row_phase_condition() {
    let scope = ServerScope::<RichEntity>::new()
        .eq(id(), 1_i64)
        .ne(id(), 2_i64)
        .gt(id(), 3_i64)
        .ge(id(), 4_i64)
        .lt(id(), 5_i64)
        .le(id(), 6_i64)
        .between(age(), 7_i32, 8_i32)
        .in_list(id(), [9_i64, 10_i64])
        .is_null(name())
        .is_not_null(name())
        .starts_with(name(), "Al")
        .contains(name(), "li")
        .contains_ignore_case(name(), "LI")
        .build()
        .unwrap();
    assert_eq!(conditions(&scope), 13);
    let root = scope.predicate().root().unwrap();
    assert!(matches!(
        scope.predicate().node(root),
        Some(PredicateNode::And(_))
    ));
}

#[test]
fn scope_custom_operator_conditions_are_accepted() {
    let operator =
        sift_query_core::CustomOperator::new("parity", "mod({field}, {0}) = 0", 1).unwrap();
    let scope = ServerScope::<RichEntity>::new()
        .custom(id(), operator, [2_i64])
        .build()
        .unwrap();
    assert_eq!(conditions(&scope), 1);
}

#[test]
fn scope_exists_helpers_keep_relation_sql_server_owned() {
    let operator = sift_query_core::CustomOperator::exists(
        "has_order",
        "EXISTS (SELECT 1 FROM orders o WHERE o.user_id = {field} AND o.state_id = {0})",
        1,
    )
    .unwrap();
    let scope = ServerScope::<RichEntity>::new()
        .exists(id(), operator, [7_i64])
        .build()
        .unwrap();
    assert_eq!(conditions(&scope), 1);
}

#[test]
fn empty_scopes_require_the_explicit_unrestricted_constructor() {
    assert_eq!(
        ServerScope::<RichEntity>::new().build().unwrap_err(),
        ParamError::InvalidValue {
            field: "server scope".to_owned(),
            reason: "empty scope must use ServerScope::unrestricted() explicitly".to_owned(),
        }
    );
}

#[test]
fn scope_rejects_unknown_aggregate_and_mistyped_fields() {
    assert_eq!(
        ServerScope::<RichEntity>::new()
            .eq(
                FieldRef::<RichEntity, i64, ScalarKind>::new(sift_query_core::FieldId(99)),
                1_i64
            )
            .build()
            .unwrap_err(),
        ParamError::UnknownField("#99".to_owned())
    );

    // Aggregates belong to the user query, never to the server scope.
    let aggregate = FieldRef::<TestEntity, i32, ScalarKind>::new(sift_query_core::FieldId(2));
    assert_eq!(
        ServerScope::<TestEntity>::new()
            .eq(aggregate, 1_i32)
            .build()
            .unwrap_err(),
        ParamError::InvalidValue {
            field: "score".to_owned(),
            reason: "scope field type or phase mismatch".to_owned(),
        }
    );

    assert_eq!(
        ServerScope::<RichEntity>::new()
            .eq(
                FieldRef::<RichEntity, String, ScalarKind>::new(sift_query_core::FieldId(0)),
                "a".to_owned()
            )
            .build()
            .unwrap_err(),
        ParamError::InvalidValue {
            field: "id".to_owned(),
            reason: "scope field type or phase mismatch".to_owned(),
        }
    );
}

#[test]
fn scope_rejects_arity_and_ignore_case_violations() {
    assert_eq!(
        ServerScope::<RichEntity>::new()
            .in_list(id(), Vec::<i64>::new())
            .build()
            .unwrap_err(),
        ParamError::WrongArity {
            field: "id".to_owned(),
            operator: "il",
            expected: "1..N",
            actual: 0,
        }
    );

    assert_eq!(
        ServerScope::<RichEntity>::new()
            .contains_ignore_case(
                FieldRef::<RichEntity, String, ScalarKind>::new(sift_query_core::FieldId(0)),
                "a".to_owned(),
            )
            .build()
            .unwrap_err(),
        ParamError::InvalidValue {
            field: "id".to_owned(),
            reason: "scope field type or phase mismatch".to_owned(),
        }
    );
}

#[test]
fn scope_builders_latch_the_first_error() {
    let error = ServerScope::<RichEntity>::new()
        .eq(
            FieldRef::<RichEntity, i64, ScalarKind>::new(sift_query_core::FieldId(99)),
            1_i64,
        )
        .eq(id(), 1_i64)
        .build()
        .unwrap_err();
    assert_eq!(error, ParamError::UnknownField("#99".to_owned()));
}

fn scope_one() -> ServerScope<RichEntity> {
    ServerScope::<RichEntity>::new()
        .eq(id(), 1_i64)
        .build()
        .unwrap()
}

fn scope_two() -> ServerScope<RichEntity> {
    ServerScope::<RichEntity>::new()
        .eq(id(), 2_i64)
        .build()
        .unwrap()
}

#[test]
fn scopes_combine_with_and_and_or() {
    let all = ServerScope::all([scope_one(), scope_two()]).unwrap();
    let root = all.predicate().root().unwrap();
    assert!(matches!(
        all.predicate().node(root),
        Some(PredicateNode::And(_))
    ));
    assert_eq!(conditions(&all), 2);

    let any = ServerScope::any([scope_one(), scope_two()]).unwrap();
    let root = any.predicate().root().unwrap();
    assert!(matches!(
        any.predicate().node(root),
        Some(PredicateNode::Or(_))
    ));
    assert_eq!(conditions(&any), 2);

    // Composing a single scope collapses to that scope's own root.
    let single = ServerScope::all([scope_one()]).unwrap();
    let root = single.predicate().root().unwrap();
    assert!(matches!(
        single.predicate().node(root),
        Some(PredicateNode::Condition(_))
    ));
}

#[test]
fn scope_composition_rejects_empty_input() {
    let empty: [ServerScope<RichEntity>; 0] = [];
    assert_eq!(
        ServerScope::all(empty).unwrap_err(),
        ParamError::InvalidValue {
            field: "server scope".to_owned(),
            reason: "scope composition must not be empty".to_owned(),
        }
    );

    let empty: [ServerScope<RichEntity>; 0] = [];
    assert_eq!(
        ServerScope::any(empty).unwrap_err(),
        ParamError::InvalidValue {
            field: "server scope".to_owned(),
            reason: "scope composition must not be empty".to_owned(),
        }
    );
}

#[test]
fn scope_composition_enforces_its_own_limits() {
    let limits = ScopeLimits {
        max_conditions: 1,
        max_logical_values: 512,
    };
    assert_eq!(
        ServerScope::all_with_limits([scope_one(), scope_two()], limits).unwrap_err(),
        ParamError::TooComplex("server scope condition count")
    );

    let limits = ScopeLimits {
        max_conditions: 64,
        max_logical_values: 1,
    };
    assert_eq!(
        ServerScope::any_with_limits([scope_one(), scope_two()], limits).unwrap_err(),
        ParamError::TooComplex("server scope logical value count")
    );
}

#[test]
fn scope_builder_enforces_its_own_limits() {
    let limits = ScopeLimits {
        max_conditions: 1,
        max_logical_values: 512,
    };
    assert_eq!(
        ServerScope::<RichEntity>::builder(limits)
            .eq(id(), 1_i64)
            .eq(id(), 2_i64)
            .build()
            .unwrap_err(),
        ParamError::TooComplex("server scope condition count")
    );

    let limits = ScopeLimits {
        max_conditions: 64,
        max_logical_values: 1,
    };
    assert_eq!(
        ServerScope::<RichEntity>::builder(limits)
            .in_list(id(), [1_i64, 2_i64])
            .build()
            .unwrap_err(),
        ParamError::TooComplex("server scope logical value count")
    );
}
