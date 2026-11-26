//! Regression tests for core budget, scope, policy, cursor, bucket, and RSQL invariants.

use sift_query_core::{
    CursorCodec, CursorValue, FieldId, FieldRef, NullOrder, NumericBucketRequest, OrderBy,
    ParamError, ParamNames, ParamParser, QueryBuilder, QueryLimits, QueryPolicy, RawParams,
    ScalarKind, ScalarValue, SearchEntity, SearchOptions, ServerScope, SortOrder,
};

mod support;

use support::{RichEntity, TestEntity};

fn rich_id() -> FieldRef<RichEntity, i64, ScalarKind> {
    FieldRef::new(FieldId(0))
}

fn rich_name() -> FieldRef<RichEntity, String, ScalarKind> {
    FieldRef::new(FieldId(1))
}

fn rich_score() -> FieldRef<RichEntity, f64, ScalarKind> {
    FieldRef::new(FieldId(4))
}

#[test]
fn gexpr_expansion_cannot_bypass_condition_or_bind_budgets() {
    let limits = QueryLimits {
        max_conditions: 4,
        max_bind_slots: 4,
        ..QueryLimits::default()
    };
    let policy = QueryPolicy::new(ParamNames::default(), limits).unwrap();
    let raw = RawParams::from_entries(
        vec![
            ("a.id".to_owned(), "1".to_owned()),
            ("b.id".to_owned(), "2".to_owned()),
            ("gexpr".to_owned(), "a|b|a|b|a".to_owned()),
        ],
        policy.limits(),
    )
    .unwrap();
    let error = ParamParser::new(&policy)
        .unwrap()
        .parse::<RichEntity>(&raw)
        .unwrap_err();
    assert_eq!(error, ParamError::TooComplex("condition count"));
}

#[test]
fn server_scope_any_with_unrestricted_stays_unrestricted() {
    let tenant = ServerScope::<RichEntity>::new()
        .eq(rich_id(), 42_i64)
        .build()
        .unwrap();
    let combined = ServerScope::any([ServerScope::<RichEntity>::unrestricted(), tenant]).unwrap();
    assert!(combined.predicate().root().is_none());
}

#[test]
fn query_policy_fingerprint_is_consistent_across_constructors() {
    let default_policy = QueryPolicy::default();
    let rebuilt = QueryPolicy::default()
        .with_param_names(ParamNames::default())
        .unwrap();
    assert_eq!(default_policy.fingerprint(), rebuilt.fingerprint());

    let compat = QueryPolicy::bean_searcher_4812();
    let compat_rebuilt = QueryPolicy::bean_searcher_4812()
        .with_param_names(ParamNames::default())
        .unwrap();
    assert_eq!(compat.fingerprint(), compat_rebuilt.fingerprint());
}

#[test]
fn query_builder_rejects_mismatched_cursor_value_types() {
    let codec = CursorCodec::new(vec![7_u8; 32]).unwrap();
    let order = vec![OrderBy::from(RichEntity::META.default_order[0])];
    let token = codec
        .encode::<RichEntity>(
            &order,
            &[CursorValue::Scalar(ScalarValue::Text("bad".into()))],
        )
        .unwrap();
    let cursor = codec.decode::<RichEntity>(&token, &order).unwrap();
    let error = QueryBuilder::<RichEntity>::new(&QueryPolicy::default())
        .cursor(cursor, 10)
        .build()
        .unwrap_err();
    assert!(matches!(error, ParamError::InvalidValue { .. }));
}

#[test]
fn query_builder_cursor_allows_order_by_after_cursor() {
    let codec = CursorCodec::new(vec![7_u8; 32]).unwrap();
    let order = vec![
        OrderBy {
            field: FieldId(1),
            direction: SortOrder::Asc,
            nulls: NullOrder::Last,
        },
        OrderBy::from(RichEntity::META.stable_order[0]),
    ];
    let token = codec
        .encode::<RichEntity>(
            &order,
            &[
                CursorValue::Scalar(ScalarValue::Text("Ada".into())),
                CursorValue::Scalar(ScalarValue::I64(10)),
            ],
        )
        .unwrap();
    let cursor = codec.decode::<RichEntity>(&token, &order).unwrap();
    let query = QueryBuilder::<RichEntity>::new(&QueryPolicy::default())
        .cursor(cursor, 10)
        .order_by(rich_name(), SortOrder::Asc)
        .build()
        .unwrap();
    assert_eq!(query.order().len(), 2);
}

#[test]
fn query_builder_enforces_max_bind_slots_without_condition_order() {
    let limits = QueryLimits {
        max_bind_slots: 1,
        max_conditions: 8,
        max_logical_values: 8,
        ..QueryLimits::default()
    };
    let policy = QueryPolicy::new(ParamNames::default(), limits).unwrap();
    let error = QueryBuilder::<RichEntity>::new(&policy)
        .eq(rich_id(), 1_i64)
        .eq(rich_name(), "Ada".to_owned())
        .build()
        .unwrap_err();
    assert_eq!(error, ParamError::TooComplex("database bind slot count"));
}

#[test]
fn numeric_bucket_rejects_non_finite_floats_and_empty_label_is_safe() {
    assert!(NumericBucketRequest::new(rich_score(), [f64::NAN]).is_err());
    assert!(NumericBucketRequest::new(rich_score(), [f64::INFINITY]).is_err());
    assert!(NumericBucketRequest::new(rich_score(), [f64::NEG_INFINITY, f64::INFINITY]).is_err());
    assert_eq!(sift_query_core::numeric_bucket_label(&[], 0), "");
}

#[test]
fn search_options_preserves_first_error() {
    let error = SearchOptions::<TestEntity>::builder()
        .sum(FieldRef::<TestEntity, i64, ScalarKind>::new(FieldId(99)))
        .sum(FieldRef::<TestEntity, i64, ScalarKind>::new(FieldId(0)))
        .sum(FieldRef::<TestEntity, i64, ScalarKind>::new(FieldId(0)))
        .build()
        .unwrap_err();
    assert_eq!(error, ParamError::UnknownField("#99".to_owned()));
}
