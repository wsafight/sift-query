use std::any::TypeId;
use std::collections::{HashMap, HashSet};
use std::marker::PhantomData;

use sift_query_core::{EntityMeta, SearchEntity};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PoolRouteError {
    #[error("invalid pool name")]
    InvalidName,
    #[error("duplicate pool name: {0}")]
    DuplicateName(String),
    #[error("pool is not registered: {0}")]
    MissingPool(String),
    #[error("entity has no pool route: {0}")]
    MissingEntity(&'static str),
    #[error("pool selection rejected: {0}")]
    Rejected(String),
}

/// Selects a server-owned pool label from trusted application context.
/// Request query parameters must never be passed to this hook.
pub trait PoolSelector<C>: Send + Sync {
    fn select_pool(
        &self,
        entity: &'static EntityMeta,
        context: &C,
    ) -> Result<String, PoolRouteError>;
}

/// Backend-neutral routing table. SQL execution remains backend-specific;
/// this type only resolves an entity and trusted context to a registered pool
/// label. Each backend can use the result to select its concrete pool type.
#[derive(Debug, Clone)]
pub struct ContextPoolRouter<C> {
    default: Option<String>,
    named: HashSet<String>,
    routes: HashMap<TypeId, String>,
    marker: PhantomData<fn() -> C>,
}

impl<C> Default for ContextPoolRouter<C> {
    fn default() -> Self {
        Self {
            default: None,
            named: HashSet::new(),
            routes: HashMap::new(),
            marker: PhantomData,
        }
    }
}

impl<C> ContextPoolRouter<C> {
    pub fn register_pool(mut self, name: impl Into<String>) -> Result<Self, PoolRouteError> {
        let name = validate_name(name.into())?;
        let first = self.default.is_none();
        if !self.named.insert(name.clone()) {
            return Err(PoolRouteError::DuplicateName(name));
        }
        if first {
            self.default = Some(name);
        }
        Ok(self)
    }

    pub fn with_default(mut self, name: impl Into<String>) -> Result<Self, PoolRouteError> {
        let name = validate_name(name.into())?;
        if !self.named.contains(&name) {
            return Err(PoolRouteError::MissingPool(name));
        }
        self.default = Some(name);
        Ok(self)
    }

    pub fn route_entity<E: SearchEntity>(
        mut self,
        name: impl Into<String>,
    ) -> Result<Self, PoolRouteError> {
        let name = validate_name(name.into())?;
        if !self.named.contains(&name) {
            return Err(PoolRouteError::MissingPool(name));
        }
        self.routes.insert(TypeId::of::<E>(), name);
        Ok(self)
    }

    pub fn select<E: SearchEntity, S: PoolSelector<C>>(
        &self,
        context: &C,
        selector: &S,
    ) -> Result<String, PoolRouteError> {
        let fallback = self
            .routes
            .get(&TypeId::of::<E>())
            .or(self.default.as_ref())
            .ok_or(PoolRouteError::MissingEntity(E::META.entity_name))?;
        let selected = selector.select_pool(E::META, context)?;
        if selected.is_empty() {
            return Ok(fallback.clone());
        }
        if !self.named.contains(&selected) {
            return Err(PoolRouteError::MissingPool(selected));
        }
        Ok(selected)
    }

    #[must_use]
    pub fn route_for<E: SearchEntity>(&self) -> Option<&str> {
        self.routes
            .get(&TypeId::of::<E>())
            .or(self.default.as_ref())
            .map(String::as_str)
    }
}

fn validate_name(name: String) -> Result<String, PoolRouteError> {
    if name.is_empty()
        || name.len() > 64
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return Err(PoolRouteError::InvalidName);
    }
    Ok(name)
}
