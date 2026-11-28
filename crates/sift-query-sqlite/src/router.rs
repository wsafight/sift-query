use std::sync::Arc;

use sift_query_core::SearchEntity;
use sift_query_sqlx_common::{PoolRouteError, PoolRouterTable, PoolSelector};
use sqlx::sqlite::SqlitePool;

use crate::SqliteSiftQuery;

#[derive(Clone, Default)]
pub struct SqlitePoolRouter {
    inner: PoolRouterTable<SqliteSiftQuery>,
}

impl std::fmt::Debug for SqlitePoolRouter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SqlitePoolRouter")
            .field("has_default", &self.inner.has_default())
            .field("route_count", &self.inner.route_count())
            .field("named_pool_count", &self.inner.named_count())
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
            inner: PoolRouterTable::with_default(searcher),
        }
    }
    #[must_use]
    pub fn register_entity<E: SearchEntity>(self, searcher: SqliteSiftQuery) -> Self {
        Self {
            inner: self.inner.register_entity::<E>(searcher),
        }
    }
    #[must_use]
    pub fn register_pool<E: SearchEntity>(self, pool: SqlitePool) -> Self {
        self.register_entity::<E>(SqliteSiftQuery::new(pool))
    }
    pub fn register_named(
        self,
        name: &str,
        searcher: SqliteSiftQuery,
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
    pub fn searcher<E: SearchEntity>(&self) -> Result<Arc<SqliteSiftQuery>, PoolRouteError> {
        self.inner.searcher::<E>()
    }
    pub fn searcher_with_context<E: SearchEntity, C>(
        &self,
        context: &C,
        selector: &impl PoolSelector<C>,
    ) -> Result<Arc<SqliteSiftQuery>, PoolRouteError> {
        self.inner.searcher_with_context::<E, C>(context, selector)
    }
}
