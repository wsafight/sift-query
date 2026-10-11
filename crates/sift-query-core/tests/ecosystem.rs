#![cfg(any(feature = "metadata", feature = "rsql"))]
mod support;
use sift_query_core::*;
use support::CursorEntity;
#[cfg(feature = "rsql")]
struct RsqlEntity;
#[cfg(feature = "rsql")]
impl SearchEntity for RsqlEntity {
    const META: &'static EntityMeta = &EntityMeta {
        fields: &[FieldMeta {
            operators: OperatorSet(&[
                Operator::Eq,
                Operator::Ne,
                Operator::Gt,
                Operator::Ge,
                Operator::Lt,
                Operator::Le,
                Operator::InList,
                Operator::NotIn,
            ]),
            ..CursorEntity::META.fields[0]
        }],
        ..*CursorEntity::META
    };
}

#[cfg(feature = "metadata")]
#[test]
fn public_descriptions_are_stable_and_do_not_expose_sql() {
    let policy = QueryPolicy::default();
    let description = metadata::describe::<CursorEntity>(&policy).unwrap();
    assert_eq!(
        description,
        metadata::describe::<CursorEntity>(&policy).unwrap()
    );
    assert_eq!(description["version"], 1);
    let output = description.to_string();
    assert!(!output.contains("base_where"));
    assert!(!output.contains("column"));
    assert!(!output.contains("result_alias"));
    let schema = metadata::openapi::<CursorEntity>(&policy).unwrap();
    assert!(schema["requestBody"]["content"]["application/x-www-form-urlencoded"].is_object());
    assert!(schema["requestBody"]["content"]["application/json"].is_null());
    assert!(
        schema["parameters"]
            .as_array()
            .unwrap()
            .iter()
            .any(|param| param["name"] == "id-op")
    );
    let changed = metadata::describe::<CursorEntity>(
        &QueryPolicy::new(
            ParamNames::default(),
            QueryLimits {
                max_page_size: 10,
                ..QueryLimits::default()
            },
        )
        .unwrap(),
    )
    .unwrap();
    assert_ne!(
        description["policyFingerprint"],
        changed["policyFingerprint"]
    );
}

#[cfg(feature = "rsql")]
#[test]
fn rsql_uses_the_same_validated_query_model() {
    let policy = QueryPolicy::default();
    for input in [
        "id==1",
        "id=in=(1,2,3)",
        "(id==1,id==2);id==1",
        "id=out=(1,2)",
    ] {
        let query = rsql::parse::<RsqlEntity>(input, &policy).unwrap();
        let decoded = ParamParser::new(&policy)
            .unwrap()
            .parse::<RsqlEntity>(&query.to_rpc(&policy).unwrap())
            .unwrap();
        assert_eq!(query.predicate(), decoded.predicate());
    }
    for input in [
        "id==",
        "id=in=()",
        "id=in=(1,)",
        "id=unknown=1",
        "internal==1",
        "id==1*",
        "(id==1",
        "id==1;$expand==x",
        "id==1,id==2 trailing",
    ] {
        assert!(
            rsql::parse::<CursorEntity>(input, &policy).is_err(),
            "{input}"
        );
    }
    let deep = format!("{}id==1{}", "(".repeat(20), ")".repeat(20));
    assert!(rsql::parse::<CursorEntity>(&deep, &policy).is_err());
    let small = QueryPolicy::new(
        ParamNames::default(),
        QueryLimits {
            max_conditions: 1,
            ..QueryLimits::default()
        },
    )
    .unwrap();
    assert!(rsql::parse::<CursorEntity>("id==1;id==2", &small).is_err());
}
