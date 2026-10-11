#![no_main]
use libfuzzer_sys::fuzz_target;
use sift_query_core::*;
mod common;
fuzz_target!(|input: &str| {
    let policy = QueryPolicy::default();
    if let Ok(query) = rsql::parse::<common::Entity>(input, &policy) {
        assert!(QueryPlan::new(&query, &ServerScope::unrestricted(), QueryTarget::MapPage).is_ok());
    }
});
