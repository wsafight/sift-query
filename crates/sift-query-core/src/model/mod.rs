//! Static entity metadata and query/result value representations.

pub(crate) mod meta;
#[cfg(feature = "postgres-types")]
pub(crate) mod native;
pub(crate) mod value;
