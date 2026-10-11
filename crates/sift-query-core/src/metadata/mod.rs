//! Versioned public query descriptions. Internal SQL is never serialized.
use crate::{ParamError, QueryPolicy, SearchEntity};
use serde_json::{Value, json};

pub fn describe<E: SearchEntity>(policy: &QueryPolicy) -> Result<Value, ParamError> {
    policy.validate_operators::<E>()?;
    policy.validate_codecs::<E>()?;
    let fields = E::META.fields.iter().map(|field| {
        let mut operators = field.operators.0.iter().map(|operator| json!({"name": operator.short_name(), "arity": operator.expected_arity(), "inputType": format!("{:?}", field.value_type)})).collect::<Vec<_>>();
        for name in E::custom_operator_names(field.id) {
            let registration = policy.registered_operator(name).expect("validated operator");
            operators.push(json!({"name":name,"arity":registration.operator.arity(),"inputType":format!("{:?}",registration.input_type)}));
        }
        json!({"name":field.public_name,"valueType":format!("{:?}",field.value_type),"filterable":field.filterable,"sortable":field.sortable,"cursorNonNull":E::cursor_key_non_null(field.id),"selectable":field.selectable && field.result_alias.is_some(),"operators": if field.filterable { operators } else { Vec::new() },"orderInput": E::condition_order(field.id).map(|order|json!({"field":E::META.field(order.source).map(|source|source.public_name),"operator":order.operator}))})
    }).collect::<Vec<_>>();
    Ok(json!({
        "version":1,"schemaFingerprint":format!("{:016x}",E::META.schema_fingerprint),"policyFingerprint":format!("{:016x}",policy.fingerprint()),"fields":fields,
        "paging":{"maxPageSize":policy.limits().max_page_size.min(E::META.limits.max_page_size),"maxOffset":policy.limits().max_offset.min(E::META.limits.max_offset),"cursor":policy.cursor_codec().is_some()},
        "parameters":{"page":policy.names().page,"size":policy.names().size,"offset":policy.names().offset,"cursor":policy.names().cursor,"orderBy":policy.names().order_by,"onlySelect":policy.names().only_select,"selectExclude":policy.names().select_exclude,"groupExpression":policy.names().group_expr},
        "encoding":{"values":"strings; repeat keys or contiguous field-0,field-1 indices","groups":"group.field; gexpr uses group names, & for AND and | for OR","projection":"comma separated public names; onlySelect and selectExclude are mutually exclusive"}
    }))
}

/// `OpenAPI` 3.1 GET parameters and POST schema; merge into application paths.
pub fn openapi<E: SearchEntity>(policy: &QueryPolicy) -> Result<Value, ParamError> {
    let description = describe::<E>(policy)?;
    let mut parameters = Vec::new();
    for field in E::META.fields.iter().filter(|field| field.filterable) {
        parameters.push(json!({"name":field.public_name,"in":"query","required":false,"style":"form","explode":true,"schema":{"type":"array","items":{"type":"string"}},"description":"Repeat values for list operators; structured inputs use a single encoded JSON string."}));
        let names = field
            .operators
            .0
            .iter()
            .map(|operator| operator.short_name())
            .chain(E::custom_operator_names(field.id).iter().copied())
            .collect::<Vec<_>>();
        parameters.push(json!({"name":format!("{}-op",field.public_name),"in":"query","schema":{"type":"string","enum":names}}));
    }
    for (name, text) in [
        (&policy.names().page, "One based page number"),
        (
            &policy.names().size,
            "Page size; entity and policy bounds apply",
        ),
        (&policy.names().offset, "Offset; exclusive with page/cursor"),
        (
            &policy.names().cursor,
            "Opaque cursor; empty string starts cursor paging",
        ),
        (
            &policy.names().order_by,
            "Comma separated publicField:asc or publicField:desc",
        ),
        (
            &policy.names().only_select,
            "Comma separated public projection fields",
        ),
        (
            &policy.names().select_exclude,
            "Comma separated excluded projection fields",
        ),
        (
            &policy.names().group_expr,
            "AND (&), OR (|), parentheses; refer to names in group.field parameters",
        ),
    ] {
        parameters.push(json!({"name":name,"in":"query","required":false,"schema":{"type":"string"},"description":text}));
    }
    Ok(
        json!({"openapi":"3.1.0","parameters":parameters,"requestBody":{"required":false,"content":{"application/x-www-form-urlencoded":{"schema":{"type":"object","additionalProperties":{"oneOf":[{"type":"string"},{"type":"array","items":{"type":"string"}}]}}}}},"x-sift-query":description}),
    )
}
