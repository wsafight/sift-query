use std::any::TypeId;
use std::collections::HashMap;
use std::sync::Arc;

use sift_query_core::SearchEntity;
use sift_query_sqlx_common::{PoolRouteError, PoolSelector};
use sqlx::mysql::MySqlPool;

use crate::MySqlSiftQuery;

/// MySQL/MariaDB pool router. MariaDB uses the same `SQLx` driver and routing contract.
#[derive(Clone, Default)]
pub struct MySqlPoolRouter {
    default: Option<Arc<MySqlSiftQuery>>,
    routes: HashMap<TypeId, Arc<MySqlSiftQuery>>,
    named: HashMap<String, Arc<MySqlSiftQuery>>,
}

impl std::fmt::Debug for MySqlPoolRouter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MySqlPoolRouter")
            .field("has_default", &self.default.is_some())
            .field("route_count", &self.routes.len())
            .field("named_pool_count", &self.named.len())
            .finish_non_exhaustive()
    }
}

impl MySqlPoolRouter {
    #[must_use]
    pub fn new(pool: MySqlPool) -> Self {
        Self::with_searcher(MySqlSiftQuery::new(pool))
    }
    #[must_use]
    pub fn with_searcher(searcher: MySqlSiftQuery) -> Self {
        Self {
            default: Some(Arc::new(searcher)),
            ..Self::default()
        }
    }
    #[must_use]
    pub fn register_entity<E: SearchEntity>(mut self, searcher: MySqlSiftQuery) -> Self {
        self.routes.insert(TypeId::of::<E>(), Arc::new(searcher));
        self
    }
    #[must_use]
    pub fn register_pool<E: SearchEntity>(self, pool: MySqlPool) -> Self {
        self.register_entity::<E>(MySqlSiftQuery::new(pool))
    }
    pub fn register_named(
        mut self,
        name: &str,
        searcher: MySqlSiftQuery,
    ) -> Result<Self, PoolRouteError> {
        validate_name(name)?;
        if self.named.contains_key(name) {
            return Err(PoolRouteError::DuplicateName(name.to_owned()));
        }
        self.named.insert(name.to_owned(), Arc::new(searcher));
        Ok(self)
    }
    pub fn route_entity<E: SearchEntity>(mut self, name: &str) -> Result<Self, PoolRouteError> {
        let searcher = self
            .named
            .get(name)
            .cloned()
            .ok_or_else(|| PoolRouteError::MissingPool(name.to_owned()))?;
        self.routes.insert(TypeId::of::<E>(), searcher);
        Ok(self)
    }
    pub fn with_default_named(mut self, name: &str) -> Result<Self, PoolRouteError> {
        self.default = Some(
            self.named
                .get(name)
                .cloned()
                .ok_or_else(|| PoolRouteError::MissingPool(name.to_owned()))?,
        );
        Ok(self)
    }
    pub fn searcher<E: SearchEntity>(&self) -> Result<Arc<MySqlSiftQuery>, PoolRouteError> {
        self.routes
            .get(&TypeId::of::<E>())
            .cloned()
            .or_else(|| self.default.clone())
            .ok_or(PoolRouteError::MissingEntity(E::META.entity_name))
    }
    pub fn searcher_with_context<E: SearchEntity, C>(
        &self,
        context: &C,
        selector: &impl PoolSelector<C>,
    ) -> Result<Arc<MySqlSiftQuery>, PoolRouteError> {
        let name = selector.select_pool(E::META, context)?;
        if name.is_empty() {
            return self.searcher::<E>();
        }
        self.named
            .get(&name)
            .cloned()
            .ok_or(PoolRouteError::MissingPool(name))
    }
}

fn validate_name(name: &str) -> Result<(), PoolRouteError> {
    if name.is_empty()
        || name.len() > 64
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return Err(PoolRouteError::InvalidName);
    }
    Ok(())
}
