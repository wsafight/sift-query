use sift_query_core::{RowValue, ScalarValue, ValueType};
use sift_query_sqlx_common::{Backend, Dialect, SearchError};
use sqlx::mysql::{MySql, MySqlArguments, MySqlRow};
use sqlx::{Arguments, Row, TypeInfo, ValueRef};

#[derive(Debug, Clone, Copy, Default)]
pub struct MySqlBackend;

/// Decode an aggregate result, including MySQL's DECIMAL representation for
/// SUM/AVG over integer and floating point columns.
pub fn decode_summary(
    row: &MySqlRow,
    column: &str,
    value_type: ValueType,
) -> Result<RowValue, SearchError> {
    let raw = row.try_get_raw(column)?;
    if raw.type_info().name() != "DECIMAL"
        || !matches!(
            value_type,
            ValueType::I32 | ValueType::I64 | ValueType::F32 | ValueType::F64
        )
    {
        return MySqlBackend::decode(row, column, value_type);
    }
    // MySQL transmits DECIMAL as ASCII in both result protocols. Its type
    // has been checked above; only the String compatibility check is bypassed.
    // This keeps integer SUM available without enabling the decimal feature.
    let text = row.try_get_unchecked::<Option<&str>, _>(column)?;
    let Some(text) = text else {
        return Ok(RowValue::Null);
    };
    parse_decimal_summary(text, value_type).map_err(|reason| SearchError::Decode {
        column: column.to_owned(),
        reason,
    })
}

fn parse_decimal_summary(text: &str, value_type: ValueType) -> Result<RowValue, String> {
    match value_type {
        ValueType::I32 => {
            let (integer, fractional) = text.split_once('.').unwrap_or((text, ""));
            if !fractional.bytes().all(|byte| byte == b'0') {
                return Err("integer summary contains a fractional value".to_owned());
            }
            integer
                .parse::<i32>()
                .map(RowValue::I32)
                .map_err(|e| e.to_string())
        }
        ValueType::I64 => {
            let (integer, fractional) = text.split_once('.').unwrap_or((text, ""));
            if !fractional.bytes().all(|byte| byte == b'0') {
                return Err("integer summary contains a fractional value".to_owned());
            }
            integer
                .parse::<i64>()
                .map(RowValue::I64)
                .map_err(|e| e.to_string())
        }
        ValueType::F32 => {
            let value = text.parse::<f32>().map_err(|e| e.to_string())?;
            if !value.is_finite() {
                return Err("summary is outside f32 range".to_owned());
            }
            Ok(RowValue::F32(value))
        }
        ValueType::F64 => {
            let value = text.parse::<f64>().map_err(|e| e.to_string())?;
            if !value.is_finite() {
                return Err("summary is outside f64 range".to_owned());
            }
            Ok(RowValue::F64(value))
        }
        _ => Err("unsupported decimal conversion".to_owned()),
    }
}

impl Backend for MySqlBackend {
    type Database = MySql;
    const DIALECT: Dialect = Dialect::MySql;

    fn arguments(values: &[ScalarValue]) -> Result<MySqlArguments, SearchError> {
        let mut args = MySqlArguments::default();
        for value in values {
            add_argument(&mut args, value)?;
        }
        Ok(args)
    }

    fn decode(
        row: &MySqlRow,
        column: &str,
        value_type: ValueType,
    ) -> Result<RowValue, SearchError> {
        decode_value(row, column, value_type)
    }

    fn count(row: &MySqlRow) -> Result<i64, SearchError> {
        row.try_get::<i64, _>("sq_count").map_err(SearchError::Sql)
    }
}

fn add_argument(args: &mut MySqlArguments, value: &ScalarValue) -> Result<(), SearchError> {
    macro_rules! add {
        ($v:expr) => {{
            args.add($v).map_err(|e| SearchError::Bind(e.to_string()))?;
        }};
    }
    match value {
        ScalarValue::Bool(v) => add!(*v),
        ScalarValue::I32(v) => add!(*v),
        ScalarValue::I64(v) => add!(*v),
        ScalarValue::F32(v) => add!(*v),
        ScalarValue::F64(v) => add!(*v),
        ScalarValue::Text(v) => add!(v),
        ScalarValue::Bytes(v) => add!(v),
        #[cfg(feature = "decimal")]
        ScalarValue::Decimal(v) => add!(*v),
        #[cfg(feature = "chrono")]
        ScalarValue::Date(v) => add!(*v),
        #[cfg(feature = "chrono")]
        ScalarValue::Time(v) => add!(*v),
        #[cfg(feature = "chrono")]
        ScalarValue::DateTime(v) => add!(*v),
        #[cfg(feature = "chrono")]
        ScalarValue::Timestamp(v) => add!(*v),
        #[cfg(feature = "uuid")]
        ScalarValue::Uuid(v) => add!(*v),
        #[cfg(feature = "json")]
        ScalarValue::Json(v) => add!(sqlx::types::Json(v.as_ref().clone())),
        _ => {
            return Err(SearchError::Bind(format!(
                "MySQL does not support {:?}",
                value.value_type()
            )));
        }
    }
    Ok(())
}

fn decode_value(
    row: &MySqlRow,
    column: &str,
    value_type: ValueType,
) -> Result<RowValue, SearchError> {
    macro_rules! decode {
        ($ty:ty, $variant:ident) => {
            row.try_get::<Option<$ty>, _>(column)
                .map(|v| v.map_or(RowValue::Null, RowValue::$variant))
                .map_err(|e| SearchError::Decode {
                    column: column.to_owned(),
                    reason: e.to_string(),
                })
        };
    }
    match value_type {
        ValueType::Bool => decode!(bool, Bool),
        ValueType::I32 => decode!(i32, I32),
        ValueType::I64 => decode!(i64, I64),
        ValueType::F32 => decode!(f32, F32),
        ValueType::F64 => decode!(f64, F64),
        ValueType::Text => decode!(String, Text),
        ValueType::Bytes => decode!(Vec<u8>, Bytes),
        #[cfg(feature = "decimal")]
        ValueType::Decimal => decode!(rust_decimal::Decimal, Decimal),
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
        ValueType::Json => row
            .try_get::<Option<sqlx::types::Json<serde_json::Value>>, _>(column)
            .map(|v| v.map_or(RowValue::Null, |v| RowValue::Json(v.0)))
            .map_err(|e| SearchError::Decode {
                column: column.to_owned(),
                reason: e.to_string(),
            }),
        _ => Err(SearchError::Decode {
            column: column.to_owned(),
            reason: "unsupported MySQL result type".to_owned(),
        }),
    }
}
