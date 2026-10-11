use super::decode::{decode_map, decode_summaries};
use crate::*;
use sift_query_core::{
    QueryInput, QueryTarget, RowValue, SearchEntity, SearchOptions, ServerScope,
};
use sift_query_sqlx_common::Backend;
use sqlx::mysql::{MySqlConnection, MySqlRow};
use sqlx::{FromRow, MySql};
use tokio::time::timeout;

impl MySqlSiftQuery {
    pub async fn search_list<E>(
        &self,
        query: impl QueryInput<E>,
        scope: &ServerScope<E>,
    ) -> Result<Vec<E>, SearchError>
    where
        E: SearchEntity + Send + Unpin + for<'r> FromRow<'r, MySqlRow>,
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
        E: SearchEntity + Send + Unpin + for<'r> FromRow<'r, MySqlRow>,
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
        connection: &mut MySqlConnection,
        query: impl QueryInput<E>,
        scope: &ServerScope<E>,
    ) -> Result<Page<E>, SearchError>
    where
        E: SearchEntity + Send + Unpin + for<'r> FromRow<'r, MySqlRow>,
    {
        self.search_on_with(connection, query, scope, None).await
    }

    /// Executes a typed page with summaries on a caller-owned connection or
    /// transaction. The connection remains entirely under the caller's
    /// transaction lifecycle.
    pub async fn search_on_with<E>(
        &self,
        connection: &mut MySqlConnection,
        query: impl QueryInput<E>,
        scope: &ServerScope<E>,
        options: Option<&SearchOptions<E>>,
    ) -> Result<Page<E>, SearchError>
    where
        E: SearchEntity + Send + Unpin + for<'r> FromRow<'r, MySqlRow>,
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
        connection: &mut MySqlConnection,
        query: impl QueryInput<E>,
        scope: &ServerScope<E>,
        options: Option<&SearchOptions<E>>,
    ) -> Result<Page<E>, SearchError>
    where
        E: SearchEntity + Send + Unpin + for<'r> FromRow<'r, MySqlRow>,
    {
        let plan = self.plan(query, scope, QueryTarget::TypedPage)?;
        if plan.force_empty {
            return Ok(Page::empty());
        }
        let list = self.renderer.render_list(&plan)?;
        let cluster = self.renderer.render_cluster(&plan, options)?;
        let list_args = MySqlBackend::arguments(&list.binds)?;
        let data = timeout(
            self.execution.timeout,
            sqlx::query_as_with::<MySql, E, _>(sqlx::AssertSqlSafe(list.sql), list_args)
                .fetch_all(&mut *connection),
        )
        .await
        .map_err(|_| SearchError::Timeout(sift_query_sqlx_common::TimeoutStage::ListQuery))??;
        let cluster_args = MySqlBackend::arguments(&cluster.binds)?;
        let row = timeout(
            self.execution.timeout,
            sqlx::query_with(sqlx::AssertSqlSafe(cluster.sql), cluster_args)
                .fetch_one(&mut *connection),
        )
        .await
        .map_err(|_| SearchError::Timeout(sift_query_sqlx_common::TimeoutStage::ClusterQuery))??;
        let total_count = MySqlBackend::count(&row)?;
        let summaries = decode_summaries::<E>(&row, options, plan.meta)?;
        Ok(Page {
            data,
            total_count,
            summaries,
            next_cursor: None,
        })
    }

    /// Executes a dynamic map page on a caller-owned connection.
    pub async fn search_map_on<E>(
        &self,
        connection: &mut MySqlConnection,
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
        connection: &mut MySqlConnection,
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
        connection: &mut MySqlConnection,
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
        let list = self.renderer.render_list(&plan)?;
        let cluster = self.renderer.render_cluster(&plan, options)?;
        let list_args = MySqlBackend::arguments(&list.binds)?;
        let rows = timeout(
            self.execution.timeout,
            sqlx::query_with(sqlx::AssertSqlSafe(list.sql), list_args).fetch_all(&mut *connection),
        )
        .await
        .map_err(|_| SearchError::Timeout(sift_query_sqlx_common::TimeoutStage::ListQuery))??;
        let mut data = rows
            .iter()
            .map(|row| decode_map::<E>(row, &plan))
            .collect::<Result<Vec<_>, _>>()?;
        self.format_map_rows::<E>(&mut data)?;
        let cluster_args = MySqlBackend::arguments(&cluster.binds)?;
        let row = timeout(
            self.execution.timeout,
            sqlx::query_with(sqlx::AssertSqlSafe(cluster.sql), cluster_args)
                .fetch_one(&mut *connection),
        )
        .await
        .map_err(|_| SearchError::Timeout(sift_query_sqlx_common::TimeoutStage::ClusterQuery))??;
        let total_count = MySqlBackend::count(&row)?;
        let summaries = decode_summaries::<E>(&row, options, plan.meta)?;
        self.extensions
            .enrich_page(
                E::META.entity_name,
                "search_map_on",
                Page {
                    data,
                    total_count,
                    summaries,
                    next_cursor: None,
                },
            )
            .await
            .map_err(SearchError::Extension)
    }
}
