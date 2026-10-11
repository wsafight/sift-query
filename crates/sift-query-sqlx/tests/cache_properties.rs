#[path = "renderer_coverage/support.rs"]
#[allow(unused_imports, dead_code)]
mod support;
use proptest::prelude::*;
use sift_query_core::*;
use sift_query_sqlx::{PgRenderer, QueryShapeCache};
use support::User;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]
    #[test]
    fn cached_templates_always_bind_current_values(ids in prop::collection::vec(any::<i64>(), 1..16), offset in 0u64..20_000, size in 1u32..=100) {
        let policy = QueryPolicy::default();
        let raw = RawParams::from_entries(ids.iter().map(|id| ("id".to_owned(), id.to_string())).chain([
            ("id-op".to_owned(), "il".to_owned()),
            ("offset".to_owned(), offset.to_string()),
            ("size".to_owned(), size.to_string()),
        ]), policy.limits()).unwrap();
        let query = ParamParser::new(&policy).unwrap().parse::<User>(&raw).unwrap();
        let scope = ServerScope::unrestricted();
        let plan = QueryPlan::new(&query, &scope, QueryTarget::MapPage).unwrap();
        let cache = QueryShapeCache::new(4).unwrap();
        let cold = support::plan::<User>("id=0&id-op=il", QueryTarget::MapPage);
        cache.render_list(&cold).unwrap();
        let expected = PgRenderer.render_list(&plan).unwrap();
        let (actual, _) = cache.render_list(&plan).unwrap();
        prop_assert_eq!(&actual.sql, &expected.sql);
        prop_assert_eq!(&actual.binds, &expected.binds);
        let (warm, hit) = cache.render_list(&plan).unwrap();
        prop_assert!(hit);
        prop_assert_eq!(warm.binds, expected.binds);
    }
}
