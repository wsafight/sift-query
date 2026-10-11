use sift_query_core::{ParamError, RawParams};

#[test]
fn preserves_order_and_duplicates() {
    let raw = RawParams::parse("status=1&status=2&name=Alice+Smith").unwrap();
    assert_eq!(
        raw.entries(),
        &[
            ("status".to_owned(), "1".to_owned()),
            ("status".to_owned(), "2".to_owned()),
            ("name".to_owned(), "Alice Smith".to_owned()),
        ]
    );
}

#[test]
fn decodes_percent_encoding_strictly() {
    let raw = RawParams::parse("gexpr=%28A%7CB%29%26C").unwrap();
    assert_eq!(raw.entries()[0].1, "(A|B)&C");
    assert_eq!(
        RawParams::parse("name=%GG"),
        Err(ParamError::InvalidEncoding)
    );
}

#[test]
fn rejects_empty_items() {
    assert!(matches!(
        RawParams::parse("a=1&&b=2"),
        Err(ParamError::InvalidName(_))
    ));
}

// ---------------------------------------------------------------------------
// Limits, decoding, and naming helpers
// ---------------------------------------------------------------------------

fn parse_with_limits(
    query: &str,
    limits: &sift_query_core::QueryLimits,
) -> Result<RawParams, ParamError> {
    RawParams::parse_with_limits(query, limits)
}

#[test]
fn a_query_longer_than_the_configured_limit_is_rejected() {
    let limits = sift_query_core::QueryLimits {
        max_raw_query_bytes: 3,
        ..sift_query_core::QueryLimits::default()
    };
    assert_eq!(
        parse_with_limits("abcd", &limits),
        Err(ParamError::QueryTooLong)
    );
    assert!(parse_with_limits("abc", &limits).is_ok());
}

#[test]
fn the_item_count_limit_is_enforced_during_parsing() {
    let limits = sift_query_core::QueryLimits {
        max_raw_items: 1,
        ..sift_query_core::QueryLimits::default()
    };
    assert!(parse_with_limits("a=1", &limits).is_ok());
    assert_eq!(
        parse_with_limits("a=1&b=2", &limits),
        Err(ParamError::TooComplex("raw parameter item count"))
    );
}

#[test]
fn an_empty_key_or_an_oversized_key_is_rejected() {
    assert_eq!(
        RawParams::parse("=1"),
        Err(ParamError::InvalidName(String::new()))
    );

    let limits = sift_query_core::QueryLimits {
        max_key_bytes: 2,
        ..sift_query_core::QueryLimits::default()
    };
    assert_eq!(
        parse_with_limits("abc=1", &limits),
        Err(ParamError::InvalidName("abc".to_owned()))
    );
}

#[test]
fn an_oversized_value_is_rejected() {
    let limits = sift_query_core::QueryLimits {
        max_value_bytes: 2,
        ..sift_query_core::QueryLimits::default()
    };
    assert_eq!(
        parse_with_limits("a=abc", &limits),
        Err(ParamError::TooComplex("single parameter value length"))
    );
}

#[test]
fn the_decoded_byte_budget_is_enforced() {
    let limits = sift_query_core::QueryLimits {
        max_decoded_bytes: 4,
        ..sift_query_core::QueryLimits::default()
    };
    // "ab=cd" decodes to four bytes, which fits; one more does not.
    assert!(parse_with_limits("ab=cd", &limits).is_ok());
    assert_eq!(
        parse_with_limits("ab=cde", &limits),
        Err(ParamError::TooComplex("decoded parameter bytes"))
    );
}

#[test]
fn a_truncated_percent_escape_is_rejected() {
    assert_eq!(RawParams::parse("a=%2"), Err(ParamError::InvalidEncoding));
    assert_eq!(RawParams::parse("a=%"), Err(ParamError::InvalidEncoding));
}

#[test]
fn lowercase_hex_digits_are_accepted() {
    let raw = RawParams::parse("a=%2f").unwrap();
    assert_eq!(raw.entries()[0].1, "/");
    assert_eq!(RawParams::parse("a=%2F").unwrap().entries()[0].1, "/");
}

#[test]
fn empty_parameter_sets_report_their_emptiness() {
    let empty = RawParams::parse("").unwrap();
    assert!(empty.is_empty());
    assert_eq!(empty.len(), 0);
    assert_eq!(empty.raw_bytes(), 0);
    assert_eq!(empty.decoded_bytes(), 0);

    let single = RawParams::parse("a=1").unwrap();
    assert!(!single.is_empty());
    assert_eq!(single.len(), 1);
    assert_eq!(single.raw_bytes(), 3);
    assert_eq!(single.decoded_bytes(), 2);
}
