//! Strict external parameter protocols and their adapters.

pub(crate) mod parser;
pub(crate) mod raw;
mod rpc;
#[cfg(feature = "rsql")]
pub mod rsql;
