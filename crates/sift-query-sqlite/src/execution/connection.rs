use super::decode::{decode_map, decode_summaries};
use super::helpers::{entity_cursor, map_cursor};
use crate::*;
use sift_query_core::{
    QueryInput, QueryTarget, RowValue, SearchEntity, SearchOptions, ServerScope,
};
use sift_query_sqlx_common::{Backend, TimeoutStage};
use sqlx::sqlite::{SqliteConnection, SqliteRow};
use sqlx::{FromRow, Sqlite};
use tokio::time::{Instant, timeout_at};

impl SqliteSiftQuery {
    pub async fn search_list<E>(
        &self,
        query: impl QueryInput<E>,
        scope: &ServerScope<E>,
    ) -> Result<Vec<E>, SearchError>
    where
        E: SearchEntity + Send + Unpin + for<'r> FromRow<'r, SqliteRow>,
    {
        self.observers
            .observe(
                E::META.entity_name,
                "search_list",
                self.search_list_inner(query, scope),
                Vec::len,
            )
            .await
    }

    async fn search_list_inner<E>(
        &self,
        query: impl QueryInput<E>,
        scope: &ServerScope<E>,
    ) -> Result<Vec<E>, SearchError>
    where
        E: SearchEntity + Send + Unpin + for<'r> FromRow<'r, SqliteRow>,
    {
        let plan = self.plan(query, scope, QueryTarget::TypedList)?;
        if plan.force_empty || plan.paging.size == 0 {
            return Ok(Vec::new());
        }
        self.fetch_typed(self.renderer.render_list(&plan)?).await
    }

    /// Executes a typed page on a caller-owned connection or transaction.
    pub async fn search_on<E>(
        &self,
        connection: &mut SqliteConnection,
        query: impl QueryInput<E>,
        scope: &ServerScope<E>,
    ) -> Result<Page<E>, SearchError>
    where
        E: SearchEntity + Send + Unpin + for<'r> FromRow<'r, SqliteRow>,
    {
        self.search_on_with(connection, query, scope, None).await
    }

    /// Executes a typed page with summaries on a caller-owned connection or
    /// transaction. The connection remains entirely under the caller's
    /// transaction lifecycle.
    pub async fn search_on_with<E>(
        &self,
        connection: &mut SqliteConnection,
        query: impl QueryInput<E>,
        scope: &ServerScope<E>,
        options: Option<&SearchOptions<E>>,
    ) -> Result<Page<E>, SearchError>
    where
        E: SearchEntity + Send + Unpin + for<'r> FromRow<'r, SqliteRow>,
    {
        self.observers
            .observe(
                E::META.entity_name,
                "search_on",
                self.search_on_with_inner(connection, query, scope, options),
                |page| page.data.len(),
            )
            .await
    }

    async fn search_on_with_inner<E>(
        &self,
        connection: &mut SqliteConnection,
        query: impl QueryInput<E>,
        scope: &ServerScope<E>,
        options: Option<&SearchOptions<E>>,
    ) -> Result<Page<E>, SearchError>
    where
        E: SearchEntity + Send + Unpin + for<'r> FromRow<'r, SqliteRow>,
    {
        let plan = self.plan(query, scope, QueryTarget::TypedPage)?;
        if plan.force_empty {
            return Ok(Page::empty());
        }
        let list = (plan.paging.size > 0)
            .then(|| self.renderer.render_list(&plan))
            .transpose()?;
        let cluster = self.renderer.render_cluster(&plan, options)?;
        let deadline = Instant::now() + self.execution.timeout;
        let data = match list {
            Some(rendered) => {
                let list_args = SqliteBackend::arguments(&rendered.binds)?;
                timeout_at(
                    deadline,
                    sqlx::query_as_with::<Sqlite, E, _>(
                        sqlx::AssertSqlSafe(rendered.sql),
                        list_args,
                    )
                    .fetch_all(&mut *connection),
                )
                .await
                .map_err(|_| SearchError::Timeout(TimeoutStage::ListQuery))??
            }
            None => Vec::new(),
        };
        let cluster_args = SqliteBackend::arguments(&cluster.binds)?;
        let row = timeout_at(
            deadline,
            sqlx::query_with(sqlx::AssertSqlSafe(cluster.sql), cluster_args)
                .fetch_one(&mut *connection),
        )
        .await
        .map_err(|_| SearchError::Timeout(TimeoutStage::ClusterQuery))??;
        let total_count = SqliteBackend::count(&row)?;
        let summaries = decode_summaries::<E>(&row, options, plan.meta)?;
        let next_cursor = if data.len() == plan.paging.size as usize {
            entity_cursor(&self.policy, &plan, data.last())?
        } else {
            None
        };
        Ok(Page {
            data,
            total_count,
            summaries,
            next_cursor,
        })
    }

    /// Executes a dynamic map page on a caller-owned connection.
    pub async fn search_map_on<E>(
        &self,
        connection: &mut SqliteConnection,
        query: impl QueryInput<E>,
        scope: &ServerScope<E>,
    ) -> Result<Page<indexmap::IndexMap<String, RowValue>>, SearchError>
    where
        E: SearchEntity,
    {
        self.search_map_on_with(connection, query, scope, None)
            .await
    }

    /// Executes a dynamic map page with summaries on a caller-owned
    /// connection or transaction.
    pub async fn search_map_on_with<E>(
        &self,
        connection: &mut SqliteConnection,
        query: impl QueryInput<E>,
        scope: &ServerScope<E>,
        options: Option<&SearchOptions<E>>,
    ) -> Result<Page<indexmap::IndexMap<String, RowValue>>, SearchError>
    where
        E: SearchEntity,
    {
        self.observers
            .observe(
                E::META.entity_name,
                "search_map_on",
                self.search_map_on_with_inner(connection, query, scope, options),
                |page| page.data.len(),
            )
            .await
    }

    async fn search_map_on_with_inner<E>(
        &self,
        connection: &mut SqliteConnection,
        query: impl QueryInput<E>,
        scope: &ServerScope<E>,
        options: Option<&SearchOptions<E>>,
    ) -> Result<Page<indexmap::IndexMap<String, RowValue>>, SearchError>
    where
        E: SearchEntity,
    {
        let plan = self.plan(query, scope, QueryTarget::MapPage)?;
        if plan.force_empty {
            return Ok(Page::empty());
        }
        let list = (plan.paging.size > 0)
            .then(|| self.renderer.render_list(&plan))
            .transpose()?;
        let cluster = self.renderer.render_cluster(&plan, options)?;
        let deadline = Instant::now() + self.execution.timeout;
        let mut data = match list {
            Some(rendered) => {
                let list_args = SqliteBackend::arguments(&rendered.binds)?;
                let rows = timeout_at(
                    deadline,
                    sqlx::query_with(sqlx::AssertSqlSafe(rendered.sql), list_args)
                        .fetch_all(&mut *connection),
                )
                .await
                .map_err(|_| SearchError::Timeout(TimeoutStage::ListQuery))??;
                rows.iter()
                    .map(|row| decode_map::<E>(row, &plan))
                    .collect::<Result<Vec<_>, _>>()?
            }
            None => Vec::new(),
        };
        let cluster_args = SqliteBackend::arguments(&cluster.binds)?;
        let row = timeout_at(
            deadline,
            sqlx::query_with(sqlx::AssertSqlSafe(cluster.sql), cluster_args)
                .fetch_one(&mut *connection),
        )
        .await
        .map_err(|_| SearchError::Timeout(TimeoutStage::ClusterQuery))??;
        let total_count = SqliteBackend::count(&row)?;
        let summaries = decode_summaries::<E>(&row, options, plan.meta)?;
        let next_cursor = if data.len() == plan.paging.size as usize {
            map_cursor(&self.policy, &plan, data.last())?
        } else {
            None
        };
        self.format_map_rows::<E>(&mut data)?;
        self.extensions
            .enrich_page(
                E::META.entity_name,
                "search_map_on",
                Page {
                    data,
                    total_count,
                    summaries,
                    next_cursor,
                },
            )
            .await
            .map_err(SearchError::Extension)
    }
}
