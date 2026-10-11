use sift_query_core::{RowValue, ScalarValue, ValueType};
use sqlx::Database;

use crate::{Dialect, SearchError};

/// Compile-time backend selection, with driver-specific binding and decoding.
/// Implementations must agree with the renderer's logical type contract.
pub trait Backend: std::fmt::Debug + Clone + Send + Sync + 'static {
    type Database: Database;
    const DIALECT: Dialect;

    fn arguments(
        values: &[ScalarValue],
    ) -> Result<<Self::Database as Database>::Arguments, SearchError>;

    fn decode(
        row: &<Self::Database as Database>::Row,
        column: &str,
        value_type: ValueType,
    ) -> Result<RowValue, SearchError>;

    fn count(row: &<Self::Database as Database>::Row) -> Result<i64, SearchError>;
}
