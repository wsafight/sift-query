//! Query-specific counting rules built on the domain-independent counter.
use crate::{ParamError, ScalarValue};
use sift_query_algorithm::BoundedCounter;

pub(crate) fn logical_values<'a>(
    values: impl IntoIterator<Item = &'a ScalarValue>,
    limit: usize,
    reason: &'static str,
) -> Result<usize, ParamError> {
    bounded_sum(
        values.into_iter().map(ScalarValue::logical_weight),
        limit,
        reason,
    )
}

pub(crate) fn bounded_sum(
    weights: impl IntoIterator<Item = usize>,
    limit: usize,
    reason: &'static str,
) -> Result<usize, ParamError> {
    let mut count = BoundedCounter::new(limit);
    for weight in weights {
        count
            .try_add(weight)
            .map_err(|_| ParamError::TooComplex(reason))?;
    }
    Ok(count.used())
}
