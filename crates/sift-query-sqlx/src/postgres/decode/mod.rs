use sift_query_core::{AggregateFn, RowValue, SearchEntity, SearchOptions, ValueType};
use sqlx::Row;
use sqlx::postgres::PgRow;

use crate::{DecodeError, PgSearchError, RowMap, SummaryValue};

pub(crate) fn decode_map<E: SearchEntity>(
    row: &PgRow,
    plan: &sift_query_core::QueryPlan<E>,
) -> Result<RowMap, DecodeError> {
    let mut output = RowMap::with_capacity(plan.projection.len());
    for field_id in &plan.projection {
        let field = plan.meta.field(*field_id).ok_or_else(|| DecodeError {
            column: format!("#{}", field_id.0),
            reason: "unknown projected field".to_owned(),
        })?;
        let alias = field.result_alias.ok_or_else(|| DecodeError {
            column: field.public_name.to_owned(),
            reason: "projected field has no result alias".to_owned(),
        })?;
        output.insert(
            field.public_name.to_owned(),
            decode_row_value(row, alias, field.value_type)?,
        );
    }
    Ok(output)
}

pub(crate) fn decode_summaries<E: SearchEntity>(
    row: &PgRow,
    options: Option<&SearchOptions<E>>,
    meta: &'static sift_query_core::EntityMeta,
) -> Result<Vec<SummaryValue>, PgSearchError> {
    let mut summaries = Vec::new();
    if let Some(options) = options {
        for (index, request) in options.requests().iter().enumerate() {
            let field = meta.field(request.field).ok_or_else(|| {
                PgSearchError::Decode(DecodeError {
                    column: format!("sq_{index}"),
                    reason: "unknown summary field".to_owned(),
                })
            })?;
            summaries.push(SummaryValue {
                field: field.public_name,
                function: request.function,
                value_type: summary_output_type(request.function, request.input_type)?,
                value: decode_summary(
                    row,
                    &format!("sq_{index}"),
                    summary_output_type(request.function, request.input_type)?,
                )?,
            });
        }
    }
    Ok(summaries)
}

pub(crate) fn decode_row_value(
    row: &PgRow,
    alias: &str,
    value_type: ValueType,
) -> Result<RowValue, DecodeError> {
    macro_rules! decode {
        ($type:ty, $variant:ident) => {
            row.try_get::<Option<$type>, _>(alias)
                .map(|value| value.map_or(RowValue::Null, RowValue::$variant))
                .map_err(|error| DecodeError {
                    column: alias.to_owned(),
                    reason: error.to_string(),
                })
        };
    }
    match value_type {
        ValueType::Bool => decode!(bool, Bool),
        ValueType::I32 => decode!(i32, I32),
        ValueType::I64 => decode!(i64, I64),
        ValueType::F32 => decode!(f32, F32),
        ValueType::F64 => decode!(f64, F64),
        #[cfg(feature = "decimal")]
        ValueType::Decimal => decode!(rust_decimal::Decimal, Decimal),
        ValueType::Text => decode!(String, Text),
        ValueType::Bytes => decode!(Vec<u8>, Bytes),
        #[cfg(feature = "chrono")]
        ValueType::Date => decode!(chrono::NaiveDate, Date),
        #[cfg(feature = "chrono")]
        ValueType::Time => decode!(chrono::NaiveTime, Time),
        #[cfg(feature = "chrono")]
        ValueType::DateTime => decode!(chrono::NaiveDateTime, DateTime),
        #[cfg(feature = "chrono")]
        ValueType::Timestamp => decode!(chrono::DateTime<chrono::Utc>, Timestamp),
        #[cfg(feature = "uuid")]
        ValueType::Uuid => decode!(uuid::Uuid, Uuid),
        #[cfg(feature = "json")]
        ValueType::Json => decode!(serde_json::Value, Json),
        #[cfg(feature = "postgres-types")]
        ValueType::ArrayI64 => decode!(Vec<Option<i64>>, ArrayI64),
        #[cfg(feature = "postgres-types")]
        ValueType::ArrayText => decode!(Vec<Option<String>>, ArrayText),
        #[cfg(feature = "postgres-types")]
        ValueType::Network => decode!(ipnet::IpNet, Network),
        #[cfg(feature = "postgres-types")]
        ValueType::Int8Range => row
            .try_get::<Option<crate::PgInt8Range>, _>(alias)
            .map(|value| value.map_or(RowValue::Null, |value| RowValue::Int8Range(value.0)))
            .map_err(|error| DecodeError {
                column: alias.to_owned(),
                reason: error.to_string(),
            }),
        _ => Err(DecodeError {
            column: alias.to_owned(),
            reason: "unsupported RowMap type".to_owned(),
        }),
    }
}

fn decode_summary(
    row: &PgRow,
    alias: &str,
    output: ValueType,
) -> Result<Option<RowValue>, PgSearchError> {
    macro_rules! decode {
        ($type:ty, $variant:ident) => {
            row.try_get::<Option<$type>, _>(alias)
                .map(|value| value.map(RowValue::$variant))
                .map_err(PgSearchError::Sql)
        };
    }
    match output {
        ValueType::Bool => decode!(bool, Bool),
        ValueType::I32 => decode!(i32, I32),
        ValueType::I64 => decode!(i64, I64),
        ValueType::F32 => decode!(f32, F32),
        ValueType::F64 => decode!(f64, F64),
        #[cfg(feature = "decimal")]
        ValueType::Decimal => decode!(rust_decimal::Decimal, Decimal),
        ValueType::Text => decode!(String, Text),
        ValueType::Bytes => decode!(Vec<u8>, Bytes),
        #[cfg(feature = "chrono")]
        ValueType::Date => decode!(chrono::NaiveDate, Date),
        #[cfg(feature = "chrono")]
        ValueType::Time => decode!(chrono::NaiveTime, Time),
        #[cfg(feature = "chrono")]
        ValueType::DateTime => decode!(chrono::NaiveDateTime, DateTime),
        #[cfg(feature = "chrono")]
        ValueType::Timestamp => decode!(chrono::DateTime<chrono::Utc>, Timestamp),
        #[cfg(feature = "uuid")]
        ValueType::Uuid => decode!(uuid::Uuid, Uuid),
        #[cfg(feature = "json")]
        ValueType::Json => decode!(serde_json::Value, Json),
        _ => Err(PgSearchError::Decode(DecodeError {
            column: alias.to_owned(),
            reason: "unsupported summary output type".to_owned(),
        })),
    }
}

fn summary_output_type(
    function: AggregateFn,
    input: ValueType,
) -> Result<ValueType, PgSearchError> {
    match function {
        AggregateFn::Sum => match input {
            ValueType::I32 => Ok(ValueType::I64),
            #[cfg(feature = "decimal")]
            ValueType::I64 | ValueType::Decimal => Ok(ValueType::Decimal),
            ValueType::F32 => Ok(ValueType::F32),
            ValueType::F64 => Ok(ValueType::F64),
            _ => unsupported_summary(),
        },
        AggregateFn::Avg => match input {
            #[cfg(feature = "decimal")]
            ValueType::I32 | ValueType::I64 | ValueType::Decimal => Ok(ValueType::Decimal),
            ValueType::F32 | ValueType::F64 => Ok(ValueType::F64),
            _ => unsupported_summary(),
        },
        AggregateFn::Min | AggregateFn::Max => Ok(input),
        AggregateFn::Custom(aggregate) => Ok(aggregate.output_type()),
    }
}

fn unsupported_summary<T>() -> Result<T, PgSearchError> {
    Err(PgSearchError::Decode(DecodeError {
        column: "summary".to_owned(),
        reason: "field type does not support the aggregate".to_owned(),
    }))
}
#[cfg(test)]
mod tests;
