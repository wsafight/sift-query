use sift_query_core::{ScalarValue, TypedList, ValueType};
use sqlx::Arguments;
use sqlx::postgres::PgArguments;

use crate::{PgBindValue, PgSearchError};

pub(crate) fn build_arguments(binds: &[PgBindValue]) -> Result<PgArguments, PgSearchError> {
    let mut arguments = PgArguments::default();
    macro_rules! add {
        ($value:expr) => {
            arguments
                .add($value)
                .map_err(|error| PgSearchError::Bind(error.to_string()))?
        };
    }

    for bind in binds {
        match bind {
            PgBindValue::Scalar(value) => match value {
                ScalarValue::Bool(value) => add!(*value),
                ScalarValue::I32(value) => add!(*value),
                ScalarValue::I64(value) => add!(*value),
                ScalarValue::F32(value) => add!(*value),
                ScalarValue::F64(value) => add!(*value),
                #[cfg(feature = "decimal")]
                ScalarValue::Decimal(value) => add!(*value),
                ScalarValue::Text(value) => add!(value.clone()),
                ScalarValue::Bytes(value) => add!(value.clone()),
                #[cfg(feature = "chrono")]
                ScalarValue::Date(value) => add!(*value),
                #[cfg(feature = "chrono")]
                ScalarValue::Time(value) => add!(*value),
                #[cfg(feature = "chrono")]
                ScalarValue::DateTime(value) => add!(*value),
                #[cfg(feature = "chrono")]
                ScalarValue::Timestamp(value) => add!(*value),
                #[cfg(feature = "uuid")]
                ScalarValue::Uuid(value) => add!(*value),
                #[cfg(feature = "json")]
                ScalarValue::Json(value) => add!(sqlx::types::Json(value.clone())),
                #[cfg(feature = "postgres-types")]
                ScalarValue::ArrayI64(value) => add!(value.clone()),
                #[cfg(feature = "postgres-types")]
                ScalarValue::ArrayText(value) => add!(value.clone()),
                #[cfg(feature = "postgres-types")]
                ScalarValue::Int8Range(value) => add!(crate::PgInt8Range(**value)),
                #[cfg(feature = "postgres-types")]
                ScalarValue::Network(value) => add!(**value),
                _ => {
                    return Err(PgSearchError::Bind(
                        "unsupported scalar binding type".to_owned(),
                    ));
                }
            },
            PgBindValue::Array(list) => match list.element_type {
                ValueType::Bool => add!(collect_array(list, |value| match value {
                    ScalarValue::Bool(value) => Some(*value),
                    _ => None,
                })?),
                ValueType::I32 => add!(collect_array(list, |value| match value {
                    ScalarValue::I32(value) => Some(*value),
                    _ => None,
                })?),
                ValueType::I64 => add!(collect_array(list, |value| match value {
                    ScalarValue::I64(value) => Some(*value),
                    _ => None,
                })?),
                ValueType::F32 => add!(collect_array(list, |value| match value {
                    ScalarValue::F32(value) => Some(*value),
                    _ => None,
                })?),
                ValueType::F64 => add!(collect_array(list, |value| match value {
                    ScalarValue::F64(value) => Some(*value),
                    _ => None,
                })?),
                #[cfg(feature = "decimal")]
                ValueType::Decimal => add!(collect_array(list, |value| match value {
                    ScalarValue::Decimal(value) => Some(*value),
                    _ => None,
                })?),
                ValueType::Text => add!(collect_array(list, |value| match value {
                    ScalarValue::Text(value) => Some(value.clone()),
                    _ => None,
                })?),
                #[cfg(feature = "chrono")]
                ValueType::Date => add!(collect_array(list, |value| match value {
                    ScalarValue::Date(value) => Some(*value),
                    _ => None,
                })?),
                #[cfg(feature = "chrono")]
                ValueType::Time => add!(collect_array(list, |value| match value {
                    ScalarValue::Time(value) => Some(*value),
                    _ => None,
                })?),
                #[cfg(feature = "chrono")]
                ValueType::DateTime => add!(collect_array(list, |value| match value {
                    ScalarValue::DateTime(value) => Some(*value),
                    _ => None,
                })?),
                #[cfg(feature = "chrono")]
                ValueType::Timestamp => add!(collect_array(list, |value| match value {
                    ScalarValue::Timestamp(value) => Some(*value),
                    _ => None,
                })?),
                #[cfg(feature = "uuid")]
                ValueType::Uuid => add!(collect_array(list, |value| match value {
                    ScalarValue::Uuid(value) => Some(*value),
                    _ => None,
                })?),
                ValueType::Bytes => {
                    return Err(PgSearchError::Bind(
                        "bytea array binding is not enabled in M1".to_owned(),
                    ));
                }
                #[cfg(feature = "json")]
                ValueType::Json => {
                    return Err(PgSearchError::Bind(
                        "JSON array binding is not enabled in M1".to_owned(),
                    ));
                }
                _ => {
                    return Err(PgSearchError::Bind(
                        "unsupported PostgreSQL array binding type".to_owned(),
                    ));
                }
            },
        }
    }
    Ok(arguments)
}

fn collect_array<T>(
    list: &TypedList,
    convert: impl Fn(&ScalarValue) -> Option<T>,
) -> Result<Vec<T>, PgSearchError> {
    list.values
        .iter()
        .map(|value| {
            convert(value)
                .ok_or_else(|| PgSearchError::Bind("heterogeneous array binding".to_owned()))
        })
        .collect()
}
#[cfg(test)]
mod tests;
