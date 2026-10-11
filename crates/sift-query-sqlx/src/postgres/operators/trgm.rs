use super::typed;
use sift_query_core::{CustomOperator, ValueType};

#[cfg(feature = "pg-trgm")]
pub fn trgm_similar() -> CustomOperator {
    typed(
        "trgm_similar",
        "{field} % {0}",
        ValueType::Text,
        ValueType::Text,
    )
}
