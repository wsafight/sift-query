mod support;
use proptest::prelude::*;
use sift_query_core::*;
use support::{CursorEntity, TestEntity};

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]
    #[test]
    fn unicode_and_repeated_raw_values_survive_encoding(entries in prop::collection::vec(("[A-Za-z][A-Za-z0-9_]{0,10}", ".{0,40}"), 0..20)) {
        let raw = RawParams::from_entries(entries.clone(), &QueryLimits::default()).unwrap();
        let decoded = RawParams::parse(&raw.to_query_string()).unwrap();
        prop_assert_eq!(decoded.entries(), entries.as_slice());
    }

    #[test]
    fn arbitrary_parameter_bytes_fail_or_produce_valid_bounded_queries(input in prop::collection::vec(any::<u8>(), 0..4096)) {
        if let Ok(input) = std::str::from_utf8(&input) && let Ok(raw) = RawParams::parse(input) {
                prop_assert!(raw.len() <= QueryLimits::default().max_raw_items);
                if let Ok(query) = ParamParser::new(&QueryPolicy::default()).unwrap().parse::<TestEntity>(&raw) {
                    prop_assert!(QueryPlan::new(&query, &ServerScope::unrestricted(), QueryTarget::MapPage).is_ok());
                }
        }
    }

    #[test]
    fn nested_rpc_preserves_query_semantics(a in any::<i64>(), b in any::<i64>(), c in any::<i64>()) {
        let policy = QueryPolicy::default();
        let parser = ParamParser::new(&policy).unwrap();
        let raw = RawParams::parse(&format!("a.id={a}&b.id={b}&c.id={c}&gexpr=(a|b)%26c")).unwrap();
        let query = parser.parse::<CursorEntity>(&raw).unwrap();
        let encoded = query.to_rpc(&policy).unwrap();
        let decoded = parser.parse::<CursorEntity>(&encoded).unwrap();
        prop_assert_eq!(query.predicate(), decoded.predicate());
        prop_assert_eq!(query.paging(), decoded.paging());
    }

    #[test]
    fn signed_cursor_roundtrips_and_changed_signatures_are_rejected(value in any::<i64>(), byte in 0usize..32, mask in 1u8..=255) {
        use base64::Engine;
        let codec = CursorCodec::new([42; 32]).unwrap();
        let order = vec![OrderBy::from(CursorEntity::META.stable_order[0])];
        let values = [CursorValue::Scalar(ScalarValue::I64(value))];
        let token = codec.encode::<CursorEntity>(&order, &values).unwrap();
        let decoded = codec.decode::<CursorEntity>(&token, &order).unwrap();
        prop_assert_eq!(decoded.values(), &values);
        let mut bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(&token).unwrap();
        let position = bytes.len() - 32 + byte;
        bytes[position] ^= mask;
        let altered = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes);
        prop_assert!(codec.decode::<CursorEntity>(&altered, &order).is_err());
    }
}
