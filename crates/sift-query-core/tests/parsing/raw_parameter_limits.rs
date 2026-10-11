use super::*;

// ---------------------------------------------------------------------------
// Raw parameter limits
// ---------------------------------------------------------------------------

#[test]
fn raw_limits_are_enforced_before_any_field_lookup() {
    let long = with_limits(|limits| limits.max_raw_query_bytes = 8);
    assert_eq!(
        rich_with(&long, "name=Alice").unwrap_err(),
        ParamError::QueryTooLong
    );

    let items = with_limits(|limits| limits.max_raw_items = 1);
    assert_eq!(
        rich_with(&items, "id=1&name=a").unwrap_err(),
        ParamError::TooComplex("raw parameter item count")
    );

    let key = with_limits(|limits| limits.max_key_bytes = 1);
    assert_eq!(
        rich_with(&key, "id=1").unwrap_err(),
        ParamError::TooComplex("parameter key or value length")
    );

    let value = with_limits(|limits| limits.max_value_bytes = 2);
    assert_eq!(
        rich_with(&value, "name=Alice").unwrap_err(),
        ParamError::TooComplex("parameter key or value length")
    );

    let decoded = with_limits(|limits| limits.max_decoded_bytes = 4);
    assert_eq!(
        rich_with(&decoded, "name=Alice").unwrap_err(),
        ParamError::TooComplex("decoded parameter bytes")
    );
}

#[test]
fn duplicate_control_parameters_are_rejected() {
    for query in [
        "page=1&page=2",
        "size=1&size=2",
        "offset=1&offset=2",
        "sort=id&sort=name",
        "order=asc&order=desc",
        "orderBy=id:asc&orderBy=name:asc",
        "gexpr=A&gexpr=B",
        "onlySelect=id&onlySelect=name",
        "selectExclude=id&selectExclude=name",
    ] {
        assert!(
            matches!(rich_error(query), ParamError::DuplicateParameter(_)),
            "query {query}"
        );
    }
}

#[test]
fn field_names_cannot_collide_with_control_parameters() {
    // `RichEntity` has no field named after a control parameter, so the
    // conflict check must be exercised through a custom policy whose control
    // name matches an entity field.
    let names = ParamNames {
        page: "name".to_owned(),
        ..ParamNames::default()
    };
    let policy = QueryPolicy::new(names, QueryLimits::default()).unwrap();
    assert_eq!(
        rich_with(&policy, "").unwrap_err(),
        ParamError::ConflictingParameters(
            "field name conflicts with a control parameter".to_owned()
        )
    );
}

#[test]
fn invalid_field_keys_are_rejected_with_precise_errors() {
    assert_eq!(
        rich_error("id-01=1"),
        ParamError::InvalidName("id-01".to_owned())
    );
    assert_eq!(
        rich_error("id-x=1"),
        ParamError::UnknownParameter("id-x".to_owned())
    );
    assert_eq!(
        rich_error("A.B.C=1"),
        ParamError::InvalidName("A.B.C".to_owned())
    );
    assert_eq!(
        rich_error("1id=1"),
        ParamError::InvalidName("1id".to_owned())
    );
    assert_eq!(
        rich_error("1group.id=1"),
        ParamError::InvalidName("1group.id".to_owned())
    );
    assert_eq!(
        rich_error("id-op=zz"),
        ParamError::InvalidValue {
            field: "id".to_owned(),
            reason: "unknown operator zz".to_owned(),
        }
    );
}

#[test]
fn ignore_case_accepts_only_booleans_and_rejects_duplicates() {
    assert_eq!(
        rich_error("name=A&name-ic=maybe"),
        ParamError::InvalidValue {
            field: "name".to_owned(),
            reason: "ignore-case must be true or false".to_owned(),
        }
    );
    assert_eq!(
        rich_error("name=A&name-ic=true&name-ic=false"),
        ParamError::DuplicateParameter("name-ic".to_owned())
    );
    assert_eq!(
        rich_error("id=1&id-op=eq&id-op=ne"),
        ParamError::DuplicateParameter("id-op".to_owned())
    );
    assert_eq!(
        rich_error("id-0=1&id-0=2"),
        ParamError::DuplicateParameter("id-0".to_owned())
    );
}

#[test]
fn indexed_values_must_be_contiguous_from_zero() {
    let query = parse_rich("id-0=1&id-1=2&id-op=il").unwrap();
    let condition = query
        .predicate()
        .nodes()
        .iter()
        .find_map(|node| match node {
            PredicateNode::Condition(condition) => Some(condition),
            _ => None,
        })
        .unwrap();
    assert_eq!(condition.values.len(), 2);

    assert_eq!(
        rich_error("id-1=2&id-op=il"),
        ParamError::NonContiguousIndex {
            field: "id".to_owned(),
            expected: 0,
        }
    );
    assert_eq!(
        rich_error("id-0=1&id-2=3&id-op=il"),
        ParamError::NonContiguousIndex {
            field: "id".to_owned(),
            expected: 1,
        }
    );
}

#[test]
fn unknown_fields_are_strict_unless_bean_searcher_compatibility_is_enabled() {
    assert_eq!(
        rich_error("missing=1"),
        ParamError::UnknownField("missing".to_owned())
    );
    let compatible = QueryPolicy::bean_searcher_4812();
    assert!(rich_with(&compatible, "missing=1").is_ok());
    assert_eq!(
        compatible.compatibility(),
        CompatibilityPolicy::BeanSearcher4812
    );
}

#[test]
fn not_filterable_and_operator_whitelist_errors_surface_from_the_parser() {
    assert_eq!(
        rich_error("computed=1"),
        ParamError::NotFilterable("computed".to_owned())
    );
    assert_eq!(
        rich_error("secret-0=a&secret-1=b&secret-op=bt"),
        ParamError::OperatorNotAllowed {
            field: "secret".to_owned(),
            operator: "bt",
        }
    );
    assert_eq!(
        // `ey` takes no values and does not support ignore-case.
        rich_error("name-ic=true&name-op=ey"),
        ParamError::IgnoreCaseNotAllowed("name".to_owned())
    );
}

#[test]
fn value_parsing_reports_the_offending_field() {
    assert_eq!(
        rich_error("id=abc"),
        ParamError::InvalidValue {
            field: "id".to_owned(),
            reason: "expected a 64-bit decimal integer".to_owned(),
        }
    );
    assert_eq!(
        rich_error("age=1.5"),
        ParamError::InvalidValue {
            field: "age".to_owned(),
            reason: "expected a 32-bit decimal integer".to_owned(),
        }
    );
}

#[test]
fn conditions_are_sorted_by_field_id_for_stable_sql() {
    let query = parse_rich("name=A&age=1&id=2").unwrap();
    let fields = query
        .predicate()
        .nodes()
        .iter()
        .filter_map(|node| match node {
            PredicateNode::Condition(condition) => Some(condition.field.0),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(fields, vec![0, 1, 2]);
}
