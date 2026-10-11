use crate::*;
use sift_query_core::{QueryPlan, RowValue, SearchEntity, SearchOptions};
use sift_query_sqlx_common::{Backend, SearchError, SummaryValue, summary_output_type};
use sqlx::sqlite::SqliteRow;

pub(crate) fn decode_summaries<E: SearchEntity>(
    row: &SqliteRow,
    options: Option<&SearchOptions<E>>,
    meta: &'static sift_query_core::EntityMeta,
) -> Result<Vec<SummaryValue>, SearchError> {
    let Some(options) = options else {
        return Ok(Vec::new());
    };
    options
        .requests()
        .iter()
        .enumerate()
        .map(|(index, request)| {
            let field = meta
                .field(request.field)
                .ok_or_else(|| SearchError::Decode {
                    column: format!("sq_{index}"),
                    reason: "unknown summary field".to_owned(),
                })?;
            let value_type = summary_output_type(request.function, request.input_type)?;
            let value = SqliteBackend::decode(row, &format!("sq_{index}"), value_type)?;
            Ok(SummaryValue {
                field: field.public_name,
                function: request.function,
                value_type,
                value: (!matches!(value, RowValue::Null)).then_some(value),
            })
        })
        .collect()
}

pub(crate) fn decode_map<E: SearchEntity>(
    row: &SqliteRow,
    plan: &QueryPlan<E>,
) -> Result<indexmap::IndexMap<String, RowValue>, SearchError> {
    let mut output = indexmap::IndexMap::with_capacity(plan.projection.len());
    for field_id in &plan.projection {
        let field = plan.meta.field(*field_id).ok_or(SearchError::Decode {
            column: field_id.0.to_string(),
            reason: "unknown field".to_owned(),
        })?;
        let alias = field.result_alias.ok_or(SearchError::Decode {
            column: field.public_name.to_owned(),
            reason: "missing result alias".to_owned(),
        })?;
        output.insert(
            field.public_name.to_owned(),
            SqliteBackend::decode(row, alias, field.value_type)?,
        );
    }
    Ok(output)
}
