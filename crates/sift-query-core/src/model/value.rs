#[cfg(feature = "decimal")]
use std::str::FromStr;

use base64::Engine;

use crate::{ParamError, ValueType};

/// A typed, non-null scalar used in predicates.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum ScalarValue {
    Bool(bool),
    I32(i32),
    I64(i64),
    F32(f32),
    F64(f64),
    #[cfg(feature = "decimal")]
    Decimal(rust_decimal::Decimal),
    Text(String),
    Bytes(Vec<u8>),
    #[cfg(feature = "chrono")]
    Date(chrono::NaiveDate),
    #[cfg(feature = "chrono")]
    Time(chrono::NaiveTime),
    #[cfg(feature = "chrono")]
    DateTime(chrono::NaiveDateTime),
    #[cfg(feature = "chrono")]
    Timestamp(chrono::DateTime<chrono::Utc>),
    #[cfg(feature = "uuid")]
    Uuid(uuid::Uuid),
    #[cfg(feature = "json")]
    Json(Box<serde_json::Value>),
    #[cfg(feature = "postgres-types")]
    ArrayI64(Vec<Option<i64>>),
    #[cfg(feature = "postgres-types")]
    ArrayText(Vec<Option<String>>),
    #[cfg(feature = "postgres-types")]
    Int8Range(Box<crate::Int8Range>),
    #[cfg(feature = "postgres-types")]
    Network(Box<ipnet::IpNet>),
}

impl ScalarValue {
    /// Structured inputs charge their elements to the same logical budget.
    pub fn logical_weight(&self) -> usize {
        match self {
            #[cfg(feature = "postgres-types")]
            Self::ArrayI64(values) => values.len().max(1),
            #[cfg(feature = "postgres-types")]
            Self::ArrayText(values) => values.len().max(1),
            #[cfg(feature = "json")]
            Self::Json(value) => json_weight(value),
            _ => 1,
        }
    }

    pub(crate) fn validate(&self) -> Result<(), ParamError> {
        #[cfg(feature = "postgres-types")]
        if let Self::Int8Range(value) = self {
            value
                .validate()
                .map_err(|reason| ParamError::InvalidValue {
                    field: "<range>".to_owned(),
                    reason,
                })?;
        }
        if matches!(self, Self::F32(value) if !value.is_finite())
            || matches!(self, Self::F64(value) if !value.is_finite())
        {
            return Err(ParamError::InvalidValue {
                field: "<value>".to_owned(),
                reason: "expected a finite float".to_owned(),
            });
        }
        Ok(())
    }
    #[must_use]
    pub fn to_external_text(&self) -> String {
        RowValue::from(self.clone()).to_external_text()
    }
    #[must_use]
    pub fn value_type(&self) -> ValueType {
        match self {
            Self::Bool(_) => ValueType::Bool,
            Self::I32(_) => ValueType::I32,
            Self::I64(_) => ValueType::I64,
            Self::F32(_) => ValueType::F32,
            Self::F64(_) => ValueType::F64,
            #[cfg(feature = "decimal")]
            Self::Decimal(_) => ValueType::Decimal,
            Self::Text(_) => ValueType::Text,
            Self::Bytes(_) => ValueType::Bytes,
            #[cfg(feature = "chrono")]
            Self::Date(_) => ValueType::Date,
            #[cfg(feature = "chrono")]
            Self::Time(_) => ValueType::Time,
            #[cfg(feature = "chrono")]
            Self::DateTime(_) => ValueType::DateTime,
            #[cfg(feature = "chrono")]
            Self::Timestamp(_) => ValueType::Timestamp,
            #[cfg(feature = "uuid")]
            Self::Uuid(_) => ValueType::Uuid,
            #[cfg(feature = "json")]
            Self::Json(_) => ValueType::Json,
            #[cfg(feature = "postgres-types")]
            Self::ArrayI64(_) => ValueType::ArrayI64,
            #[cfg(feature = "postgres-types")]
            Self::ArrayText(_) => ValueType::ArrayText,
            #[cfg(feature = "postgres-types")]
            Self::Int8Range(_) => ValueType::Int8Range,
            #[cfg(feature = "postgres-types")]
            Self::Network(_) => ValueType::Network,
        }
    }

    pub(crate) fn parse(
        value_type: ValueType,
        input: &str,
        field: &str,
    ) -> Result<Self, ParamError> {
        let invalid = |reason: &str| ParamError::InvalidValue {
            field: field.to_owned(),
            reason: reason.to_owned(),
        };
        match value_type {
            ValueType::Bool => match input {
                "true" => Ok(Self::Bool(true)),
                "false" => Ok(Self::Bool(false)),
                _ => Err(invalid("expected true or false")),
            },
            ValueType::I32 => input
                .parse()
                .map(Self::I32)
                .map_err(|_| invalid("expected a 32-bit decimal integer")),
            ValueType::I64 => input
                .parse()
                .map(Self::I64)
                .map_err(|_| invalid("expected a 64-bit decimal integer")),
            ValueType::F32 => {
                let value: f32 = input
                    .parse()
                    .map_err(|_| invalid("expected a finite 32-bit float"))?;
                value
                    .is_finite()
                    .then_some(Self::F32(value))
                    .ok_or_else(|| invalid("expected a finite 32-bit float"))
            }
            ValueType::F64 => {
                let value: f64 = input
                    .parse()
                    .map_err(|_| invalid("expected a finite 64-bit float"))?;
                value
                    .is_finite()
                    .then_some(Self::F64(value))
                    .ok_or_else(|| invalid("expected a finite 64-bit float"))
            }
            #[cfg(feature = "decimal")]
            ValueType::Decimal => rust_decimal::Decimal::from_str(input)
                .map(Self::Decimal)
                .map_err(|_| invalid("expected a decimal")),
            ValueType::Text => Ok(Self::Text(input.to_owned())),
            ValueType::Bytes => base64::engine::general_purpose::URL_SAFE_NO_PAD
                .decode(input)
                .map(Self::Bytes)
                .map_err(|_| invalid("expected unpadded base64url")),
            #[cfg(feature = "chrono")]
            ValueType::Date => chrono::NaiveDate::parse_from_str(input, "%Y-%m-%d")
                .map(Self::Date)
                .map_err(|_| invalid("expected YYYY-MM-DD")),
            #[cfg(feature = "chrono")]
            ValueType::Time => chrono::NaiveTime::parse_from_str(input, "%H:%M:%S%.f")
                .map(Self::Time)
                .map_err(|_| invalid("expected HH:MM:SS[.fraction]")),
            #[cfg(feature = "chrono")]
            ValueType::DateTime => {
                chrono::NaiveDateTime::parse_from_str(input, "%Y-%m-%dT%H:%M:%S%.f")
                    .map(Self::DateTime)
                    .map_err(|_| invalid("expected YYYY-MM-DDTHH:MM:SS[.fraction]"))
            }
            #[cfg(feature = "chrono")]
            ValueType::Timestamp => chrono::DateTime::parse_from_rfc3339(input)
                .map(|value| Self::Timestamp(value.to_utc()))
                .map_err(|_| invalid("expected an RFC 3339 timestamp")),
            #[cfg(feature = "uuid")]
            ValueType::Uuid => uuid::Uuid::parse_str(input)
                .map(Self::Uuid)
                .map_err(|_| invalid("expected a UUID")),
            #[cfg(feature = "json")]
            ValueType::Json => serde_json::from_str(input)
                .map(|value| Self::Json(Box::new(value)))
                .map_err(|_| invalid("expected structured JSON")),
            #[cfg(feature = "postgres-types")]
            ValueType::ArrayI64 => serde_json::from_str(input)
                .map(Self::ArrayI64)
                .map_err(|_| invalid("expected JSON array of int64 values or null")),
            #[cfg(feature = "postgres-types")]
            ValueType::ArrayText => serde_json::from_str(input)
                .map(Self::ArrayText)
                .map_err(|_| invalid("expected JSON array of strings or null")),
            #[cfg(feature = "postgres-types")]
            ValueType::Int8Range => input
                .parse()
                .map(|value| Self::Int8Range(Box::new(value)))
                .map_err(|reason: String| invalid(&reason)),
            #[cfg(feature = "postgres-types")]
            ValueType::Network => input
                .parse()
                .or_else(|_| input.parse::<std::net::IpAddr>().map(ipnet::IpNet::from))
                .map(|value| Self::Network(Box::new(value)))
                .map_err(|_| invalid("expected IPv4/IPv6 address or network")),
        }
    }
}

#[cfg(feature = "json")]
fn json_weight(value: &serde_json::Value) -> usize {
    match value {
        serde_json::Value::Array(values) => values
            .iter()
            .map(json_weight)
            .sum::<usize>()
            .saturating_add(1),
        serde_json::Value::Object(values) => values
            .values()
            .map(json_weight)
            .sum::<usize>()
            .saturating_add(1),
        _ => 1,
    }
}

/// A homogeneous array value used by PostgreSQL `ANY`.
#[derive(Debug, Clone, PartialEq)]
pub struct TypedList {
    pub element_type: ValueType,
    pub values: Vec<ScalarValue>,
}

impl TypedList {
    pub fn new(element_type: ValueType, values: Vec<ScalarValue>) -> Result<Self, ParamError> {
        if values.is_empty() {
            return Err(ParamError::InvalidValue {
                field: "<list>".to_owned(),
                reason: "list must not be empty".to_owned(),
            });
        }
        if values
            .iter()
            .any(|value| value.value_type() != element_type)
        {
            return Err(ParamError::InvalidValue {
                field: "<list>".to_owned(),
                reason: "list elements must have one type".to_owned(),
            });
        }
        Ok(Self {
            element_type,
            values,
        })
    }
}

/// Dynamic result value. Unlike [`ScalarValue`], this can represent SQL NULL.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum RowValue {
    #[cfg(feature = "postgres-types")]
    ArrayI64(Vec<Option<i64>>),
    #[cfg(feature = "postgres-types")]
    ArrayText(Vec<Option<String>>),
    #[cfg(feature = "postgres-types")]
    Int8Range(crate::Int8Range),
    #[cfg(feature = "postgres-types")]
    Network(ipnet::IpNet),
    /// Presentation-only list; never a PostgreSQL array bind or cursor key.
    List(Vec<RowValue>),
    Null,
    Bool(bool),
    I32(i32),
    I64(i64),
    F32(f32),
    F64(f64),
    #[cfg(feature = "decimal")]
    Decimal(rust_decimal::Decimal),
    Text(String),
    Bytes(Vec<u8>),
    #[cfg(feature = "chrono")]
    Date(chrono::NaiveDate),
    #[cfg(feature = "chrono")]
    Time(chrono::NaiveTime),
    #[cfg(feature = "chrono")]
    DateTime(chrono::NaiveDateTime),
    #[cfg(feature = "chrono")]
    Timestamp(chrono::DateTime<chrono::Utc>),
    #[cfg(feature = "uuid")]
    Uuid(uuid::Uuid),
    #[cfg(feature = "json")]
    Json(serde_json::Value),
}

impl RowValue {
    #[must_use]
    pub fn value_type(&self) -> Option<ValueType> {
        match self {
            Self::Null | Self::List(_) => None,
            Self::Bool(_) => Some(ValueType::Bool),
            Self::I32(_) => Some(ValueType::I32),
            Self::I64(_) => Some(ValueType::I64),
            Self::F32(_) => Some(ValueType::F32),
            Self::F64(_) => Some(ValueType::F64),
            #[cfg(feature = "decimal")]
            Self::Decimal(_) => Some(ValueType::Decimal),
            Self::Text(_) => Some(ValueType::Text),
            Self::Bytes(_) => Some(ValueType::Bytes),
            #[cfg(feature = "chrono")]
            Self::Date(_) => Some(ValueType::Date),
            #[cfg(feature = "chrono")]
            Self::Time(_) => Some(ValueType::Time),
            #[cfg(feature = "chrono")]
            Self::DateTime(_) => Some(ValueType::DateTime),
            #[cfg(feature = "chrono")]
            Self::Timestamp(_) => Some(ValueType::Timestamp),
            #[cfg(feature = "uuid")]
            Self::Uuid(_) => Some(ValueType::Uuid),
            #[cfg(feature = "json")]
            Self::Json(_) => Some(ValueType::Json),
            #[cfg(feature = "postgres-types")]
            Self::ArrayI64(_) => Some(ValueType::ArrayI64),
            #[cfg(feature = "postgres-types")]
            Self::ArrayText(_) => Some(ValueType::ArrayText),
            #[cfg(feature = "postgres-types")]
            Self::Int8Range(_) => Some(ValueType::Int8Range),
            #[cfg(feature = "postgres-types")]
            Self::Network(_) => Some(ValueType::Network),
        }
    }
    /// Canonical external text used by CSV's default formatter. JSON uses
    /// compact JSON, bytes use unpadded base64url, and timestamps use RFC 3339.
    #[must_use]
    pub fn to_external_text(&self) -> String {
        match self {
            #[cfg(feature = "postgres-types")]
            Self::ArrayI64(value) => serde_json::to_string(value).expect("integer array"),
            #[cfg(feature = "postgres-types")]
            Self::ArrayText(value) => serde_json::to_string(value).expect("text array"),
            #[cfg(feature = "postgres-types")]
            Self::Int8Range(value) => value.to_string(),
            #[cfg(feature = "postgres-types")]
            Self::Network(value) => value.to_string(),
            Self::List(values) => format!(
                "[{}]",
                values.iter().map(json_text).collect::<Vec<_>>().join(",")
            ),
            Self::Null => String::new(),
            Self::Bool(value) => value.to_string(),
            Self::I32(value) => value.to_string(),
            Self::I64(value) => value.to_string(),
            Self::F32(value) => value.to_string(),
            Self::F64(value) => value.to_string(),
            Self::Text(value) => value.clone(),
            Self::Bytes(value) => base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(value),
            #[cfg(feature = "decimal")]
            Self::Decimal(value) => value.to_string(),
            #[cfg(feature = "chrono")]
            Self::Date(value) => value.format("%Y-%m-%d").to_string(),
            #[cfg(feature = "chrono")]
            Self::Time(value) => value.format("%H:%M:%S%.f").to_string(),
            #[cfg(feature = "chrono")]
            Self::DateTime(value) => value.format("%Y-%m-%dT%H:%M:%S%.f").to_string(),
            #[cfg(feature = "chrono")]
            Self::Timestamp(value) => value.to_rfc3339(),
            #[cfg(feature = "uuid")]
            Self::Uuid(value) => value.to_string(),
            #[cfg(feature = "json")]
            Self::Json(value) => value.to_string(),
        }
    }
}

impl From<ScalarValue> for RowValue {
    fn from(value: ScalarValue) -> Self {
        match value {
            #[cfg(feature = "postgres-types")]
            ScalarValue::ArrayI64(value) => Self::ArrayI64(value),
            #[cfg(feature = "postgres-types")]
            ScalarValue::ArrayText(value) => Self::ArrayText(value),
            #[cfg(feature = "postgres-types")]
            ScalarValue::Int8Range(value) => Self::Int8Range(*value),
            #[cfg(feature = "postgres-types")]
            ScalarValue::Network(value) => Self::Network(*value),
            ScalarValue::Bool(value) => Self::Bool(value),
            ScalarValue::I32(value) => Self::I32(value),
            ScalarValue::I64(value) => Self::I64(value),
            ScalarValue::F32(value) => Self::F32(value),
            ScalarValue::F64(value) => Self::F64(value),
            ScalarValue::Text(value) => Self::Text(value),
            ScalarValue::Bytes(value) => Self::Bytes(value),
            #[cfg(feature = "decimal")]
            ScalarValue::Decimal(value) => Self::Decimal(value),
            #[cfg(feature = "chrono")]
            ScalarValue::Date(value) => Self::Date(value),
            #[cfg(feature = "chrono")]
            ScalarValue::Time(value) => Self::Time(value),
            #[cfg(feature = "chrono")]
            ScalarValue::DateTime(value) => Self::DateTime(value),
            #[cfg(feature = "chrono")]
            ScalarValue::Timestamp(value) => Self::Timestamp(value),
            #[cfg(feature = "uuid")]
            ScalarValue::Uuid(value) => Self::Uuid(value),
            #[cfg(feature = "json")]
            ScalarValue::Json(value) => Self::Json(*value),
        }
    }
}

fn json_text(value: &RowValue) -> String {
    match value {
        RowValue::Null => "null".to_owned(),
        RowValue::Bool(_) | RowValue::I32(_) => value.to_external_text(),
        RowValue::F32(number) if number.is_finite() => number.to_string(),
        RowValue::F64(number) if number.is_finite() => number.to_string(),
        RowValue::F32(_) | RowValue::F64(_) => "null".to_owned(),
        RowValue::List(_) => value.to_external_text(),
        #[cfg(feature = "json")]
        RowValue::Json(json) => json.to_string(),
        _ => {
            use std::fmt::Write;
            let mut output = String::from("\"");
            for character in value.to_external_text().chars() {
                match character {
                    '"' => output.push_str("\\\""),
                    '\\' => output.push_str("\\\\"),
                    character if character <= '\u{1f}' => {
                        write!(&mut output, "\\u{:04x}", u32::from(character))
                            .expect("writing to String cannot fail");
                    }
                    character => output.push(character),
                }
            }
            output.push('"');
            output
        }
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for RowValue {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::Error as _;
        match self {
            Self::List(values) => serde::Serialize::serialize(values, serializer),
            #[cfg(feature = "postgres-types")]
            Self::ArrayI64(values) => serde::Serialize::serialize(
                &values
                    .iter()
                    .map(|value| value.map(|value| value.to_string()))
                    .collect::<Vec<_>>(),
                serializer,
            ),
            #[cfg(feature = "postgres-types")]
            Self::ArrayText(values) => serde::Serialize::serialize(values, serializer),
            #[cfg(feature = "postgres-types")]
            Self::Int8Range(value) => serializer.serialize_str(&value.to_string()),
            #[cfg(feature = "postgres-types")]
            Self::Network(value) => serializer.serialize_str(&value.to_string()),
            Self::Null => serializer.serialize_none(),
            Self::Bool(value) => serializer.serialize_bool(*value),
            Self::I32(value) => serializer.serialize_i32(*value),
            // Preserve exact values for JavaScript and other IEEE-754 consumers.
            Self::I64(value) => serializer.serialize_str(&value.to_string()),
            Self::F32(value) if value.is_finite() => serializer.serialize_f32(*value),
            Self::F64(value) if value.is_finite() => serializer.serialize_f64(*value),
            Self::F32(_) | Self::F64(_) => {
                Err(S::Error::custom("non-finite floats cannot be serialized"))
            }
            #[cfg(feature = "decimal")]
            Self::Decimal(value) => serializer.serialize_str(&value.to_string()),
            Self::Text(value) => serializer.serialize_str(value),
            Self::Bytes(value) => serializer
                .serialize_str(&base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(value)),
            #[cfg(feature = "chrono")]
            Self::Date(value) => serializer.serialize_str(&value.format("%Y-%m-%d").to_string()),
            #[cfg(feature = "chrono")]
            Self::Time(value) => serializer.serialize_str(&value.format("%H:%M:%S%.f").to_string()),
            #[cfg(feature = "chrono")]
            Self::DateTime(value) => {
                serializer.serialize_str(&value.format("%Y-%m-%dT%H:%M:%S%.f").to_string())
            }
            #[cfg(feature = "chrono")]
            Self::Timestamp(value) => serializer.serialize_str(&value.to_rfc3339()),
            #[cfg(feature = "uuid")]
            Self::Uuid(value) => serializer.serialize_str(&value.to_string()),
            #[cfg(feature = "json")]
            Self::Json(value) => value.serialize(serializer),
        }
    }
}

pub trait IntoScalarValue {
    fn into_scalar_value(self) -> ScalarValue;
}

macro_rules! impl_scalar {
    ($type:ty, $variant:ident) => {
        impl IntoScalarValue for $type {
            fn into_scalar_value(self) -> ScalarValue {
                ScalarValue::$variant(self)
            }
        }
    };
}

impl_scalar!(bool, Bool);
impl_scalar!(i32, I32);
impl_scalar!(i64, I64);
impl_scalar!(f32, F32);
impl_scalar!(f64, F64);
impl_scalar!(String, Text);
impl_scalar!(Vec<u8>, Bytes);

impl IntoScalarValue for &str {
    fn into_scalar_value(self) -> ScalarValue {
        ScalarValue::Text(self.to_owned())
    }
}

#[cfg(feature = "decimal")]
impl_scalar!(rust_decimal::Decimal, Decimal);
#[cfg(feature = "chrono")]
impl_scalar!(chrono::NaiveDate, Date);
#[cfg(feature = "chrono")]
impl_scalar!(chrono::NaiveTime, Time);
#[cfg(feature = "chrono")]
impl_scalar!(chrono::NaiveDateTime, DateTime);
#[cfg(feature = "chrono")]
impl_scalar!(chrono::DateTime<chrono::Utc>, Timestamp);
#[cfg(feature = "uuid")]
impl_scalar!(uuid::Uuid, Uuid);
#[cfg(feature = "json")]
impl IntoScalarValue for serde_json::Value {
    fn into_scalar_value(self) -> ScalarValue {
        ScalarValue::Json(Box::new(self))
    }
}
#[cfg(feature = "postgres-types")]
impl_scalar!(Vec<Option<i64>>, ArrayI64);
#[cfg(feature = "postgres-types")]
impl_scalar!(Vec<Option<String>>, ArrayText);
#[cfg(feature = "postgres-types")]
impl IntoScalarValue for crate::Int8Range {
    fn into_scalar_value(self) -> ScalarValue {
        ScalarValue::Int8Range(Box::new(self))
    }
}
#[cfg(feature = "postgres-types")]
impl IntoScalarValue for ipnet::IpNet {
    fn into_scalar_value(self) -> ScalarValue {
        ScalarValue::Network(Box::new(self))
    }
}
#[cfg(feature = "postgres-types")]
impl IntoScalarValue for Vec<i64> {
    fn into_scalar_value(self) -> ScalarValue {
        ScalarValue::ArrayI64(self.into_iter().map(Some).collect())
    }
}
#[cfg(feature = "postgres-types")]
impl IntoScalarValue for Vec<String> {
    fn into_scalar_value(self) -> ScalarValue {
        ScalarValue::ArrayText(self.into_iter().map(Some).collect())
    }
}
