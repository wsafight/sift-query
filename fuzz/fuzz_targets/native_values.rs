#![no_main]
use libfuzzer_sys::fuzz_target;
use sift_query_core::*;
mod common;
macro_rules! entity {
    ($name:ident,$type:ident) => {
        struct $name;
        impl SearchEntity for $name {
            const META: &'static EntityMeta = &EntityMeta {
                fields: &[FieldMeta {
                    value_type: ValueType::$type,
                    sortable: false,
                    ..common::Entity::META.fields[0]
                }],
                ..*common::Entity::META
            };
        }
    };
}
entity!(ArrayI64, ArrayI64);
entity!(ArrayText, ArrayText);
entity!(Range, Int8Range);
entity!(Network, Network);
entity!(Json, Json);
fn check<E: SearchEntity>(raw: &RawParams, policy: &QueryPolicy) {
    if let Ok(query) = ParamParser::new(policy).unwrap().parse::<E>(raw) {
        assert!(QueryPlan::new(&query, &ServerScope::unrestricted(), QueryTarget::MapPage).is_ok());
        let encoded = query.to_rpc(policy).unwrap();
        assert_eq!(
            ParamParser::new(policy)
                .unwrap()
                .parse::<E>(&encoded)
                .unwrap()
                .predicate(),
            query.predicate()
        );
    }
}
fuzz_target!(|input: &str| {
    let policy = QueryPolicy::default();
    if let Ok(raw) = RawParams::from_entries([("id".to_owned(), input.to_owned())], policy.limits())
    {
        check::<ArrayI64>(&raw, &policy);
        check::<ArrayText>(&raw, &policy);
        check::<Range>(&raw, &policy);
        check::<Network>(&raw, &policy);
        check::<Json>(&raw, &policy);
    }
});
