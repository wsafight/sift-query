use crate::ConfigError;

/// Runtime query limits. Entity limits can only make paging values stricter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryLimits {
    pub max_raw_query_bytes: usize,
    pub max_raw_items: usize,
    pub max_key_bytes: usize,
    pub max_value_bytes: usize,
    pub max_decoded_bytes: usize,
    pub max_conditions: usize,
    pub max_values_per_condition: usize,
    pub max_logical_values: usize,
    pub max_bind_slots: usize,
    pub max_sort_fields: usize,
    pub max_group_expr_bytes: usize,
    pub max_group_depth: usize,
    pub max_group_nodes: usize,
    pub max_cursor_bytes: usize,
    pub max_filter_steps: usize,
    pub max_filter_items: usize,
    pub max_filter_bytes: usize,
    /// Total wall-clock budget for trusted synchronous filters; checked after each step.
    pub max_filter_micros: u64,
    pub max_page_size: u32,
    pub max_offset: u64,
    pub timeout_ms: u64,
}

impl Default for QueryLimits {
    fn default() -> Self {
        Self {
            max_raw_query_bytes: 64 * 1024,
            max_raw_items: 128,
            max_key_bytes: 160,
            max_value_bytes: 4 * 1024,
            max_decoded_bytes: 256 * 1024,
            max_conditions: 32,
            max_values_per_condition: 100,
            max_logical_values: 512,
            max_bind_slots: 512,
            max_sort_fields: 5,
            max_group_expr_bytes: 256,
            max_group_depth: 8,
            max_group_nodes: 64,
            max_cursor_bytes: 1024,
            max_filter_steps: 8,
            max_filter_items: 256,
            max_filter_bytes: 512 * 1024,
            max_filter_micros: 10_000,
            max_page_size: 100,
            max_offset: 20_000,
            timeout_ms: 5_000,
        }
    }
}

/// Absolute library guardrails. Ordinary configuration cannot exceed these values.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QueryHardLimits(pub QueryLimits);

pub(super) fn ensure_within_hard_limits(
    limits: &QueryLimits,
    hard: &QueryLimits,
) -> Result<(), ConfigError> {
    let within = limits.max_raw_query_bytes <= hard.max_raw_query_bytes
        && limits.max_raw_items <= hard.max_raw_items
        && limits.max_key_bytes <= hard.max_key_bytes
        && limits.max_value_bytes <= hard.max_value_bytes
        && limits.max_decoded_bytes <= hard.max_decoded_bytes
        && limits.max_conditions <= hard.max_conditions
        && limits.max_values_per_condition <= hard.max_values_per_condition
        && limits.max_logical_values <= hard.max_logical_values
        && limits.max_bind_slots <= hard.max_bind_slots
        && limits.max_sort_fields <= hard.max_sort_fields
        && limits.max_group_expr_bytes <= hard.max_group_expr_bytes
        && limits.max_group_depth <= hard.max_group_depth
        && limits.max_group_nodes <= hard.max_group_nodes
        && limits.max_cursor_bytes <= hard.max_cursor_bytes
        && limits.max_filter_steps <= hard.max_filter_steps
        && limits.max_filter_items <= hard.max_filter_items
        && limits.max_filter_bytes <= hard.max_filter_bytes
        && limits.max_filter_micros <= hard.max_filter_micros
        && limits.max_page_size <= hard.max_page_size
        && limits.max_offset <= hard.max_offset
        && limits.timeout_ms <= hard.timeout_ms;
    if within {
        Ok(())
    } else {
        Err(ConfigError::Invalid(
            "ordinary policy cannot exceed hard limits".to_owned(),
        ))
    }
}
