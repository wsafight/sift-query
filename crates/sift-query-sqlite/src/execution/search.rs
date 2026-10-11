use super::{
    decode::{decode_map, decode_summaries},
    helpers::{cursor_page_size, entity_cursor, map_cursor},
};
use crate::*;
use sift_query_core::{
    GroupScope, QueryInput, QueryPlan, QueryTarget, SearchEntity, SearchOptions, ServerScope,
};
use sift_query_sqlx_common::{Backend, RenderedQuery};
use sqlx::sqlite::SqliteRow;
use sqlx::{FromRow, Sqlite};
use tokio::time::timeout;

impl SqliteSiftQuery {
    pub async fn search<E>(
        &self,
        query: impl QueryInput<E>,
        scope: &ServerScope<E>,
    ) -> Result<Page<E>, SearchError>
    where
        E: SearchEntity + Send + Unpin + for<'r> FromRow<'r, SqliteRow>,
    {
        self.observers
            .observe(
                E::META.entity_name,
                "search",
                self.search_typed_with_scopes_inner(
                    query,
                    scope,
                    &GroupScope::unrestricted(),
                    None,
                ),
                |page| page.data.len(),
            )
            .await
    }

    /// Executes a cursor page with one bounded list query. The query must use
    /// cursor paging (`cursor=` for the first page); COUNT and summaries are
    /// intentionally skipped.
    pub async fn search_cursor<E>(
        &self,
        query: impl QueryInput<E>,
        scope: &ServerScope<E>,
    ) -> Result<CursorPage<E>, SearchError>
    where
        E: SearchEntity + Send + Unpin + for<'r> FromRow<'r, SqliteRow>,
    {
        self.observers
            .observe(
                E::META.entity_name,
                "search_cursor",
                async {
                    let mut plan = self.plan(query, scope, QueryTarget::TypedPage)?;
                    let size = cursor_page_size(&mut plan)?;
                    if plan.force_empty || size == 0 {
                        return Ok(CursorPage::empty());
                    }
                    let mut data = self.fetch_typed(self.renderer.render_list(&plan)?).await?;
                    let has_more = data.len() > size as usize;
                    if has_more {
                        data.truncate(size as usize);
                    }
                    let next_cursor = if has_more {
                        entity_cursor(&self.policy, &plan, data.last())?
                    } else {
                        None
                    };
                    Ok(CursorPage {
                        data,
                        next_cursor,
                        has_more,
                    })
                },
                |page| page.data.len(),
            )
            .await
    }

    pub async fn search_with_group_scope<E>(
        &self,
        query: impl QueryInput<E>,
        scope: &ServerScope<E>,
        group_scope: &GroupScope<E>,
    ) -> Result<Page<E>, SearchError>
    where
        E: SearchEntity + Send + Unpin + for<'r> FromRow<'r, SqliteRow>,
    {
        self.search_typed_with_scopes(query, scope, group_scope, None)
            .await
    }

    pub async fn search_enriched<E, R>(
        &self,
        query: impl QueryInput<E>,
        scope: &ServerScope<E>,
        enricher: &R,
    ) -> Result<Page<E>, SearchError>
    where
        E: SearchEntity + Send + Unpin + for<'r> FromRow<'r, SqliteRow>,
        R: sift_query_sqlx_common::TypedPageEnricher<E>,
    {
        let mut page = self.search(query, scope).await?;
        let expected_rows = page.data.len();
        page.data = tokio::time::timeout(
            self.execution.timeout,
            enricher.enrich(
                page.data,
                sift_query_sqlx_common::EnrichContext {
                    entity_name: E::META.entity_name,
                    operation: "search_enriched",
                },
            ),
        )
        .await
        .map_err(|_| SearchError::Timeout(sift_query_sqlx_common::TimeoutStage::Enrichment))??;
        if page.data.len() != expected_rows {
            return Err(SearchError::Extension(
                sift_query_sqlx_common::ExtensionError::rejected(
                    "typed_page_enricher",
                    "enrichment must preserve row count",
                ),
            ));
        }
        Ok(page)
    }

    /// Executes a typed page with an optional server-owned HAVING scope and
    /// aggregate summaries.
    pub async fn search_typed_with_scopes<E>(
        &self,
        query: impl QueryInput<E>,
        scope: &ServerScope<E>,
        group_scope: &GroupScope<E>,
        options: Option<&SearchOptions<E>>,
    ) -> Result<Page<E>, SearchError>
    where
        E: SearchEntity + Send + Unpin + for<'r> FromRow<'r, SqliteRow>,
    {
        self.observers
            .observe(
                E::META.entity_name,
                "search",
                self.search_typed_with_scopes_inner(query, scope, group_scope, options),
                |page| page.data.len(),
            )
            .await
    }

    async fn search_typed_with_scopes_inner<E>(
        &self,
        query: impl QueryInput<E>,
        scope: &ServerScope<E>,
        group_scope: &GroupScope<E>,
        options: Option<&SearchOptions<E>>,
    ) -> Result<Page<E>, SearchError>
    where
        E: SearchEntity + Send + Unpin + for<'r> FromRow<'r, SqliteRow>,
    {
        let plan = self.plan_with_scopes(query, scope, group_scope, QueryTarget::TypedPage)?;
        if plan.force_empty {
            return Ok(Page::empty());
        }
        let data = self.fetch_typed(self.renderer.render_list(&plan)?).await?;
        let (total_count, summaries) = self
            .fetch_cluster(
                self.renderer.render_cluster(&plan, options)?,
                options,
                plan.meta,
            )
            .await?;
        Ok(Page {
            data,
            total_count,
            summaries,
            next_cursor: None,
        })
    }

    /// Executes a typed page and returns requested aggregate summaries.
    pub async fn search_with<E>(
        &self,
        query: impl QueryInput<E>,
        scope: &ServerScope<E>,
        options: &SearchOptions<E>,
    ) -> Result<Page<E>, SearchError>
    where
        E: SearchEntity + Send + Unpin + for<'r> FromRow<'r, SqliteRow>,
    {
        self.search_typed_with_scopes(query, scope, &GroupScope::unrestricted(), Some(options))
            .await
    }

    pub async fn search_map<E>(
        &self,
        query: impl QueryInput<E>,
        scope: &ServerScope<E>,
    ) -> Result<Page<indexmap::IndexMap<String, RowValue>>, SearchError>
    where
        E: SearchEntity,
    {
        self.observers
            .observe(
                E::META.entity_name,
                "search_map",
                self.search_map_with_scopes_inner(query, scope, &GroupScope::unrestricted(), None),
                |page| page.data.len(),
            )
            .await
    }

    /// Dynamic map equivalent of [`Self::search_cursor`].
    pub async fn search_map_cursor<E>(
        &self,
        query: impl QueryInput<E>,
        scope: &ServerScope<E>,
    ) -> Result<CursorPage<RowMap>, SearchError>
    where
        E: SearchEntity,
    {
        self.observers
            .observe(
                E::META.entity_name,
                "search_map_cursor",
                async {
                    let mut plan = self.plan(query, scope, QueryTarget::MapPage)?;
                    let size = cursor_page_size(&mut plan)?;
                    if plan.force_empty || size == 0 {
                        return Ok(CursorPage::empty());
                    }
                    let mut data = self
                        .fetch_maps(self.renderer.render_list(&plan)?, &plan)
                        .await?;
                    let has_more = data.len() > size as usize;
                    if has_more {
                        data.truncate(size as usize);
                    }
                    let next_cursor = if has_more {
                        map_cursor(&self.policy, &plan, data.last())?
                    } else {
                        None
                    };
                    let page = self
                        .extensions
                        .enrich_page(
                            E::META.entity_name,
                            "search_map_cursor",
                            Page {
                                data,
                                total_count: -1,
                                summaries: Vec::new(),
                                next_cursor: next_cursor.clone(),
                            },
                        )
                        .await
                        .map_err(SearchError::Extension)?;
                    Ok(CursorPage {
                        data: page.data,
                        next_cursor,
                        has_more,
                    })
                },
                |page| page.data.len(),
            )
            .await
    }

    pub async fn search_map_with_group_scope<E>(
        &self,
        query: impl QueryInput<E>,
        scope: &ServerScope<E>,
        group_scope: &GroupScope<E>,
    ) -> Result<Page<indexmap::IndexMap<String, RowValue>>, SearchError>
    where
        E: SearchEntity,
    {
        self.search_map_with_scopes(query, scope, group_scope, None)
            .await
    }

    pub async fn search_map_with_scopes<E>(
        &self,
        query: impl QueryInput<E>,
        scope: &ServerScope<E>,
        group_scope: &GroupScope<E>,
        options: Option<&SearchOptions<E>>,
    ) -> Result<Page<indexmap::IndexMap<String, RowValue>>, SearchError>
    where
        E: SearchEntity,
    {
        self.observers
            .observe(
                E::META.entity_name,
                "search_map",
                self.search_map_with_scopes_inner(query, scope, group_scope, options),
                |page| page.data.len(),
            )
            .await
    }

    async fn search_map_with_scopes_inner<E>(
        &self,
        query: impl QueryInput<E>,
        scope: &ServerScope<E>,
        group_scope: &GroupScope<E>,
        options: Option<&SearchOptions<E>>,
    ) -> Result<Page<indexmap::IndexMap<String, RowValue>>, SearchError>
    where
        E: SearchEntity,
    {
        let plan = self.plan_with_scopes(query, scope, group_scope, QueryTarget::MapPage)?;
        if plan.force_empty {
            return Ok(Page::empty());
        }
        let data = self
            .fetch_maps(self.renderer.render_list(&plan)?, &plan)
            .await?;
        let (total_count, summaries) = self
            .fetch_cluster(
                self.renderer.render_cluster(&plan, options)?,
                options,
                plan.meta,
            )
            .await?;
        self.extensions
            .enrich_page(
                E::META.entity_name,
                "search_map",
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

    /// Executes a dynamic map page and returns requested aggregate summaries.
    pub async fn search_map_with<E>(
        &self,
        query: impl QueryInput<E>,
        scope: &ServerScope<E>,
        options: &SearchOptions<E>,
    ) -> Result<Page<indexmap::IndexMap<String, RowValue>>, SearchError>
    where
        E: SearchEntity,
    {
        self.search_map_with_scopes(query, scope, &GroupScope::unrestricted(), Some(options))
            .await
    }

    pub async fn count<E>(
        &self,
        query: impl QueryInput<E>,
        scope: &ServerScope<E>,
    ) -> Result<i64, SearchError>
    where
        E: SearchEntity,
    {
        self.observers
            .observe(
                E::META.entity_name,
                "count",
                self.count_inner(query, scope),
                |_| 0,
            )
            .await
    }

    async fn count_inner<E>(
        &self,
        query: impl QueryInput<E>,
        scope: &ServerScope<E>,
    ) -> Result<i64, SearchError>
    where
        E: SearchEntity,
    {
        let plan = self.plan(query, scope, QueryTarget::Count)?;
        if plan.force_empty {
            return Ok(0);
        }
        self.fetch_count(self.renderer.render_count(&plan)?).await
    }

    pub(crate) async fn fetch_typed<E>(
        &self,
        rendered: RenderedQuery,
    ) -> Result<Vec<E>, SearchError>
    where
        E: Send + Unpin + for<'r> FromRow<'r, SqliteRow>,
    {
        let args = SqliteBackend::arguments(&rendered.binds)?;
        timeout(
            self.execution.timeout,
            sqlx::query_as_with::<Sqlite, E, _>(sqlx::AssertSqlSafe(rendered.sql), args)
                .fetch_all(&self.pool),
        )
        .await
        .map_err(|_| SearchError::Timeout(sift_query_sqlx_common::TimeoutStage::ListQuery))?
        .map_err(SearchError::Sql)
    }

    async fn fetch_maps<E>(
        &self,
        rendered: RenderedQuery,
        plan: &QueryPlan<E>,
    ) -> Result<Vec<indexmap::IndexMap<String, RowValue>>, SearchError>
    where
        E: SearchEntity,
    {
        let args = SqliteBackend::arguments(&rendered.binds)?;
        let rows = timeout(
            self.execution.timeout,
            sqlx::query_with(sqlx::AssertSqlSafe(rendered.sql), args).fetch_all(&self.pool),
        )
        .await
        .map_err(|_| SearchError::Timeout(sift_query_sqlx_common::TimeoutStage::ListQuery))??;
        let mut data = rows
            .iter()
            .map(|row| decode_map::<E>(row, plan))
            .collect::<Result<Vec<_>, _>>()?;
        self.format_map_rows::<E>(&mut data)?;
        Ok(data)
    }

    async fn fetch_count(&self, rendered: RenderedQuery) -> Result<i64, SearchError> {
        let args = SqliteBackend::arguments(&rendered.binds)?;
        let row = timeout(
            self.execution.timeout,
            sqlx::query_with(sqlx::AssertSqlSafe(rendered.sql), args).fetch_one(&self.pool),
        )
        .await
        .map_err(|_| SearchError::Timeout(sift_query_sqlx_common::TimeoutStage::ClusterQuery))??;
        SqliteBackend::count(&row)
    }

    async fn fetch_cluster<E: SearchEntity>(
        &self,
        rendered: RenderedQuery,
        options: Option<&SearchOptions<E>>,
        meta: &'static sift_query_core::EntityMeta,
    ) -> Result<(i64, Vec<SummaryValue>), SearchError> {
        let args = SqliteBackend::arguments(&rendered.binds)?;
        let row = timeout(
            self.execution.timeout,
            sqlx::query_with(sqlx::AssertSqlSafe(rendered.sql), args).fetch_one(&self.pool),
        )
        .await
        .map_err(|_| SearchError::Timeout(sift_query_sqlx_common::TimeoutStage::ClusterQuery))??;
        let total_count = SqliteBackend::count(&row)?;
        let summaries = decode_summaries::<E>(&row, options, meta)?;
        Ok((total_count, summaries))
    }

    pub(crate) fn plan_with_scopes<E: SearchEntity>(
        &self,
        query: impl QueryInput<E>,
        scope: &ServerScope<E>,
        group_scope: &GroupScope<E>,
        target: QueryTarget,
    ) -> Result<QueryPlan<E>, SearchError> {
        let query = query.into_user_query(&self.policy)?;
        Ok(QueryPlan::new_with_scopes(
            &query,
            scope,
            group_scope,
            target,
        )?)
    }

    pub(crate) fn plan<E: SearchEntity>(
        &self,
        query: impl QueryInput<E>,
        scope: &ServerScope<E>,
        target: QueryTarget,
    ) -> Result<QueryPlan<E>, SearchError> {
        self.plan_with_scopes(query, scope, &GroupScope::unrestricted(), target)
    }
}
