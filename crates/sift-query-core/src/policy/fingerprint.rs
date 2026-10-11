use super::{CompatibilityPolicy, ParamNames, QueryLimits, QueryPolicy};
use sift_query_algorithm::StableHasher64;

impl QueryPolicy {
    pub(super) fn recompute_fingerprint(&mut self) {
        let mut hash = StableHasher64::default();
        hash.write(policy_fingerprint(&self.names, &self.limits).to_le_bytes());
        for filter in &self.filters {
            hash.write(filter.fingerprint().to_le_bytes());
        }
        hash.write(self.codecs.fingerprint().to_le_bytes());
        for registration in &self.operators {
            hash.write_delimited(registration.operator.name());
            hash.write_delimited(registration.operator.template());
            hash.write([registration.operator.arity()]);
            hash.write_delimited(format!("{:?}", registration.input_type));
            hash.write_delimited(format!("{:?}", registration.field_type));
        }
        hash.write(compatibility_fingerprint(self.compatibility).to_le_bytes());
        if self.cursor_codec.is_some() {
            hash.write(0x4355_5253_4f52_0001_u64.to_le_bytes());
        }
        self.fingerprint = hash.finish();
    }
}

pub(super) fn policy_fingerprint(names: &ParamNames, limits: &QueryLimits) -> u64 {
    let mut hash = StableHasher64::default();
    for name in names.all() {
        hash.write_delimited(name);
    }
    for value in [
        usize_to_u64(limits.max_raw_query_bytes),
        usize_to_u64(limits.max_raw_items),
        usize_to_u64(limits.max_key_bytes),
        usize_to_u64(limits.max_value_bytes),
        usize_to_u64(limits.max_decoded_bytes),
        usize_to_u64(limits.max_conditions),
        usize_to_u64(limits.max_values_per_condition),
        usize_to_u64(limits.max_logical_values),
        usize_to_u64(limits.max_bind_slots),
        usize_to_u64(limits.max_sort_fields),
        usize_to_u64(limits.max_group_expr_bytes),
        usize_to_u64(limits.max_group_depth),
        usize_to_u64(limits.max_group_nodes),
        usize_to_u64(limits.max_cursor_bytes),
        usize_to_u64(limits.max_filter_steps),
        usize_to_u64(limits.max_filter_items),
        usize_to_u64(limits.max_filter_bytes),
        limits.max_filter_micros,
        u64::from(limits.max_page_size),
        limits.max_offset,
        limits.timeout_ms,
    ] {
        hash.write(value.to_le_bytes());
    }
    hash.finish()
}

pub(super) const fn compatibility_fingerprint(policy: CompatibilityPolicy) -> u64 {
    match policy {
        CompatibilityPolicy::Strict => 0,
        CompatibilityPolicy::BeanSearcher4812 => 0x4812_4812_4812_4812,
    }
}

fn usize_to_u64(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}
