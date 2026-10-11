#![no_main]
use libfuzzer_sys::fuzz_target;
use sift_query_core::*;
mod common;
fuzz_target!(|data: &[u8]| {
    if let Ok(input) = std::str::from_utf8(data) {
        if let Ok(raw) = RawParams::parse(input) {
            assert_eq!(
                RawParams::parse(&raw.to_query_string()).unwrap().entries(),
                raw.entries()
            );
            let policy = QueryPolicy::default();
            let _ = ParamParser::new(&policy)
                .unwrap()
                .parse::<common::Entity>(&raw);
        }
    }
});
