use super::*;

// ---------------------------------------------------------------------------
// Paging
// ---------------------------------------------------------------------------

#[test]
fn paging_parameters_cover_page_offset_and_defaults() {
    let default = parse_rich("").unwrap().paging();
    assert_eq!(default.size, 15);
    assert_eq!(default.offset, 0);
    assert_eq!(default.mode, sift_query_core::PagingMode::Page { page: 1 });
    assert!(!default.explicit_page);
    assert!(!default.explicit_size);

    let paged = parse_rich("page=3&size=4").unwrap().paging();
    assert_eq!(paged.offset, 8);
    assert!(paged.explicit_page);
    assert!(paged.explicit_size);

    let offset = parse_rich("offset=7&size=4").unwrap().paging();
    assert_eq!(offset.mode, sift_query_core::PagingMode::Offset);
    assert_eq!(offset.offset, 7);
    assert!(offset.explicit_offset);
    assert!(!offset.explicit_page);
}

#[test]
fn paging_parameters_reject_bad_input() {
    assert_eq!(
        rich_error("page=1&offset=1"),
        ParamError::ConflictingParameters(
            "page, offset, and cursor are mutually exclusive".to_owned()
        )
    );
    assert_eq!(
        rich_error("page=abc"),
        ParamError::InvalidValue {
            field: "page".to_owned(),
            reason: "expected a non-negative integer".to_owned(),
        }
    );
    assert_eq!(
        rich_error("page=0"),
        ParamError::PagingOutOfRange("page".to_owned())
    );
    assert_eq!(
        rich_error("size=101"),
        ParamError::PagingOutOfRange("size".to_owned())
    );
    assert_eq!(
        rich_error("offset=20001"),
        ParamError::PagingOutOfRange("offset".to_owned())
    );
    assert_eq!(
        rich_error("page=18446744073709551615&size=100"),
        ParamError::PagingOutOfRange("page offset overflow".to_owned())
    );
    assert_eq!(
        rich_error("size=abc"),
        ParamError::InvalidValue {
            field: "size".to_owned(),
            reason: "expected a non-negative integer".to_owned(),
        }
    );
}

#[test]
fn cursor_parameters_require_a_configured_codec() {
    assert_eq!(
        rich_error("cursor="),
        ParamError::InvalidValue {
            field: "cursor".to_owned(),
            reason: "cursor pagination is not configured".to_owned(),
        }
    );
    assert_eq!(
        rich_error("cursor=&page=1"),
        ParamError::ConflictingParameters(
            "page, offset, and cursor are mutually exclusive".to_owned()
        )
    );

    let policy = with_limits(|limits| limits.max_cursor_bytes = 4);
    let error = rich_with(&policy, "cursor=AAAAAAAAAAAAAAAAAAAAAAAA").unwrap_err();
    assert_eq!(error, ParamError::TooComplex("cursor length"));
}

#[test]
fn cursor_values_must_match_the_ordering_field_types() {
    let codec = sift_query_core::CursorCodec::new([0x31; 32]).unwrap();
    let policy = QueryPolicy::default().with_cursor_codec(codec.clone());
    let base = rich_with(&policy, "cursor=&size=2").unwrap();
    let order = base.order().to_vec();

    // The ordering is `id`, an i64 column, but the cursor carries text.
    let token = codec
        .encode::<RichEntity>(
            &order,
            &[sift_query_core::CursorValue::Scalar(
                sift_query_core::ScalarValue::Text("x".to_owned()),
            )],
        )
        .unwrap();
    assert_eq!(
        rich_with(&policy, &format!("cursor={token}&size=2")).unwrap_err(),
        ParamError::InvalidValue {
            field: "cursor".to_owned(),
            reason: "cursor value type does not match ordering field id".to_owned(),
        }
    );
}

#[test]
fn bean_searcher_compatibility_downgrades_paging_errors_to_an_empty_page() {
    let compatible = QueryPolicy::bean_searcher_4812();
    let query = rich_with(&compatible, "page=0").unwrap();
    assert!(query.is_forced_empty());
    assert_eq!(query.paging().offset, 0);

    // Non-paging errors are still reported.
    assert!(rich_with(&compatible, "id=abc").is_err());
}
