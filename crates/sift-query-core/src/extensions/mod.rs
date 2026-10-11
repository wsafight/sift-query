//! Bounded parameter normalization extension points.
use crate::{EntityMeta, ParamError, QueryLimits, RawParams};
use sift_query_algorithm::StableHasher64;
use std::fmt;

mod param_filters;

// Keep declarations in this module: default fingerprints use their historical
// Rust type paths. Only implementations move into the private child module.
/// Converts repeated/indexed array keys to canonical indexed parameters.
#[derive(Debug, Clone, Copy, Default)]
pub struct IndexedArrayParamFilter;

/// Expands JSON arrays into indexed parameter values before strict parsing.
#[derive(Debug, Clone, Copy, Default)]
pub struct JsonArrayParamFilter;

/// Rewrites operator suffixes to canonical parameters while preserving repeats.
#[derive(Debug, Clone, Copy, Default)]
pub struct SuffixOperatorParamFilter;

/// Adds trusted default parameters before strict parsing.
#[derive(Debug, Clone, Default)]
pub struct DefaultParamFilter {
    entries: Vec<(String, String)>,
}

/// A server-owned parameter normalization step.
pub trait RawParamFilter: Send + Sync + fmt::Debug {
    /// Normalize `input` while preserving ordered, repeated parameters.
    fn filter(
        &self,
        entity: &'static EntityMeta,
        input: &RawParams,
    ) -> Result<RawParams, ParamError>;

    /// Budget-aware entry point. Server code must perform bounded synchronous
    /// work; arbitrary extension code cannot be preempted safely by core.
    fn filter_with_limits(
        &self,
        entity: &'static EntityMeta,
        input: &RawParams,
        _limits: &QueryLimits,
    ) -> Result<RawParams, ParamError> {
        self.filter(entity, input)
    }

    /// Stable identity used by query policy fingerprints.
    fn fingerprint(&self) -> u64 {
        let mut hash = StableHasher64::default();
        hash.write_delimited(std::any::type_name::<Self>());
        hash.finish()
    }
}
