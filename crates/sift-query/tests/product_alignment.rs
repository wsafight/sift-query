#![allow(dead_code)]
use sift_query::*;

search_entities! {
    fields Identity {
        #[search_field(sortable)]
        id: i64,
    }
    fields Common {
        include Identity;
        #[search_field(codec = "status", custom_operators = ["statusAbove"])]
        status: i32,
    }
    mapping Users {
        #[search_entity(from = "app_user u", column_prefix = "u.", default_order = "id:asc", stable_order = "id:asc")]
    }
    #[derive(Debug, SearchEntity)]
    #[search_mapping(Users)]
    struct User { include Common; name: String }
    #[derive(Debug, SearchEntity)]
    #[search_mapping(Users)]
    struct UserDto { include Common; #[search_field(column = "upper(u.name)")] name: String }
    mapping Naming {
        #[search_entity(table_prefix = "app_", strip_suffix = "Dto", column_prefix = "u.", column_case = "upper_snake", stable_order = "id:asc")]
    }
    #[derive(Debug, SearchEntity)]
    #[search_mapping(Naming)]
    struct AuditDto { include Identity; first_name: String }
    #[derive(Debug, SearchEntity, sqlx::FromRow)]
    #[search_mapping(Naming)]
    #[search_entity(from = "actual a", column_prefix = "a.", column_case = "verbatim")]
    struct AuditOverride {
        include Identity;
        #[sqlx(rename = "db_name")]
        #[search_field(name = "display", result_alias = "db_name")]
        first_name: String
    }
    #[derive(Debug, SearchEntity)]
    #[search_entity(
        from = "app_user u",
        joins = ["JOIN organization o ON o.id = u.org_id"],
        stable_order = "id:asc"
    )]
    struct UserReadModel {
        #[search_field(column = "u.id", sortable)]
        id: i64,
        #[search_field(column = "o.name")]
        organization_name: String,
    }
}

fn policy() -> QueryPolicy {
    QueryPolicy::default()
        .with_codec(EnumCodec::new("status", "status", [("enabled", 1), ("disabled", 2)]).unwrap())
        .with_custom_operator(
            CustomOperator::new("statusAbove", "{field} > {0}", 1).unwrap(),
            ValueType::I32,
        )
        .unwrap()
}

#[test]
fn reusable_mapping_retains_typed_fields_aliases_and_distinct_fingerprints() {
    assert_eq!(User::META.fields[0].column, "u.id");
    assert_eq!(UserDto::META.fields[1].column, "u.status");
    assert_eq!(UserDto::META.fields[2].column, "upper(u.name)");
    assert_ne!(
        User::META.schema_fingerprint,
        UserDto::META.schema_fingerprint
    );
    assert_eq!(
        User::custom_operator_names(UserFields::STATUS.id()),
        &["statusAbove"]
    );
    assert_eq!(AuditDto::META.from, "app_audit");
    assert_eq!(AuditDto::META.fields[1].column, "u.FIRST_NAME");
    assert_eq!(AuditOverride::META.from, "actual a");
    assert_eq!(AuditOverride::META.fields[1].column, "a.first_name");
    assert_eq!(AuditOverride::META.fields[1].public_name, "display");
    assert_eq!(AuditOverride::META.fields[1].result_alias, Some("db_name"));
}

#[test]
fn read_model_joins_are_server_owned_and_enter_the_schema_contract() {
    assert_eq!(
        UserReadModel::META.from,
        "app_user u JOIN organization o ON o.id = u.org_id"
    );
    assert_ne!(
        UserReadModel::META.schema_fingerprint,
        User::META.schema_fingerprint
    );
}

#[test]
fn registered_http_operator_and_codec_rpc_match_programmatic_values() {
    let policy = policy();
    let operator = policy.registered_operator("statusAbove").unwrap().operator;
    let query = QueryBuilder::<User>::new(&policy)
        .custom(UserFields::STATUS, operator, [1_i32])
        .build()
        .unwrap();
    let raw = query.to_rpc(&policy).unwrap();
    assert!(raw.entries().iter().any(|(_, value)| value == "enabled"));
    let decoded = ParamParser::new(&policy)
        .unwrap()
        .parse::<User>(&raw)
        .unwrap();
    assert_eq!(query.predicate(), decoded.predicate());
    assert!(
        ParamParser::new(&policy)
            .unwrap()
            .parse::<User>(&RawParams::parse("name=x&name-op=statusAbove").unwrap())
            .is_err()
    );
    assert!(
        ParamParser::new(&QueryPolicy::default())
            .unwrap()
            .parse::<User>(&RawParams::default())
            .is_err()
    );
    assert!(
        policy
            .clone()
            .with_custom_operator(operator, ValueType::I32)
            .is_err()
    );
    assert!(
        QueryPolicy::default()
            .with_custom_operator(
                CustomOperator::new("eq", "{field} = {0}", 1).unwrap(),
                ValueType::I32
            )
            .is_err()
    );
    let changed = QueryPolicy::default()
        .with_codec(EnumCodec::new("status", "status", [("enabled", 1), ("disabled", 2)]).unwrap())
        .with_custom_operator(
            CustomOperator::new("statusAbove", "{field} >= {0}", 1).unwrap(),
            ValueType::I32,
        )
        .unwrap();
    assert_ne!(policy.fingerprint(), changed.fingerprint());
    assert!(query.to_rpc(&changed).is_err());
}

#[derive(Debug, SearchEntity)]
#[search_entity(stable_order = "id:asc")]
struct OperatorCases {
    #[search_field(sortable, custom_operators = ["nonzero", "within"])]
    id: i64,
}

#[test]
fn http_custom_operators_enforce_arity_types_whitelist_and_safe_templates() {
    let policy = QueryPolicy::default()
        .with_custom_operator(
            CustomOperator::new("nonzero", "{field} <> 0", 0).unwrap(),
            ValueType::I64,
        )
        .unwrap()
        .with_custom_operator(
            CustomOperator::new("within", "{field} BETWEEN {0} AND {1}", 2).unwrap(),
            ValueType::I64,
        )
        .unwrap();
    let parser = ParamParser::new(&policy).unwrap();
    for input in ["id-op=nonzero", "id-op=within&id=1&id=3"] {
        let query = parser
            .parse::<OperatorCases>(&RawParams::parse(input).unwrap())
            .unwrap();
        assert_eq!(
            query.predicate(),
            parser
                .parse::<OperatorCases>(&query.to_rpc(&policy).unwrap())
                .unwrap()
                .predicate()
        );
    }
    for input in [
        "id-op=within&id=1",
        "id-op=within&id=1&id=2&id=3",
        "id-op=nonzero&id=1",
        "id-op=within&id=1&id=x",
        "id-op=unknown",
        "id-op=within&id=1&id=2&id-ic=true",
    ] {
        assert!(
            parser
                .parse::<OperatorCases>(&RawParams::parse(input).unwrap())
                .is_err(),
            "{input}"
        );
    }
    let incompatible = QueryPolicy::default()
        .with_custom_operator(
            CustomOperator::new("nonzero", "{field} <> 0", 0).unwrap(),
            ValueType::Text,
        )
        .unwrap()
        .with_custom_operator(
            CustomOperator::new("within", "{field} BETWEEN {0} AND {1}", 2).unwrap(),
            ValueType::I64,
        )
        .unwrap();
    assert!(
        ParamParser::new(&incompatible)
            .unwrap()
            .parse::<OperatorCases>(&RawParams::default())
            .is_err()
    );
    assert!(CustomOperator::new("evil", "{field} = {0}; DROP TABLE items", 1).is_err());
    let trusted = QueryBuilder::<OperatorCases>::new(&policy)
        .custom(
            OperatorCasesFields::ID,
            CustomOperator::new("serverOnly", "{field} = {0}", 1).unwrap(),
            [1_i64],
        )
        .build()
        .unwrap();
    assert!(trusted.to_rpc(&policy).is_err());
}
