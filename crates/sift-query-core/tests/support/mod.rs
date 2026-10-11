#![allow(dead_code, unused_imports)]

//! Shared fixtures for `sift-query-core` integration tests.
//!
//! Every test binary compiles this module independently, so a fixture that is
//! unused by one binary must not produce a warning in it.

mod errors;
mod grouped;
mod shared;
mod typed;

pub use errors::*;
pub use grouped::*;
pub use shared::*;
pub use typed::*;
