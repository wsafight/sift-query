//! Service-owned row and group authorization scopes.
use crate::policy::budget::logical_values;
use crate::{
    Condition, FieldId, FieldRef, ParamError, PredicateArena, PredicateNode, SearchEntity,
};
mod group;
mod policy;
mod row;
pub use group::GroupScopeBuilder;
pub use policy::ScopeProvider;
pub use row::ServerScopeBuilder;

use std::marker::PhantomData;

/// Server-owned field visibility and queryability policy.
pub struct FieldAccessPolicy<E: SearchEntity> {
    schema_fingerprint: u64,
    allowed: Vec<bool>,
    _entity: PhantomData<fn() -> E>,
}

impl<E: SearchEntity> Clone for FieldAccessPolicy<E> {
    fn clone(&self) -> Self {
        Self {
            schema_fingerprint: self.schema_fingerprint,
            allowed: self.allowed.clone(),
            _entity: PhantomData,
        }
    }
}

impl<E: SearchEntity> std::fmt::Debug for FieldAccessPolicy<E> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("FieldAccessPolicy")
            .field("schema_fingerprint", &self.schema_fingerprint)
            .field("allowed", &self.allowed)
            .finish()
    }
}

impl<E: SearchEntity> PartialEq for FieldAccessPolicy<E> {
    fn eq(&self, other: &Self) -> bool {
        self.schema_fingerprint == other.schema_fingerprint && self.allowed == other.allowed
    }
}

impl<E: SearchEntity> Eq for FieldAccessPolicy<E> {}

impl<E: SearchEntity> FieldAccessPolicy<E> {
    #[must_use]
    pub fn unrestricted() -> Self {
        Self {
            schema_fingerprint: E::META.schema_fingerprint,
            allowed: vec![true; E::META.fields.len()],
            _entity: PhantomData,
        }
    }

    #[must_use]
    pub fn none() -> Self {
        Self {
            schema_fingerprint: E::META.schema_fingerprint,
            allowed: vec![false; E::META.fields.len()],
            _entity: PhantomData,
        }
    }

    #[must_use]
    pub fn builder() -> FieldAccessPolicyBuilder<E> {
        FieldAccessPolicyBuilder {
            policy: Self::none(),
            error: None,
        }
    }

    #[must_use]
    pub fn allows(&self, field: FieldId) -> bool {
        self.allowed
            .get(usize::from(field.0))
            .copied()
            .unwrap_or(false)
    }

    #[must_use]
    pub fn schema_fingerprint(&self) -> u64 {
        self.schema_fingerprint
    }

    pub(crate) fn intersect(&self, other: &Self) -> Self {
        let allowed = self
            .allowed
            .iter()
            .zip(&other.allowed)
            .map(|(left, right)| *left && *right)
            .collect();
        Self {
            schema_fingerprint: self.schema_fingerprint,
            allowed,
            _entity: PhantomData,
        }
    }
}

/// Builder for an explicit least-privilege field policy.
#[derive(Debug)]
pub struct FieldAccessPolicyBuilder<E: SearchEntity> {
    policy: FieldAccessPolicy<E>,
    error: Option<ParamError>,
}

impl<E: SearchEntity> FieldAccessPolicyBuilder<E> {
    #[must_use]
    pub fn allow<T, K>(self, field: FieldRef<E, T, K>) -> Self {
        self.allow_id(field.id())
    }

    #[must_use]
    pub fn allow_id(mut self, field: FieldId) -> Self {
        if self.error.is_some() {
            return self;
        }
        let Some(slot) = self.policy.allowed.get_mut(usize::from(field.0)) else {
            self.error = Some(ParamError::UnknownField(format!("#{}", field.0)));
            return self;
        };
        *slot = true;
        self
    }

    #[must_use]
    pub fn allow_all(mut self) -> Self {
        if self.error.is_none() {
            self.policy.allowed.fill(true);
        }
        self
    }

    pub fn build(self) -> Result<FieldAccessPolicy<E>, ParamError> {
        self.error.map_or(Ok(self.policy), Err)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScopeLimits {
    pub max_conditions: usize,
    pub max_logical_values: usize,
}

impl Default for ScopeLimits {
    fn default() -> Self {
        Self {
            max_conditions: 64,
            max_logical_values: 512,
        }
    }
}

fn validate_predicates<'a>(
    predicates: impl Iterator<Item = &'a PredicateArena> + Clone,
    limits: ScopeLimits,
) -> Result<(), ParamError> {
    let conditions = predicates
        .flat_map(PredicateArena::nodes)
        .filter_map(|node| {
            if let PredicateNode::Condition(condition) = node {
                Some(condition)
            } else {
                None
            }
        });
    validate_conditions(conditions, limits)
}

fn validate_conditions<'a>(
    conditions: impl Iterator<Item = &'a Condition> + Clone,
    limits: ScopeLimits,
) -> Result<(), ParamError> {
    let condition_count = conditions.clone().count();
    if condition_count > limits.max_conditions {
        return Err(ParamError::TooComplex("server scope condition count"));
    }
    logical_values(
        conditions.flat_map(|condition| &condition.values),
        limits.max_logical_values,
        "server scope logical value count",
    )?;
    Ok(())
}
