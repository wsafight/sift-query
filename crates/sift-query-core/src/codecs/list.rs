use crate::{Codec, ConfigError, FieldMeta, RowValue, ScalarValue, ValueType};
use sift_query_algorithm::StableHasher64;

/// Literal separated database text -> typed presentation list. This does not
/// add PostgreSQL array-column operators or change the database bind type.
#[derive(Debug, Clone)]
pub struct ListResultCodec {
    name: &'static str,
    field: &'static str,
    separator: &'static str,
    item_type: ValueType,
    max_items: usize,
}
impl ListResultCodec {
    pub fn new(
        name: &'static str,
        field: &'static str,
        separator: &'static str,
        item_type: ValueType,
        max_items: usize,
    ) -> Result<Self, ConfigError> {
        if separator.is_empty() || separator.len() > 32 || max_items == 0 || max_items > 16384 {
            return Err(ConfigError::Invalid(
                "invalid list separator or item budget".to_owned(),
            ));
        }
        #[cfg(feature = "json")]
        if item_type == ValueType::Json {
            return Err(ConfigError::Invalid(
                "list items must be scalar values".to_owned(),
            ));
        }
        Ok(Self {
            name,
            field,
            separator,
            item_type,
            max_items,
        })
    }
}
impl Codec for ListResultCodec {
    fn name(&self) -> &'static str {
        self.name
    }
    fn supports(&self, field: &FieldMeta) -> bool {
        field.public_name == self.field && field.value_type == ValueType::Text
    }
    fn parse(&self, _: &FieldMeta, input: &str) -> Result<ScalarValue, String> {
        Ok(ScalarValue::Text(input.to_owned()))
    }
    fn format_input(&self, _: &FieldMeta, value: &ScalarValue) -> Result<String, String> {
        Ok(value.to_external_text())
    }
    fn format_result(&self, _: &FieldMeta, value: RowValue) -> Result<RowValue, String> {
        let text = match value {
            RowValue::Null => return Ok(RowValue::Null),
            RowValue::Text(text) => text,
            _ => return Err("list result needs a text field".to_owned()),
        };
        let mut items = Vec::new();
        if !text.trim().is_empty() {
            for part in text.split(self.separator) {
                if items.len() >= self.max_items {
                    return Err("list item budget exceeded".to_owned());
                }
                items.push(
                    ScalarValue::parse(self.item_type, part, self.field)
                        .map(RowValue::from)
                        .map_err(|_| "invalid list item".to_owned())?,
                );
            }
        }
        Ok(RowValue::List(items))
    }
    fn fingerprint(&self) -> u64 {
        let mut hash = StableHasher64::default();
        for part in [self.name, self.field, self.separator] {
            hash.write_delimited(part);
        }
        hash.write_delimited(format!("{:?}", self.item_type));
        hash.write_delimited(self.max_items.to_string());
        hash.finish()
    }
}
