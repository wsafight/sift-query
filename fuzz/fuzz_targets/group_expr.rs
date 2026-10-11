#![no_main]
use libfuzzer_sys::fuzz_target;
use sift_query_core::*;
mod common;
fuzz_target!(|data: &str| {
    let policy = QueryPolicy::default();
    let entries = [("a.id", "1"), ("b.id", "2"), ("c.id", "3"), ("gexpr", data)];
    if let Ok(raw) = RawParams::from_entries(
        entries
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value.to_owned())),
        policy.limits(),
    ) {
        if let Ok(query) = ParamParser::new(&policy)
            .unwrap()
            .parse::<common::Entity>(&raw)
        {
            assert!(
                QueryPlan::new(&query, &ServerScope::unrestricted(), QueryTarget::MapPage).is_ok()
            );
        }
    }
});
