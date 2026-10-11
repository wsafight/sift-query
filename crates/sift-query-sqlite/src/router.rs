use std::any::TypeId;
use std::collections::HashMap;
use std::sync::Arc;

use sift_query_core::SearchEntity;
use sift_query_sqlx_common::{PoolRouteError, PoolSelector};
use sqlx::sqlite::SqlitePool;

use crate::SqliteSiftQuery;

#[derive(Clone, Default)]
pub struct SqlitePoolRouter {
    default: Option<Arc<SqliteSiftQuery>>,
    routes: HashMap<TypeId, Arc<SqliteSiftQuery>>,
    named: HashMap<String, Arc<SqliteSiftQuery>>,
}
impl std::fmt::Debug for SqlitePoolRouter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SqlitePoolRouter")
            .field("has_default", &self.default.is_some())
            .field("route_count", &self.routes.len())
            .field("named_pool_count", &self.named.len())
            .finish_non_exhaustive()
    }
}
impl SqlitePoolRouter {
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self::with_searcher(SqliteSiftQuery::new(pool))
    }
    #[must_use]
    pub fn with_searcher(searcher: SqliteSiftQuery) -> Self {
        Self {
            default: Some(Arc::new(searcher)),
            ..Self::default()
        }
    }
    #[must_use]
    pub fn register_entity<E: SearchEntity>(mut self, searcher: SqliteSiftQuery) -> Self {
        self.routes.insert(TypeId::of::<E>(), Arc::new(searcher));
        self
    }
    #[must_use]
    pub fn register_pool<E: SearchEntity>(self, pool: SqlitePool) -> Self {
        self.register_entity::<E>(SqliteSiftQuery::new(pool))
    }
    pub fn register_named(
        mut self,
        name: &str,
        searcher: SqliteSiftQuery,
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
    pub fn searcher<E: SearchEntity>(&self) -> Result<Arc<SqliteSiftQuery>, PoolRouteError> {
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
    ) -> Result<Arc<SqliteSiftQuery>, PoolRouteError> {
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
