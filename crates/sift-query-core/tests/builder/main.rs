//! Programmatic [`QueryBuilder`] coverage: every operator helper, every
//! rejection path, and the complexity budget.

use sift_query_core::{
    Condition, CursorCodec, CursorValue, FieldId, FieldRef, FilterExpr, Operator, PagingMode,
    ParamError, ParamNames, PredicateNode, Projection, QueryBuilder, QueryLimits, QueryPolicy,
    ScalarKind, ScalarValue, SortOrder, UserQuery,
};

#[path = "../support/mod.rs"]
mod support;

use support::{CursorEntity, RichEntity, TestEntity, UnstableEntity};

fn policy() -> QueryPolicy {
    QueryPolicy::default()
}

fn field<T>(id: u16) -> FieldRef<RichEntity, T, ScalarKind> {
    FieldRef::new(FieldId(id))
}

fn with_limits(adjust: impl FnOnce(&mut QueryLimits)) -> QueryPolicy {
    let mut limits = QueryLimits::default();
    adjust(&mut limits);
    QueryPolicy::new(ParamNames::default(), limits).unwrap()
}

fn conditions<E>(query: &UserQuery<E>) -> Vec<Condition> {
    query
        .predicate()
        .nodes()
        .iter()
        .filter_map(|node| match node {
            PredicateNode::Condition(condition) => Some(condition.clone()),
            PredicateNode::And(_) | PredicateNode::Or(_) => None,
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Happy paths
// ---------------------------------------------------------------------------

mod complexity_budget;
mod field_validation;
mod happy_paths;
mod nested_filters;
mod paging;
