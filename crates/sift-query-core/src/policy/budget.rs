//! Query-specific counting rules built on the domain-independent counter.
use crate::{
    Condition, Operator, ParamError, PredicateArena, PredicateNode, QueryLimits, ScalarValue,
};
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

pub(crate) fn condition_bind_slots(condition: &Condition) -> usize {
    match condition.operator {
        Operator::IsNull
        | Operator::NotNull
        | Operator::Empty
        | Operator::NotEmpty
        | Operator::AlwaysTrue
        | Operator::AlwaysFalse => 0,
        Operator::InList | Operator::NotIn => 1,
        _ => condition.values.len(),
    }
}

pub(crate) fn validate_predicate_budget(
    predicate: &PredicateArena,
    limits: &QueryLimits,
) -> Result<(), ParamError> {
    let mut conditions = BoundedCounter::new(limits.max_conditions);
    let mut logical = BoundedCounter::new(limits.max_logical_values);
    let mut binds = BoundedCounter::new(limits.max_bind_slots);
    for node in predicate.nodes() {
        let PredicateNode::Condition(condition) = node else {
            continue;
        };
        conditions
            .try_add(1)
            .map_err(|_| ParamError::TooComplex("condition count"))?;
        for value in &condition.values {
            logical
                .try_add(value.logical_weight())
                .map_err(|_| ParamError::TooComplex("logical value count"))?;
        }
        binds
            .try_add(condition_bind_slots(condition))
            .map_err(|_| ParamError::TooComplex("database bind slot count"))?;
    }
    Ok(())
}
