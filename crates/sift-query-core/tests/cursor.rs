mod support;

use sift_query_core::{
    CursorCodec, CursorValue, FieldId, PagingMode, ParamError, ParamParser, QueryPolicy, RawParams,
    ScalarValue,
};

use support::CursorEntity;

struct NonNullEntity;
impl sift_query_core::SearchEntity for NonNullEntity {
    const META: &'static sift_query_core::EntityMeta = CursorEntity::META;
    fn cursor_key_non_null(_: FieldId) -> bool {
        true
    }
}

#[test]
fn explicit_non_null_keys_reject_null_tokens_and_programmatic_plans() {
    let codec = codec();
    let policy = QueryPolicy::default().with_cursor_codec(codec.clone());
    let query = ParamParser::new(&policy)
        .unwrap()
        .parse::<NonNullEntity>(&RawParams::parse("cursor=&size=2").unwrap())
        .unwrap();
    assert!(
        codec
            .encode::<NonNullEntity>(query.order(), &[CursorValue::Null])
            .is_err()
    );
    // Even a correctly signed token with the same schema cannot override the assertion.
    let token = codec
        .encode::<CursorEntity>(query.order(), &[CursorValue::Null])
        .unwrap();
    assert!(
        codec
            .decode::<NonNullEntity>(&token, query.order())
            .is_err()
    );
    let cursor = codec.decode::<CursorEntity>(&token, query.order()).unwrap();
    let programmatic = sift_query_core::QueryBuilder::<NonNullEntity>::new(&policy)
        .cursor(cursor, 2)
        .build();
    assert!(programmatic.is_err());
}

fn codec() -> CursorCodec {
    CursorCodec::new([0x5a; 32]).unwrap()
}

#[test]
fn signed_cursor_round_trips_and_rejects_tampering() {
    let policy = QueryPolicy::default();
    let base = ParamParser::new(&policy)
        .unwrap()
        .parse::<CursorEntity>(&RawParams::parse("").unwrap())
        .unwrap();
    let token = codec()
        .encode::<CursorEntity>(base.order(), &[CursorValue::Scalar(ScalarValue::I64(42))])
        .unwrap();
    let decoded = codec()
        .decode::<CursorEntity>(&token, base.order())
        .unwrap();
    assert_eq!(
        decoded.values(),
        &[CursorValue::Scalar(ScalarValue::I64(42))]
    );

    let mut tampered = token;
    tampered.push('A');
    assert!(matches!(
        codec().decode::<CursorEntity>(&tampered, base.order()),
        Err(ParamError::InvalidValue { .. })
    ));
}

#[test]
fn parser_supports_first_and_following_cursor_pages() {
    let codec = codec();
    let policy = QueryPolicy::default().with_cursor_codec(codec.clone());
    let first = ParamParser::new(&policy)
        .unwrap()
        .parse::<CursorEntity>(&RawParams::parse("cursor=&size=10").unwrap())
        .unwrap();
    assert!(matches!(first.paging().mode, PagingMode::Cursor(None)));

    let token = codec
        .encode::<CursorEntity>(first.order(), &[CursorValue::Scalar(ScalarValue::I64(7))])
        .unwrap();
    let next = ParamParser::new(&policy)
        .unwrap()
        .parse::<CursorEntity>(&RawParams::parse(&format!("cursor={token}&size=10")).unwrap())
        .unwrap();
    assert!(matches!(next.paging().mode, PagingMode::Cursor(Some(_))));
}
