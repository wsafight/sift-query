use super::typed;
use sift_query_core::{CustomOperator, ValueType};

use sift_query_core::ConfigError;

#[cfg(feature = "postgres-types")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArrayQuery {
    Contains,
    ContainedBy,
    Overlap,
}
#[cfg(feature = "postgres-types")]
pub fn array(value_type: ValueType, mode: ArrayQuery) -> Result<CustomOperator, ConfigError> {
    if !matches!(value_type, ValueType::ArrayI64 | ValueType::ArrayText) {
        return Err(ConfigError::Invalid(
            "expected a native array field".to_owned(),
        ));
    }
    let (name, template) = match mode {
        ArrayQuery::Contains => ("array_contains", "{field} @> {0}"),
        ArrayQuery::ContainedBy => ("array_contained_by", "{field} <@ {0}"),
        ArrayQuery::Overlap => ("array_overlap", "{field} && {0}"),
    };
    let name = if value_type == ValueType::ArrayText {
        match mode {
            ArrayQuery::Contains => "array_text_contains",
            ArrayQuery::ContainedBy => "array_text_contained_by",
            ArrayQuery::Overlap => "array_text_overlap",
        }
    } else {
        name
    };
    Ok(typed(name, template, value_type, value_type))
}

#[cfg(feature = "postgres-types")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RangeQuery {
    Contains,
    ContainsElement,
    ContainedBy,
    Overlap,
    Left,
    Right,
    Adjacent,
}
#[cfg(feature = "postgres-types")]
pub fn range(mode: RangeQuery) -> CustomOperator {
    let (name, template, input) = match mode {
        RangeQuery::Contains => ("range_contains", "{field} @> {0}", ValueType::Int8Range),
        RangeQuery::ContainsElement => ("range_element", "{field} @> {0}", ValueType::I64),
        RangeQuery::ContainedBy => ("range_contained_by", "{field} <@ {0}", ValueType::Int8Range),
        RangeQuery::Overlap => ("range_overlap", "{field} && {0}", ValueType::Int8Range),
        RangeQuery::Left => ("range_left", "{field} << {0}", ValueType::Int8Range),
        RangeQuery::Right => ("range_right", "{field} >> {0}", ValueType::Int8Range),
        RangeQuery::Adjacent => ("range_adjacent", "{field} -|- {0}", ValueType::Int8Range),
    };
    typed(name, template, ValueType::Int8Range, input)
}
#[cfg(feature = "postgres-types")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkQuery {
    Contains,
    ContainsOrEqual,
    ContainedBy,
    ContainedByOrEqual,
    Overlap,
}
#[cfg(feature = "postgres-types")]
pub fn network(mode: NetworkQuery) -> CustomOperator {
    let (name, template) = match mode {
        NetworkQuery::Contains => ("net_contains", "{field} >> {0}"),
        NetworkQuery::ContainsOrEqual => ("net_contains_eq", "{field} >>= {0}"),
        NetworkQuery::ContainedBy => ("net_contained_by", "{field} << {0}"),
        NetworkQuery::ContainedByOrEqual => ("net_contained_by_eq", "{field} <<= {0}"),
        NetworkQuery::Overlap => ("net_overlap", "{field} && {0}"),
    };
    typed(name, template, ValueType::Network, ValueType::Network)
}
