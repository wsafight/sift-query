use std::any::TypeId;
use std::collections::HashMap;
use std::sync::Arc;

use sift_query_core::{EntityMeta, QueryInput, SearchEntity, ServerScope};
use sqlx::postgres::PgPool;

use crate::{Page, PgSearchError, PgSiftQuery, RowMap};

#[derive(Debug, thiserror::Error)]
pub enum PoolRouteError {
    #[error("no PostgreSQL pool is registered for entity {0}")]
    MissingEntity(&'static str),
    #[error("invalid PostgreSQL pool name")]
    InvalidName,
    #[error("duplicate PostgreSQL pool name: {0}")]
    DuplicateName(String),
    #[error("PostgreSQL pool is not registered: {0}")]
    MissingPool(String),
    #[error("server pool selection rejected: {0}")]
    Rejected(&'static str),
    #[error("query failed on routed PostgreSQL pool: {0}")]
    Query(#[from] PgSearchError),
}

/// Application-owned, nonblocking resolver for trusted request context.
/// Client query parameters are never supplied to this hook.
pub trait PgPoolSelector<C>: Send + Sync {
    fn select_pool(
        &self,
        entity: &'static EntityMeta,
        context: &C,
    ) -> Result<String, PoolRouteError>;
}

/// Immutable server-owned entity-to-pool routing table.
#[derive(Clone, Default)]
pub struct PgPoolRouter {
    default: Option<Arc<PgSiftQuery>>,
    routes: HashMap<TypeId, Arc<PgSiftQuery>>,
    named: HashMap<String, Arc<PgSiftQuery>>,
    labels: HashMap<TypeId, String>,
    default_label: String,
}

impl std::fmt::Debug for PgPoolRouter {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PgPoolRouter")
            .field("has_default", &self.default.is_some())
            .field("route_count", &self.routes.len())
            .field("named_pool_count", &self.named.len())
            .finish_non_exhaustive()
    }
}

impl PgPoolRouter {
    #[must_use]
    pub fn new(default: PgPool) -> Self {
        Self {
            default: Some(Arc::new(PgSiftQuery::new(default))),
            routes: HashMap::new(),
            default_label: "default".to_owned(),
            ..Self::default()
        }
    }

    #[must_use]
    pub fn with_searcher(default: PgSiftQuery) -> Self {
        Self {
            default: Some(Arc::new(default)),
            routes: HashMap::new(),
            default_label: "default".to_owned(),
            ..Self::default()
        }
    }

    #[must_use]
    pub fn register_entity<E: SearchEntity>(mut self, searcher: PgSiftQuery) -> Self {
        self.routes.insert(TypeId::of::<E>(), Arc::new(searcher));
        self.labels
            .insert(TypeId::of::<E>(), E::META.entity_name.to_owned());
        self
    }

    #[must_use]
    pub fn register_pool<E: SearchEntity>(mut self, pool: PgPool) -> Self {
        self.routes
            .insert(TypeId::of::<E>(), Arc::new(PgSiftQuery::new(pool)));
        self.labels
            .insert(TypeId::of::<E>(), E::META.entity_name.to_owned());
        self
    }

    /// Names are stable configuration labels, never tenant/user identifiers.
    pub fn register_named(
        mut self,
        name: &str,
        searcher: PgSiftQuery,
    ) -> Result<Self, PoolRouteError> {
        if name.is_empty()
            || name.len() > 64
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
        {
            return Err(PoolRouteError::InvalidName);
        }
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
        self.labels.insert(TypeId::of::<E>(), name.to_owned());
        Ok(self)
    }

    pub fn with_default_named(mut self, name: &str) -> Result<Self, PoolRouteError> {
        self.default = Some(
            self.named
                .get(name)
                .cloned()
                .ok_or_else(|| PoolRouteError::MissingPool(name.to_owned()))?,
        );
        name.clone_into(&mut self.default_label);
        Ok(self)
    }

    /// Selected searcher exposes normal query, scan and export entry points.
    pub fn searcher<E: SearchEntity>(&self) -> Result<Arc<PgSiftQuery>, PoolRouteError> {
        let result = self
            .routes
            .get(&TypeId::of::<E>())
            .cloned()
            .or_else(|| self.default.clone())
            .ok_or(PoolRouteError::MissingEntity(E::META.entity_name))?;
        let label = self
            .labels
            .get(&TypeId::of::<E>())
            .unwrap_or(&self.default_label);
        tracing::debug!(entity = E::META.entity_name, backend = "postgres", pool_route = %label, "selected PostgreSQL pool");
        Ok(result)
    }

    pub fn searcher_with_context<E: SearchEntity, C>(
        &self,
        context: &C,
        selector: &impl PgPoolSelector<C>,
    ) -> Result<Arc<PgSiftQuery>, PoolRouteError> {
        let name = selector.select_pool(E::META, context)?;
        if name.is_empty() {
            return self.searcher::<E>();
        }
        // Unknown selection fails closed, with no cross-pool fallback.
        let searcher = self
            .named
            .get(&name)
            .ok_or_else(|| PoolRouteError::MissingPool(name.clone()))?;
        tracing::debug!(entity = E::META.entity_name, backend = "postgres", pool_route = %name, "selected PostgreSQL pool from trusted context");
        Ok(Arc::clone(searcher))
    }

    pub async fn search_map<E: SearchEntity>(
        &self,
        query: impl QueryInput<E>,
        scope: &ServerScope<E>,
    ) -> Result<Page<RowMap>, PoolRouteError> {
        Ok(self.searcher::<E>()?.search_map(query, scope).await?)
    }

    pub async fn search_with_context<E, C>(
        &self,
        query: impl QueryInput<E>,
        scope: &ServerScope<E>,
        context: &C,
        selector: &impl PgPoolSelector<C>,
    ) -> Result<Page<E>, PoolRouteError>
    where
        E: SearchEntity + Send + Unpin + for<'row> sqlx::FromRow<'row, sqlx::postgres::PgRow>,
    {
        Ok(self
            .searcher_with_context::<E, C>(context, selector)?
            .search(query, scope)
            .await?)
    }

    pub async fn search_map_with_context<E: SearchEntity, C>(
        &self,
        query: impl QueryInput<E>,
        scope: &ServerScope<E>,
        context: &C,
        selector: &impl PgPoolSelector<C>,
    ) -> Result<Page<RowMap>, PoolRouteError> {
        Ok(self
            .searcher_with_context::<E, C>(context, selector)?
            .search_map(query, scope)
            .await?)
    }

    pub async fn search<E>(
        &self,
        query: impl QueryInput<E>,
        scope: &ServerScope<E>,
    ) -> Result<Page<E>, PoolRouteError>
    where
        E: SearchEntity + Send + Unpin + for<'row> sqlx::FromRow<'row, sqlx::postgres::PgRow>,
    {
        Ok(self.searcher::<E>()?.search(query, scope).await?)
    }
}
