use super::typed;
use sift_query_core::{CustomOperator, ValueType};

#[cfg(feature = "ltree")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreeQuery {
    Ancestor,
    Descendant,
}
#[cfg(feature = "ltree")]
pub fn ltree(mode: TreeQuery) -> CustomOperator {
    match mode {
        TreeQuery::Ancestor => typed(
            "tree_ancestor",
            "{field} @> {0}::ltree",
            ValueType::Text,
            ValueType::Text,
        ),
        TreeQuery::Descendant => typed(
            "tree_descendant",
            "{field} <@ {0}::ltree",
            ValueType::Text,
            ValueType::Text,
        ),
    }
}
#[cfg(feature = "ltree")]
#[derive(Debug, Clone, Copy)]
pub struct LtreeCodec {
    field: &'static str,
}
#[cfg(feature = "ltree")]
impl LtreeCodec {
    pub const fn new(field: &'static str) -> Self {
        Self { field }
    }
}
#[cfg(feature = "ltree")]
impl sift_query_core::Codec for LtreeCodec {
    fn name(&self) -> &'static str {
        "ltree"
    }
    fn fingerprint(&self) -> u64 {
        let mut hash = sift_query_algorithm::StableHasher64::default();
        hash.write_delimited(self.name());
        hash.write_delimited(self.field);
        hash.finish()
    }
    fn supports(&self, field: &sift_query_core::FieldMeta) -> bool {
        field.value_type == ValueType::Text && field.public_name == self.field
    }
    fn parse(
        &self,
        _field: &sift_query_core::FieldMeta,
        input: &str,
    ) -> Result<sift_query_core::ScalarValue, String> {
        if !input.is_empty()
            && input.split('.').any(|label| {
                label.is_empty()
                    || label.chars().count() > 255
                    || !label.chars().all(|ch| ch.is_alphanumeric() || ch == '_')
            })
        {
            return Err("expected a dot separated ltree path".to_owned());
        }
        Ok(sift_query_core::ScalarValue::Text(input.to_owned()))
    }
}
