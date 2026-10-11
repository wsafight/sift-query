mod support;
use sift_query_core::*;
use support::*;

fn text_field() -> &'static FieldMeta {
    BaseTyped::META.resolve_field("title").unwrap()
}

struct ExtensionEntity;

impl SearchEntity for ExtensionEntity {
    const META: &'static EntityMeta = BaseTyped::META;

    fn custom_operator_names(field: FieldId) -> &'static [&'static str] {
        if field == FieldId(2) { &["near"] } else { &[] }
    }
}

#[test]
fn scalar_text_and_field_convertor_keep_typed_values() {
    let field = text_field();
    let codec = StringCodec {
        name: "trim_title",
        field: "title",
        trim: true,
    };
    let _: &dyn FieldConvertor = &codec;
    assert_eq!(
        codec.parse(field, "  hello ").unwrap(),
        ScalarValue::Text("hello".into())
    );
    assert_eq!(
        codec
            .format_result(field, RowValue::Text("  hello ".into()))
            .unwrap(),
        RowValue::Text("hello".into())
    );
}

#[test]
fn controlled_server_sql_extensions_are_bounded_and_registry_only() {
    let op = CustomOperator::typed(
        "near",
        "abs({field} - {0}) < 1",
        1,
        ValueType::I64,
        ValueType::I64,
    )
    .unwrap();
    let extension = ServerSqlExtension::new(op, ValueType::I64, &["count64"]).unwrap();
    let mut registry = ServerSqlRegistry::default();
    registry.register(extension).unwrap();
    assert_eq!(
        registry.get("near").unwrap().operator.template(),
        "abs({field} - {0}) < 1"
    );
    assert!(registry.get("client_sql").is_none());
    let malformed = ServerSqlExtension {
        operator: op,
        input_type: ValueType::I64,
        fields: &["field-name"],
    };
    assert!(ServerSqlRegistry::default().register(malformed).is_err());
    assert!(CustomOperator::new("too_long", "{field} = {0}", 1).is_ok());
    let mut limited = ServerSqlRegistry::with_limit(1).unwrap();
    limited.register(extension).unwrap();
    assert!(limited.register(extension).is_err());

    let policy = QueryPolicy::default()
        .with_server_sql_registry::<ExtensionEntity>(&registry)
        .unwrap();
    let parsed = ParamParser::new(&policy)
        .unwrap()
        .parse::<ExtensionEntity>(&RawParams::parse("count64=41&count64-op=near").unwrap())
        .unwrap();
    assert_eq!(parsed.predicate().nodes().len(), 1);
}

#[test]
fn separated_results_keep_input_scalar_and_enforce_types_nulls_and_budget() {
    let field = text_field();
    let codec = ListResultCodec::new("tags", "title", "|", ValueType::I64, 2).unwrap();
    assert!(codec.supports(field));
    assert_eq!(
        codec.parse(field, "1|2").unwrap(),
        ScalarValue::Text("1|2".into())
    );
    assert_eq!(
        codec.format_result(field, RowValue::Null).unwrap(),
        RowValue::Null
    );
    assert_eq!(
        codec
            .format_result(field, RowValue::Text(" ".into()))
            .unwrap(),
        RowValue::List(vec![])
    );
    assert_eq!(
        codec
            .format_result(field, RowValue::Text("1|9007199254740993".into()))
            .unwrap(),
        RowValue::List(vec![RowValue::I64(1), RowValue::I64(9_007_199_254_740_993)])
    );
    for input in ["1|2|3", "1|", "bad"] {
        assert!(
            codec
                .format_result(field, RowValue::Text(input.into()))
                .is_err()
        );
    }
    assert!(codec.format_result(field, RowValue::I64(1)).is_err());
    assert!(ListResultCodec::new("tags", "title", "", ValueType::Text, 2).is_err());
    assert!(ListResultCodec::new("tags", "title", "|", ValueType::Text, 0).is_err());
    assert_ne!(
        codec.fingerprint(),
        ListResultCodec::new("tags", "title", ",", ValueType::I64, 2)
            .unwrap()
            .fingerprint()
    );
    let policy = QueryPolicy::default().with_codec(codec);
    let query = ParamParser::new(&policy)
        .unwrap()
        .parse::<BaseTyped>(&RawParams::parse("title=1%7C2").unwrap())
        .unwrap();
    let raw = query.to_rpc(&policy).unwrap();
    assert!(raw.entries().iter().any(|(_, value)| value == "1|2"));
}

#[cfg(feature = "serde")]
#[test]
fn lists_serialize_large_integers_and_escape_text_consistently_with_csv() {
    let list = RowValue::List(vec![
        RowValue::I64(9_007_199_254_740_993),
        RowValue::I32(2),
        RowValue::Text("a\"\\\n\t\u{0001}".into()),
        RowValue::Null,
        RowValue::List(vec![RowValue::Bool(true)]),
    ]);
    assert_eq!(
        serde_json::to_value(&list).unwrap(),
        serde_json::from_str::<serde_json::Value>(&list.to_external_text()).unwrap()
    );
    assert_eq!(serde_json::to_value(&list).unwrap()[0], "9007199254740993");
    assert!(CursorValue::from_row_value(&list).is_err());
}

#[cfg(feature = "chrono")]
#[test]
fn timezone_conversion_uses_explicit_instants_and_rejects_dst_ambiguity() {
    let field = ChronoTyped::META.resolve_field("instant").unwrap();
    let codec =
        TimestampCodec::new("zoned", "instant", "%Y/%m/%d %H:%M:%S", "Asia/Shanghai").unwrap();
    let expected = codec.parse(field, "2026-10-10T00:00:00Z").unwrap();
    assert_eq!(codec.parse(field, "2026/10/10 08:00:00").unwrap(), expected);
    assert_eq!(
        codec.parse(field, "2026-10-10T08:00:00+08:00").unwrap(),
        expected
    );
    let compact = TimestampCodec::new("zoned", "instant", "%Y%m%d%H%M%S", "Asia/Shanghai").unwrap();
    assert_eq!(compact.parse(field, "20261010080000").unwrap(), expected);
    let offset =
        TimestampCodec::new("zoned", "instant", "%Y/%m/%d %H:%M:%S %z", "Asia/Shanghai").unwrap();
    assert_eq!(
        offset.parse(field, "2026/10/10 00:00:00 +0000").unwrap(),
        expected
    );
    assert_eq!(
        codec.format_result(field, expected.clone().into()).unwrap(),
        RowValue::Text("2026/10/10 08:00:00".into())
    );
    assert_eq!(
        codec.parse(field, "0").unwrap().to_external_text(),
        "1970-01-01T00:00:00+00:00"
    );
    assert!(codec.parse(field, "").is_err());
    assert!(codec.parse(field, "9223372036854775807").is_err());
    let dst =
        TimestampCodec::new("zoned", "instant", "%Y/%m/%d %H:%M:%S", "America/New_York").unwrap();
    for input in ["2026/03/08 02:30:00", "2026/11/01 01:30:00"] {
        assert!(dst.parse(field, input).is_err(), "{input}");
    }
    assert!(dst.parse(field, "2026-11-01T01:30:00-04:00").is_ok());
    assert_ne!(codec.fingerprint(), dst.fingerprint());
    assert!(TimestampCodec::new("zoned", "instant", "%Q", "UTC").is_err());
    assert!(TimestampCodec::new("zoned", "instant", "%F %T", "unknown").is_err());
    assert_eq!(
        codec.format_result(field, RowValue::Null).unwrap(),
        RowValue::Null
    );
}

#[cfg(feature = "chrono")]
#[test]
fn rpc_preserves_temporal_precision_independent_of_display_format() {
    let policy = QueryPolicy::default()
        .with_codec(DateCodec {
            name: "day",
            field: "day",
            format: "%Y/%m/%d",
        })
        .with_codec(TimeCodec {
            name: "clock",
            field: "clock",
            format: "%H:%M",
        })
        .with_codec(DateTimeCodec {
            name: "local",
            field: "localAt",
            format: "%Y/%m/%d %H:%M",
        })
        .with_codec(
            TimestampCodec::new("instant", "instant", "%Y/%m/%d %H:%M", "Asia/Shanghai").unwrap(),
        );
    let query = ParamParser::new(&policy).unwrap().parse::<ChronoTyped>(&RawParams::parse("day=2026%2F10%2F10&clock=01:02:03.123456789&localAt=2026-10-10T01:02:03.123456789&instant=2026-10-10T01:02:03.123456789Z").unwrap()).unwrap();
    let raw = query.to_rpc(&policy).unwrap();
    let decoded = ParamParser::new(&policy)
        .unwrap()
        .parse::<ChronoTyped>(&raw)
        .unwrap();
    let conditions = |query: &UserQuery<ChronoTyped>| {
        query
            .predicate()
            .nodes()
            .iter()
            .filter_map(|node| match node {
                PredicateNode::Condition(condition) => Some(condition.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(conditions(&query), conditions(&decoded));
    for field in ChronoTyped::META.fields {
        let codec = policy.codec_for::<ChronoTyped>(field).unwrap();
        assert_eq!(
            codec.format_result(field, RowValue::Null).unwrap(),
            RowValue::Null
        );
        assert!(codec.parse(field, "").is_err());
    }
}
