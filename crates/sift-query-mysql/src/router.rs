use std::sync::Arc;

use sift_query_core::SearchEntity;
use sift_query_sqlx_common::{PoolRouteError, PoolRouterTable, PoolSelector};
use sqlx::mysql::MySqlPool;

use crate::MySqlSiftQuery;

/// MySQL/MariaDB pool router. MariaDB uses the same `SQLx` driver and routing contract.
#[derive(Clone, Default)]
pub struct MySqlPoolRouter {
    inner: PoolRouterTable<MySqlSiftQuery>,
}

impl std::fmt::Debug for MySqlPoolRouter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MySqlPoolRouter")
            .field("has_default", &self.inner.has_default())
            .field("route_count", &self.inner.route_count())
            .field("named_pool_count", &self.inner.named_count())
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
            inner: PoolRouterTable::with_default(searcher),
        }
    }
    #[must_use]
    pub fn register_entity<E: SearchEntity>(self, searcher: MySqlSiftQuery) -> Self {
        Self {
            inner: self.inner.register_entity::<E>(searcher),
        }
    }
    #[must_use]
    pub fn register_pool<E: SearchEntity>(self, pool: MySqlPool) -> Self {
        self.register_entity::<E>(MySqlSiftQuery::new(pool))
    }
    pub fn register_named(
        self,
        name: &str,
        searcher: MySqlSiftQuery,
    ) -> Result<Self, PoolRouteError> {
        Ok(Self {
            inner: self.inner.register_named(name, searcher)?,
        })
    }
    pub fn route_entity<E: SearchEntity>(self, name: &str) -> Result<Self, PoolRouteError> {
        Ok(Self {
            inner: self.inner.route_entity::<E>(name)?,
        })
    }
    pub fn with_default_named(self, name: &str) -> Result<Self, PoolRouteError> {
        Ok(Self {
            inner: self.inner.with_default_named(name)?,
        })
    }
    pub fn searcher<E: SearchEntity>(&self) -> Result<Arc<MySqlSiftQuery>, PoolRouteError> {
        self.inner.searcher::<E>()
    }
    pub fn searcher_with_context<E: SearchEntity, C>(
        &self,
        context: &C,
        selector: &impl PoolSelector<C>,
    ) -> Result<Arc<MySqlSiftQuery>, PoolRouteError> {
        self.inner.searcher_with_context::<E, C>(context, selector)
    }
}
