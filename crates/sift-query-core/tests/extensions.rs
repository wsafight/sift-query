mod support;

use sift_query_core::*;
use support::{BaseTyped, RichEntity, TestEntity};

#[test]
fn builtin_filter_identities_survive_source_reorganization() {
    // Frozen pre-refactor identities, used in policy and cache fingerprints.
    assert_eq!(IndexedArrayParamFilter.fingerprint(), 0x4afe_64ff_9f49_928e);
    assert_eq!(JsonArrayParamFilter.fingerprint(), 0x26b8_c646_1b55_e6a3);
    assert_eq!(
        SuffixOperatorParamFilter.fingerprint(),
        0x5897_7f52_a442_ced1
    );
}

fn values<E: SearchEntity>(query: &UserQuery<E>) -> Vec<ScalarValue> {
    query
        .predicate()
        .nodes()
        .iter()
        .filter_map(|node| match node {
            PredicateNode::Condition(condition) => Some(condition.values.clone()),
            _ => None,
        })
        .flatten()
        .collect()
}

#[test]
fn ordered_array_and_suffix_filters_still_use_strict_parsing() {
    let policy = QueryPolicy::default()
        .with_raw_param_filter(IndexedArrayParamFilter)
        .with_raw_param_filter(SuffixOperatorParamFilter);
    let query = ParamParser::new(&policy)
        .unwrap()
        .parse::<RichEntity>(&RawParams::parse("id[]=4&id[]=2&id-op=il&name-ct-ic=AL").unwrap())
        .unwrap();
    assert_eq!(
        values(&query),
        [
            ScalarValue::I64(4),
            ScalarValue::I64(2),
            ScalarValue::Text("AL".to_owned())
        ]
    );
    for bad in [
        "id[1]=2&id-op=il",
        "id[]=2&id[0]=3&id-op=il",
        "name-ct-oops=x",
        "missing[]=1",
    ] {
        assert!(
            ParamParser::new(&policy)
                .unwrap()
                .parse::<RichEntity>(&RawParams::parse(bad).unwrap())
                .is_err(),
            "{bad}"
        );
    }
}

#[test]
fn suffix_filters_preserve_multi_value_conditions_and_reject_conflicting_controls() {
    let policy = QueryPolicy::default().with_raw_param_filter(SuffixOperatorParamFilter);
    let parser = ParamParser::new(&policy).unwrap();
    for (raw, expected) in [
        (
            "id-il=4&id-il=2",
            vec![ScalarValue::I64(4), ScalarValue::I64(2)],
        ),
        (
            "id-bt=2&id-bt=4",
            vec![ScalarValue::I64(2), ScalarValue::I64(4)],
        ),
        (
            "name-ol-ic=AL&name-ol-ic=BO",
            vec![
                ScalarValue::Text("AL".to_owned()),
                ScalarValue::Text("BO".to_owned()),
            ],
        ),
    ] {
        let query = parser
            .parse::<RichEntity>(&RawParams::parse(raw).unwrap())
            .unwrap();
        assert_eq!(values(&query), expected, "{raw}");
    }
    for bad in [
        "id-il=1&id-ni=2",
        "name-ol=x&name-ol-ic=y",
        "id-eq=1&id-eq=2",
        "id-il=1&id-op=il",
        "name-ol-ic=x&name-ic=false",
    ] {
        assert!(
            parser
                .parse::<RichEntity>(&RawParams::parse(bad).unwrap())
                .is_err(),
            "{bad}"
        );
    }
    let query = parser
        .parse::<RichEntity>(&RawParams::parse("a.id-il=1&a.id-il=2&b.id-eq=3&gexpr=a|b").unwrap())
        .unwrap();
    assert_eq!(
        values(&query),
        [
            ScalarValue::I64(1),
            ScalarValue::I64(2),
            ScalarValue::I64(3)
        ]
    );
}

#[test]
fn zero_arity_suffixes_accept_empty_shorthand_but_reject_supplied_values() {
    let policy = QueryPolicy::default().with_raw_param_filter(SuffixOperatorParamFilter);
    let parser = ParamParser::new(&policy).unwrap();
    for raw in [
        "id-nl=", "id-nn=", "id-at=", "id-af=", "name-ey=", "name-ny=",
    ] {
        let query = parser
            .parse::<RichEntity>(&RawParams::parse(raw).unwrap())
            .unwrap();
        assert!(values(&query).is_empty(), "{raw}");
        assert_eq!(
            query
                .predicate()
                .nodes()
                .iter()
                .filter(|node| matches!(node, PredicateNode::Condition(_)))
                .count(),
            1,
            "{raw}"
        );
    }
    for bad in ["id-nl=1", "name-ey=x", "id-nl-ic=", "name-nl-ic="] {
        assert!(
            parser
                .parse::<RichEntity>(&RawParams::parse(bad).unwrap())
                .is_err(),
            "{bad}"
        );
    }
}

#[cfg(feature = "json")]
#[test]
fn json_arrays_expand_scalars_and_reject_malformed_nested_or_empty_values() {
    let policy = QueryPolicy::default().with_raw_param_filter(JsonArrayParamFilter);
    let query = ParamParser::new(&policy)
        .unwrap()
        .parse::<RichEntity>(&RawParams::parse("id=[2,3]&id-op=il").unwrap())
        .unwrap();
    assert_eq!(values(&query), [ScalarValue::I64(2), ScalarValue::I64(3)]);
    for bad in [
        "id=[]&id-op=il",
        "id=[null]",
        "id=[{}]",
        "id=[1,",
        "size=[1]",
    ] {
        assert!(
            ParamParser::new(&policy)
                .unwrap()
                .parse::<RichEntity>(&RawParams::parse(bad).unwrap())
                .is_err(),
            "{bad}"
        );
    }
}

#[test]
fn defaults_fill_missing_parameters_and_configuration_changes_fingerprint() {
    let default = DefaultParamFilter::new([("size".to_owned(), "4".to_owned())]);
    let policy = QueryPolicy::default().with_raw_param_filter(default);
    let parser = ParamParser::new(&policy).unwrap();
    assert_eq!(
        parser
            .parse::<RichEntity>(&RawParams::default())
            .unwrap()
            .paging()
            .size,
        4
    );
    assert_eq!(
        parser
            .parse::<RichEntity>(&RawParams::parse("size=2").unwrap())
            .unwrap()
            .paging()
            .size,
        2
    );
    assert_ne!(
        policy.fingerprint(),
        QueryPolicy::default()
            .with_raw_param_filter(DefaultParamFilter::new([(
                "size".to_owned(),
                "5".to_owned()
            )]))
            .fingerprint()
    );
    let a = QueryPolicy::default()
        .with_raw_param_filter(IndexedArrayParamFilter)
        .with_raw_param_filter(SuffixOperatorParamFilter);
    let b = QueryPolicy::default()
        .with_raw_param_filter(SuffixOperatorParamFilter)
        .with_raw_param_filter(IndexedArrayParamFilter);
    assert_ne!(a.fingerprint(), b.fingerprint());
}

#[test]
fn metadata_validation_cache_separates_entities_and_custom_policy_names() {
    let normal = QueryPolicy::default();
    for _ in 0..2 {
        ParamParser::new(&normal)
            .unwrap()
            .parse::<RichEntity>(&RawParams::default())
            .unwrap();
        ParamParser::new(&normal)
            .unwrap()
            .parse::<BaseTyped>(&RawParams::default())
            .unwrap();
    }
    let conflicting = QueryPolicy::new(
        ParamNames {
            page: "id".to_owned(),
            ..Default::default()
        },
        QueryLimits::default(),
    )
    .unwrap();
    for policy in [&conflicting, &conflicting.clone()] {
        assert!(matches!(
            ParamParser::new(policy)
                .unwrap()
                .parse::<RichEntity>(&RawParams::default()),
            Err(ParamError::ConflictingParameters(_))
        ));
        ParamParser::new(policy)
            .unwrap()
            .parse::<BaseTyped>(&RawParams::default())
            .unwrap();
    }
    ParamParser::new(&normal)
        .unwrap()
        .parse::<RichEntity>(&RawParams::default())
        .unwrap();
}

#[derive(Debug)]
struct RemoveAll;
impl RawParamFilter for RemoveAll {
    fn filter(&self, _: &'static EntityMeta, _: &RawParams) -> Result<RawParams, ParamError> {
        Ok(RawParams::default())
    }
}

#[test]
fn filters_cannot_hide_oversized_original_input_and_chain_has_its_own_budget() {
    let limits = QueryLimits {
        max_raw_query_bytes: 4,
        ..QueryLimits::default()
    };
    let policy = QueryPolicy::new(ParamNames::default(), limits)
        .unwrap()
        .with_raw_param_filter(RemoveAll);
    assert_eq!(
        ParamParser::new(&policy)
            .unwrap()
            .parse::<RichEntity>(&RawParams::parse("id=12345").unwrap())
            .unwrap_err(),
        ParamError::QueryTooLong
    );
    let limits = QueryLimits {
        max_filter_steps: 1,
        ..QueryLimits::default()
    };
    let policy = QueryPolicy::new(ParamNames::default(), limits)
        .unwrap()
        .with_raw_param_filter(RemoveAll)
        .with_raw_param_filter(RemoveAll);
    assert!(matches!(
        ParamParser::new(&policy)
            .unwrap()
            .parse::<RichEntity>(&RawParams::default()),
        Err(ParamError::TooComplex(_))
    ));
}

#[test]
fn normalized_bytes_account_for_url_encoding_and_rpc_preserves_repeats() {
    let raw = RawParams::from_entries(
        [
            ("name".to_owned(), "中文 +&=".to_owned()),
            ("name".to_owned(), "two".to_owned()),
        ],
        &QueryLimits::default(),
    )
    .unwrap();
    let encoded = raw.to_query_string();
    assert_eq!(raw.raw_bytes(), encoded.len());
    assert_eq!(RawParams::parse(&encoded).unwrap().entries(), raw.entries());
    let limits = QueryLimits {
        max_raw_query_bytes: 10,
        ..QueryLimits::default()
    };
    assert_eq!(
        RawParams::from_entries([("a".to_owned(), "中文".to_owned())], &limits).unwrap_err(),
        ParamError::QueryTooLong
    );
}

#[test]
fn enum_codec_round_trips_and_unknown_names_codes_or_duplicate_registration_fail() {
    let codec = EnumCodec::new("status", "count32", [("enabled", 1), ("disabled", 2)]).unwrap();
    let field = BaseTyped::META.resolve_field("count32").unwrap();
    assert_eq!(
        codec.format_result(field, RowValue::I32(1)).unwrap(),
        RowValue::Text("enabled".to_owned())
    );
    assert!(codec.format_result(field, RowValue::I32(7)).is_err());
    let policy = QueryPolicy::default().with_codec(codec.clone());
    let query = ParamParser::new(&policy)
        .unwrap()
        .parse::<BaseTyped>(
            &RawParams::parse("count32=enabled&count32=disabled&count32-op=il").unwrap(),
        )
        .unwrap();
    assert_eq!(values(&query), [ScalarValue::I32(1), ScalarValue::I32(2)]);
    assert!(
        ParamParser::new(&policy)
            .unwrap()
            .parse::<BaseTyped>(&RawParams::parse("count32=").unwrap())
            .is_err()
    );
    let duplicate = policy.with_codec(codec);
    assert!(
        ParamParser::new(&duplicate)
            .unwrap()
            .parse::<BaseTyped>(&RawParams::default())
            .is_err()
    );
    assert!(EnumCodec::new("status", "count32", [("x", 1), ("x", 2)]).is_err());
}

#[derive(Debug)]
struct WrongType;
impl Codec for WrongType {
    fn name(&self) -> &'static str {
        "wrong"
    }
    fn supports(&self, field: &FieldMeta) -> bool {
        field.public_name == "id"
    }
    fn parse(&self, _: &FieldMeta, _: &str) -> Result<ScalarValue, String> {
        Ok(ScalarValue::Text("oops".to_owned()))
    }
}

#[test]
fn codec_cannot_bypass_operator_or_field_type_checks() {
    let policy = QueryPolicy::default().with_codec(WrongType);
    for input in ["id=1", "id=1&id-op=ct"] {
        assert!(
            ParamParser::new(&policy)
                .unwrap()
                .parse::<RichEntity>(&RawParams::parse(input).unwrap())
                .is_err()
        );
    }
}

#[test]
fn boolean_codec_is_strict_and_has_a_stable_public_representation() {
    let codec = BooleanCodec {
        name: "flag",
        field: "flag",
        true_name: "yes",
        false_name: "no",
    };
    let field = BaseTyped::META.resolve_field("flag").unwrap();
    assert_eq!(codec.parse(field, "yes").unwrap(), ScalarValue::Bool(true));
    assert_eq!(
        codec.format_result(field, RowValue::Bool(false)).unwrap(),
        RowValue::Text("no".to_owned())
    );
    assert!(codec.parse(field, "").is_err());
}

#[test]
fn group_scope_stays_separate_from_client_or_and_has_limits() {
    let policy = QueryPolicy::default();
    let query = ParamParser::new(&policy)
        .unwrap()
        .parse::<TestEntity>(&RawParams::parse("a.score=1&b.score=2&gexpr=a|b").unwrap())
        .unwrap();
    let field = FieldRef::<TestEntity, i32, AggregateKind>::new(FieldId(2));
    let scope = GroupScope::new().ge(field, 10).build().unwrap();
    let plan = QueryPlan::new_with_scopes(
        &query,
        &ServerScope::unrestricted(),
        &scope,
        QueryTarget::MapPage,
    )
    .unwrap();
    assert!(plan.group_predicate.root().is_some());
    assert_eq!(plan.server_group_predicate, *scope.predicate());
    assert!(GroupScope::<TestEntity>::new().build().is_err());
    assert!(
        GroupScope::builder(ScopeLimits {
            max_conditions: 0,
            max_logical_values: 1
        })
        .ge(field, 1)
        .build()
        .is_err()
    );
    let scalar = FieldRef::<RichEntity, i64, AggregateKind>::new(FieldId(0));
    assert!(GroupScope::new().eq(scalar, 1).build().is_err());
}

#[cfg(feature = "chrono")]
#[test]
fn date_codec_has_no_implicit_timezone_and_rejects_empty_dates() {
    let field = support::ChronoTyped::META.resolve_field("day").unwrap();
    let codec = DateCodec {
        name: "date",
        field: "day",
        format: "%d/%m/%Y",
    };
    let value = codec.parse(field, "09/10/2026").unwrap();
    assert_eq!(
        value,
        ScalarValue::Date(chrono::NaiveDate::from_ymd_opt(2026, 10, 9).unwrap())
    );
    assert!(codec.parse(field, "").is_err());
}
