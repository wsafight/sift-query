use super::Codec;
use crate::{ConfigError, FieldMeta, RowValue, ScalarValue, ValueType};
use sift_query_algorithm::StableHasher64;

/// A strict text field codec with optional whitespace normalization. It is
/// useful when an API's representation must be explicit even though the
/// underlying value is already a SQL text column.
#[derive(Debug, Clone, Copy)]
pub struct StringCodec {
    pub name: &'static str,
    pub field: &'static str,
    pub trim: bool,
}

impl Codec for StringCodec {
    fn name(&self) -> &'static str {
        self.name
    }
    fn supports(&self, field: &FieldMeta) -> bool {
        field.public_name == self.field && field.value_type == ValueType::Text
    }
    fn parse(&self, _: &FieldMeta, input: &str) -> Result<ScalarValue, String> {
        Ok(ScalarValue::Text(
            if self.trim { input.trim() } else { input }.to_owned(),
        ))
    }
    fn format_result(&self, _: &FieldMeta, value: RowValue) -> Result<RowValue, String> {
        match value {
            RowValue::Null => Ok(RowValue::Null),
            RowValue::Text(value) => Ok(RowValue::Text(
                if self.trim { value.trim() } else { &value }.to_owned(),
            )),
            _ => Err("string result has an incompatible type".to_owned()),
        }
    }
    fn fingerprint(&self) -> u64 {
        let mut hash = StableHasher64::default();
        hash.write_delimited(self.name);
        hash.write_delimited(self.field);
        hash.write([u8::from(self.trim)]);
        hash.finish()
    }
}

/// Configurable text representation for a scalar field. Parsing and result
/// conversion use the same logical type, so cursor and database binds remain
/// typed while the public text format is centralized in one codec.
#[derive(Debug, Clone, Copy)]
pub struct ScalarTextCodec {
    pub name: &'static str,
    pub field: &'static str,
    pub value_type: ValueType,
    pub trim: bool,
}

impl Codec for ScalarTextCodec {
    fn name(&self) -> &'static str {
        self.name
    }
    fn supports(&self, field: &FieldMeta) -> bool {
        field.public_name == self.field && field.value_type == self.value_type
    }
    fn parse(&self, _: &FieldMeta, input: &str) -> Result<ScalarValue, String> {
        ScalarValue::parse(
            self.value_type,
            if self.trim { input.trim() } else { input },
            self.field,
        )
        .map_err(|error| error.to_string())
    }
    fn format_result(&self, _: &FieldMeta, value: RowValue) -> Result<RowValue, String> {
        if matches!(value, RowValue::Null) {
            return Ok(RowValue::Null);
        }
        if value.value_type() != Some(self.value_type) {
            return Err("scalar result has an incompatible type".to_owned());
        }
        Ok(RowValue::Text(value.to_external_text()))
    }
    fn format_input(&self, _: &FieldMeta, value: &ScalarValue) -> Result<String, String> {
        if value.value_type() != self.value_type {
            return Err("scalar input has an incompatible type".to_owned());
        }
        Ok(value.to_external_text())
    }
    fn fingerprint(&self) -> u64 {
        let mut hash = StableHasher64::default();
        hash.write_delimited(self.name);
        hash.write_delimited(self.field);
        hash.write_delimited(format!("{:?}", self.value_type));
        hash.write([u8::from(self.trim)]);
        hash.finish()
    }
}

#[cfg(feature = "json")]
#[derive(Debug, Clone, Copy)]
pub struct JsonValueCodec {
    pub name: &'static str,
    pub field: &'static str,
    pub max_bytes: usize,
}

#[cfg(feature = "json")]
impl Codec for JsonValueCodec {
    fn name(&self) -> &'static str {
        self.name
    }
    fn supports(&self, field: &FieldMeta) -> bool {
        field.public_name == self.field && field.value_type == ValueType::Json
    }
    fn parse(&self, _: &FieldMeta, input: &str) -> Result<ScalarValue, String> {
        if self.max_bytes == 0 || input.len() > self.max_bytes {
            return Err("JSON input exceeds codec budget".to_owned());
        }
        serde_json::from_str(input)
            .map(|value| ScalarValue::Json(Box::new(value)))
            .map_err(|error| format!("invalid JSON: {error}"))
    }
    fn format_result(&self, _: &FieldMeta, value: RowValue) -> Result<RowValue, String> {
        match value {
            RowValue::Null => Ok(RowValue::Null),
            RowValue::Json(value) => {
                let text = value.to_string();
                if self.max_bytes == 0 || text.len() > self.max_bytes {
                    return Err("JSON result exceeds codec budget".to_owned());
                }
                Ok(RowValue::Text(text))
            }
            _ => Err("JSON result has an incompatible type".to_owned()),
        }
    }
    fn format_input(&self, _: &FieldMeta, value: &ScalarValue) -> Result<String, String> {
        match value {
            ScalarValue::Json(value) => Ok(value.to_string()),
            _ => Err("JSON input has an incompatible type".to_owned()),
        }
    }
    fn fingerprint(&self) -> u64 {
        let mut hash = StableHasher64::default();
        hash.write_delimited(self.name);
        hash.write_delimited(self.field);
        hash.write_delimited(self.max_bytes.to_string());
        hash.finish()
    }
}

/// Strict enum names mapped to an integer database field. Empty and unknown
/// names/codes are errors; results use the same names accepted by input.
#[derive(Debug, Clone)]
pub struct EnumCodec {
    name: &'static str,
    field: &'static str,
    variants: Vec<(&'static str, i32)>,
}

impl EnumCodec {
    pub fn new(
        name: &'static str,
        field: &'static str,
        variants: impl IntoIterator<Item = (&'static str, i32)>,
    ) -> Result<Self, ConfigError> {
        let variants = variants.into_iter().collect::<Vec<_>>();
        if variants.is_empty()
            || variants.iter().enumerate().any(|(index, (name, code))| {
                name.is_empty()
                    || variants[..index]
                        .iter()
                        .any(|(other, value)| other == name || value == code)
            })
        {
            return Err(ConfigError::Invalid(
                "enum codec requires unique nonempty names and codes".to_owned(),
            ));
        }
        Ok(Self {
            name,
            field,
            variants,
        })
    }
}

impl Codec for EnumCodec {
    fn name(&self) -> &'static str {
        self.name
    }
    fn supports(&self, field: &FieldMeta) -> bool {
        field.public_name == self.field && field.value_type == ValueType::I32
    }
    fn parse(&self, _field: &FieldMeta, input: &str) -> Result<ScalarValue, String> {
        self.variants
            .iter()
            .find(|(name, _)| *name == input)
            .map(|(_, value)| ScalarValue::I32(*value))
            .ok_or_else(|| "unknown enum name".to_owned())
    }
    fn format_result(&self, _field: &FieldMeta, value: RowValue) -> Result<RowValue, String> {
        match value {
            RowValue::Null => Ok(RowValue::Null),
            RowValue::I32(value) => self
                .variants
                .iter()
                .find(|(_, code)| *code == value)
                .map(|(name, _)| RowValue::Text((*name).to_owned()))
                .ok_or_else(|| "unknown enum code".to_owned()),
            _ => Err("enum result has an incompatible type".to_owned()),
        }
    }
    fn fingerprint(&self) -> u64 {
        let mut hash = StableHasher64::default();
        hash.write_delimited(self.name);
        hash.write_delimited(self.field);
        for (name, code) in &self.variants {
            hash.write_delimited(name);
            hash.write(code.to_le_bytes());
        }
        hash.finish()
    }
}

/// A strict pair of business boolean names (for example `enabled`/`disabled`).
#[derive(Debug, Clone, Copy)]
pub struct BooleanCodec {
    pub name: &'static str,
    pub field: &'static str,
    pub true_name: &'static str,
    pub false_name: &'static str,
}

impl Codec for BooleanCodec {
    fn name(&self) -> &'static str {
        self.name
    }
    fn supports(&self, field: &FieldMeta) -> bool {
        field.public_name == self.field && field.value_type == ValueType::Bool
    }
    fn parse(&self, _field: &FieldMeta, input: &str) -> Result<ScalarValue, String> {
        if self.true_name == self.false_name
            || self.true_name.is_empty()
            || self.false_name.is_empty()
        {
            return Err("invalid boolean codec configuration".to_owned());
        }
        if input == self.true_name {
            Ok(ScalarValue::Bool(true))
        } else if input == self.false_name {
            Ok(ScalarValue::Bool(false))
        } else {
            Err("unknown boolean name".to_owned())
        }
    }
    fn format_result(&self, _field: &FieldMeta, value: RowValue) -> Result<RowValue, String> {
        match value {
            RowValue::Null => Ok(RowValue::Null),
            RowValue::Bool(value) => Ok(RowValue::Text(
                if value {
                    self.true_name
                } else {
                    self.false_name
                }
                .to_owned(),
            )),
            _ => Err("boolean result has an incompatible type".to_owned()),
        }
    }
    fn fingerprint(&self) -> u64 {
        let mut hash = StableHasher64::default();
        for part in [self.name, self.field, self.true_name, self.false_name] {
            hash.write_delimited(part);
        }
        hash.finish()
    }
}

/// A configured date-only format. This codec has no implicit timezone;
/// use `TimestampCodec` for configured timezone conversion.
#[cfg(feature = "chrono")]
#[derive(Debug, Clone, Copy)]
pub struct DateCodec {
    pub name: &'static str,
    pub field: &'static str,
    pub format: &'static str,
}

#[cfg(feature = "chrono")]
impl Codec for DateCodec {
    fn name(&self) -> &'static str {
        self.name
    }
    fn supports(&self, field: &FieldMeta) -> bool {
        field.public_name == self.field && field.value_type == ValueType::Date
    }
    fn parse(&self, _field: &FieldMeta, input: &str) -> Result<ScalarValue, String> {
        chrono::NaiveDate::parse_from_str(input, self.format)
            .map(ScalarValue::Date)
            .map_err(|_| "invalid date".to_owned())
    }
    fn format_result(&self, _field: &FieldMeta, value: RowValue) -> Result<RowValue, String> {
        match value {
            RowValue::Null => Ok(RowValue::Null),
            RowValue::Date(value) => {
                use std::fmt::Write;
                let mut output = String::new();
                write!(&mut output, "{}", value.format(self.format))
                    .map_err(|_| "invalid date format".to_owned())?;
                Ok(RowValue::Text(output))
            }
            _ => Err("date result has an incompatible type".to_owned()),
        }
    }
    fn fingerprint(&self) -> u64 {
        let mut hash = StableHasher64::default();
        for part in [self.name, self.field, self.format] {
            hash.write_delimited(part);
        }
        hash.finish()
    }
}
