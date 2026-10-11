use sift_query_core::{RowValue, ScalarValue, ValueType};
use sift_query_sqlx_common::{Backend, Dialect, SearchError};
use sqlx::sqlite::{Sqlite, SqliteArguments, SqliteRow};
use sqlx::{Arguments, Row};

#[derive(Debug, Clone, Copy, Default)]
pub struct SqliteBackend;

impl Backend for SqliteBackend {
    type Database = Sqlite;
    const DIALECT: Dialect = Dialect::Sqlite;

    fn arguments(values: &[ScalarValue]) -> Result<SqliteArguments, SearchError> {
        let mut args = SqliteArguments::default();
        for value in values {
            add_argument(&mut args, value)?;
        }
        Ok(args)
    }

    fn decode(
        row: &SqliteRow,
        column: &str,
        value_type: ValueType,
    ) -> Result<RowValue, SearchError> {
        decode_value(row, column, value_type)
    }

    fn count(row: &SqliteRow) -> Result<i64, SearchError> {
        row.try_get::<i64, _>("sq_count").map_err(SearchError::Sql)
    }
}

fn add_argument(args: &mut SqliteArguments, value: &ScalarValue) -> Result<(), SearchError> {
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
                "SQLite does not support {:?}",
                value.value_type()
            )));
        }
    }
    Ok(())
}

fn decode_value(
    row: &SqliteRow,
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
            reason: "unsupported SQLite result type".to_owned(),
        }),
    }
}
