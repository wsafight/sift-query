use super::*;

// ---------------------------------------------------------------------------
// Policy configuration
// ---------------------------------------------------------------------------

#[test]
fn policy_rejects_invalid_names_limits_and_timeouts() {
    assert_eq!(
        QueryPolicy::new(
            ParamNames {
                page: "1bad".to_owned(),
                ..ParamNames::default()
            },
            QueryLimits::default()
        )
        .unwrap_err(),
        sift_query_core::ConfigError::Invalid("invalid control parameter name: 1bad".to_owned())
    );

    assert_eq!(
        QueryPolicy::new(
            ParamNames {
                page: "size".to_owned(),
                ..ParamNames::default()
            },
            QueryLimits::default()
        )
        .unwrap_err(),
        sift_query_core::ConfigError::Invalid("duplicate control parameter name: size".to_owned())
    );

    let limits = QueryLimits {
        timeout_ms: 0,
        ..QueryLimits::default()
    };
    assert_eq!(
        QueryPolicy::new(ParamNames::default(), limits).unwrap_err(),
        sift_query_core::ConfigError::Invalid("query timeout must be greater than zero".to_owned())
    );

    let limits = QueryLimits {
        max_page_size: 101,
        ..QueryLimits::default()
    };
    assert_eq!(
        QueryPolicy::new(ParamNames::default(), limits).unwrap_err(),
        sift_query_core::ConfigError::Invalid(
            "ordinary policy cannot exceed hard limits".to_owned()
        )
    );
}

#[test]
fn policy_fingerprints_track_configuration_changes() {
    let base = QueryPolicy::default();
    assert_eq!(base.fingerprint(), QueryPolicy::default().fingerprint());

    let names = ParamNames {
        page: "p".to_owned(),
        ..ParamNames::default()
    };
    let renamed = QueryPolicy::new(names, QueryLimits::default()).unwrap();
    assert_ne!(base.fingerprint(), renamed.fingerprint());

    let limits = QueryLimits {
        max_page_size: 50,
        ..QueryLimits::default()
    };
    let tightened = QueryPolicy::new(ParamNames::default(), limits).unwrap();
    assert_ne!(base.fingerprint(), tightened.fingerprint());

    let compatible = QueryPolicy::bean_searcher_4812();
    assert_ne!(base.fingerprint(), compatible.fingerprint());

    let with_codec = QueryPolicy::default()
        .with_cursor_codec(sift_query_core::CursorCodec::new([1u8; 32]).unwrap());
    assert_ne!(base.fingerprint(), with_codec.fingerprint());
    assert!(with_codec.cursor_codec().is_some());
    assert!(base.cursor_codec().is_none());
}

#[test]
fn hard_limits_can_only_be_elevated_explicitly() {
    let elevated = QueryLimits {
        max_page_size: 200,
        ..QueryLimits::default()
    };
    let hard = sift_query_core::QueryHardLimits(QueryLimits {
        max_page_size: 200,
        ..QueryLimits::default()
    });
    let policy = QueryPolicy::with_hard_limits(ParamNames::default(), elevated, hard).unwrap();
    assert_eq!(policy.limits().max_page_size, 200);
    assert_eq!(policy.hard_limits().0.max_page_size, 200);
}
