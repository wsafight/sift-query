//! PostgreSQL bindings, result decoding and opt-in query capabilities.

pub(crate) mod bind;
#[cfg(feature = "postgres-search")]
mod capabilities;
pub(crate) mod decode;
#[cfg(feature = "postgres-search")]
mod operators;
#[cfg(feature = "rls")]
pub(crate) mod rls;
pub(crate) mod sum;
#[cfg(feature = "postgres-types")]
pub(crate) mod types;

#[cfg(feature = "postgres-search")]
pub use capabilities::*;
#[cfg(feature = "postgres-search")]
pub use operators::*;
