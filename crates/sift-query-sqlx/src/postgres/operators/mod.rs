//! Server-owned, typed PostgreSQL operator definitions.
use sift_query_core::{CustomOperator, ValueType};
#[cfg(feature = "json")]
mod json;
#[cfg(feature = "postgres-types")]
mod native;
#[cfg(feature = "postgis")]
mod spatial;
mod text;
#[cfg(feature = "ltree")]
mod tree;
#[cfg(feature = "pg-trgm")]
mod trgm;
#[cfg(feature = "json")]
pub use json::{JsonQuery, jsonb};
#[cfg(feature = "postgres-types")]
pub use native::{ArrayQuery, NetworkQuery, RangeQuery, array, network, range};
#[cfg(feature = "postgis")]
pub use spatial::{EwktCodec, SpatialQuery, spatial};
pub use text::{TextConfig, TextQuery, full_text};
#[cfg(feature = "ltree")]
pub use tree::{LtreeCodec, TreeQuery, ltree};
#[cfg(feature = "pg-trgm")]
pub use trgm::trgm_similar;

fn typed(
    name: &'static str,
    template: &'static str,
    field: ValueType,
    input: ValueType,
) -> CustomOperator {
    CustomOperator::typed(name, template, 1, field, input).expect("static PostgreSQL operator")
}
