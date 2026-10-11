use super::invalid_cursor;
use crate::{ParamError, RowValue, ScalarValue};

#[derive(Debug, Clone, PartialEq)]
pub enum CursorValue {
    Null,
    Scalar(ScalarValue),
}

pub trait IntoCursorValue {
    fn to_cursor_value(&self) -> CursorValue;
}

macro_rules! impl_cursor_value {
    ($type:ty, $variant:ident) => {
        impl IntoCursorValue for $type {
            fn to_cursor_value(&self) -> CursorValue {
                CursorValue::Scalar(ScalarValue::$variant(self.clone()))
            }
        }
    };
}

impl_cursor_value!(bool, Bool);
impl_cursor_value!(i32, I32);
impl_cursor_value!(i64, I64);
impl_cursor_value!(f32, F32);
impl_cursor_value!(f64, F64);
impl_cursor_value!(String, Text);
impl_cursor_value!(Vec<u8>, Bytes);
#[cfg(feature = "decimal")]
impl_cursor_value!(rust_decimal::Decimal, Decimal);
#[cfg(feature = "chrono")]
impl_cursor_value!(chrono::NaiveDate, Date);
#[cfg(feature = "chrono")]
impl_cursor_value!(chrono::NaiveTime, Time);
#[cfg(feature = "chrono")]
impl_cursor_value!(chrono::NaiveDateTime, DateTime);
#[cfg(feature = "chrono")]
impl_cursor_value!(chrono::DateTime<chrono::Utc>, Timestamp);
#[cfg(feature = "uuid")]
impl_cursor_value!(uuid::Uuid, Uuid);

impl<T: IntoCursorValue> IntoCursorValue for Option<T> {
    fn to_cursor_value(&self) -> CursorValue {
        self.as_ref()
            .map_or(CursorValue::Null, IntoCursorValue::to_cursor_value)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Cursor {
    pub(super) values: Vec<CursorValue>,
}

impl Cursor {
    #[must_use]
    pub fn values(&self) -> &[CursorValue] {
        &self.values
    }
}

impl CursorValue {
    pub fn from_row_value(value: &RowValue) -> Result<Self, ParamError> {
        Ok(match value {
            RowValue::List(_) => return Err(invalid_cursor("lists cannot be cursor keys")),
            #[cfg(feature = "postgres-types")]
            RowValue::ArrayI64(_)
            | RowValue::ArrayText(_)
            | RowValue::Int8Range(_)
            | RowValue::Network(_) => {
                return Err(invalid_cursor("native types cannot be cursor keys"));
            }
            RowValue::Null => Self::Null,
            RowValue::Bool(value) => Self::Scalar(ScalarValue::Bool(*value)),
            RowValue::I32(value) => Self::Scalar(ScalarValue::I32(*value)),
            RowValue::I64(value) => Self::Scalar(ScalarValue::I64(*value)),
            RowValue::F32(value) => Self::Scalar(ScalarValue::F32(*value)),
            RowValue::F64(value) => Self::Scalar(ScalarValue::F64(*value)),
            #[cfg(feature = "decimal")]
            RowValue::Decimal(value) => Self::Scalar(ScalarValue::Decimal(*value)),
            RowValue::Text(value) => Self::Scalar(ScalarValue::Text(value.clone())),
            RowValue::Bytes(value) => Self::Scalar(ScalarValue::Bytes(value.clone())),
            #[cfg(feature = "chrono")]
            RowValue::Date(value) => Self::Scalar(ScalarValue::Date(*value)),
            #[cfg(feature = "chrono")]
            RowValue::Time(value) => Self::Scalar(ScalarValue::Time(*value)),
            #[cfg(feature = "chrono")]
            RowValue::DateTime(value) => Self::Scalar(ScalarValue::DateTime(*value)),
            #[cfg(feature = "chrono")]
            RowValue::Timestamp(value) => Self::Scalar(ScalarValue::Timestamp(*value)),
            #[cfg(feature = "uuid")]
            RowValue::Uuid(value) => Self::Scalar(ScalarValue::Uuid(*value)),
            #[cfg(feature = "json")]
            RowValue::Json(_) => return Err(invalid_cursor("JSON cannot be a cursor key")),
        })
    }
}
