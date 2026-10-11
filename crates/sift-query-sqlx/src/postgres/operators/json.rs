use super::typed;
use sift_query_core::{CustomOperator, ValueType};

#[cfg(feature = "json")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JsonQuery {
    Contains,
    ContainedBy,
    Key,
    #[cfg(feature = "postgres-types")]
    AnyKeys,
    #[cfg(feature = "postgres-types")]
    AllKeys,
}
#[cfg(feature = "json")]
pub fn jsonb(mode: JsonQuery) -> CustomOperator {
    match mode {
        JsonQuery::Contains => typed(
            "json_contains",
            "{field} @> {0}",
            ValueType::Json,
            ValueType::Json,
        ),
        JsonQuery::ContainedBy => typed(
            "json_contained_by",
            "{field} <@ {0}",
            ValueType::Json,
            ValueType::Json,
        ),
        JsonQuery::Key => typed(
            "json_key",
            "{field} ? {0}",
            ValueType::Json,
            ValueType::Text,
        ),
        #[cfg(feature = "postgres-types")]
        JsonQuery::AnyKeys => typed(
            "json_any_keys",
            "{field} ?| {0}",
            ValueType::Json,
            ValueType::ArrayText,
        ),
        #[cfg(feature = "postgres-types")]
        JsonQuery::AllKeys => typed(
            "json_all_keys",
            "{field} ?& {0}",
            ValueType::Json,
            ValueType::ArrayText,
        ),
    }
}
