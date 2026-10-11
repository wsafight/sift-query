//! Parser-level coverage: control parameters, projections, ordering, group
//! expressions, complexity budgets, and hard limits.

use sift_query_core::{
    CompatibilityPolicy, CustomAggregate, FieldRef, ParamError, ParamNames, ParamParser,
    PredicateNode, QueryLimits, QueryPolicy, RawParams, ScalarKind, SearchEntity as _,
    SearchOptions, SortOrder, ValueType,
};

#[path = "../support/mod.rs"]
mod support;

use support::{BaseTyped, InvalidNameEntity, RichEntity, TestEntity, parse_rich};

fn rich_error(query: &str) -> ParamError {
    parse_rich(query).unwrap_err()
}

fn with_limits(adjust: impl FnOnce(&mut QueryLimits)) -> QueryPolicy {
    let mut limits = QueryLimits::default();
    adjust(&mut limits);
    QueryPolicy::new(ParamNames::default(), limits).unwrap()
}

fn rich_with(
    policy: &QueryPolicy,
    query: &str,
) -> Result<sift_query_core::UserQuery<RichEntity>, ParamError> {
    let raw = RawParams::parse(query)?;
    ParamParser::new(policy)?.parse(&raw)
}

mod complexity_budgets;
mod group_expressions;
mod ordering;
mod paging;
mod policy_configuration;
mod projection;
mod raw_parameter_limits;
mod searchoptions;
