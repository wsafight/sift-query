//! Advanced entry points for a connection/transaction owned by the caller.
//!
//! These operations borrow the connection and leave all transaction and
//! session configuration to its owner. On timeout or cancellation, the
//! owner must recover or discard the connection before reusing it; the crate
//! cannot close a borrowed connection. In a transaction, database errors can
//! leave the transaction aborted and the caller must roll it back.

use sift_query_core::{
    GroupScope, QueryInput, QueryTarget, SearchEntity, SearchOptions, ServerScope,
};
use sqlx::FromRow;
use sqlx::postgres::{PgConnection, PgRow};

use crate::cache::RenderRequest;
use crate::execution::io::{
    fetch_cluster_external, fetch_maps_external, fetch_typed_external, prepare,
};
use crate::{ExecutionStrategy, Page, PgSearchError, PgSiftQuery, RowMap, TimeoutStage};

impl PgSiftQuery {
    /// Executes a typed page in an existing connection or transaction.
    ///
    /// Pass `&mut *transaction` for a sqlx transaction. This method does not
    /// begin/commit/rollback transactions or modify `statement_timeout`.
    pub async fn search_on<E>(
        &self,
        connection: &mut PgConnection,
        query: impl QueryInput<E>,
        scope: &ServerScope<E>,
    ) -> Result<Page<E>, PgSearchError>
    where
        E: SearchEntity + Send + Unpin + for<'row> FromRow<'row, PgRow>,
    {
        self.search_on_with(connection, query, scope, &GroupScope::unrestricted(), None)
            .await
    }

    /// Typed external execution with server-owned HAVING and summaries.
    pub async fn search_on_with<E>(
        &self,
        connection: &mut PgConnection,
        query: impl QueryInput<E>,
        scope: &ServerScope<E>,
        group_scope: &GroupScope<E>,
        options: Option<&SearchOptions<E>>,
    ) -> Result<Page<E>, PgSearchError>
    where
        E: SearchEntity + Send + Unpin + for<'row> FromRow<'row, PgRow>,
    {
        self.observe::<E, _>(
            "search_on",
            async {
                self.validate_external_strategy()?;
                let plan = self.resolve_plan_with_scopes(
                    query,
                    scope,
                    group_scope,
                    QueryTarget::TypedPage,
                    "search_on",
                )?;
                if plan.force_empty {
                    return Ok(empty_page());
                }
                let partition = self.extensions.resolve_partition(&plan, "search_on")?;
                let list = (plan.paging.size > 0)
                    .then(|| {
                        self.render(
                            &plan,
                            "search_on",
                            "list",
                            partition.as_deref(),
                            RenderRequest::List,
                        )
                    })
                    .transpose()?
                    .map(prepare)
                    .transpose()?;
                let cluster = prepare(self.render(
                    &plan,
                    "search_on",
                    "cluster",
                    partition.as_deref(),
                    RenderRequest::Cluster(options),
                )?)?;
                let deadline = self.deadline::<E>();
                let data = match list {
                    Some(list) => {
                        fetch_typed_external(connection, list, deadline, TimeoutStage::ListQuery)
                            .await?
                    }
                    None => Vec::new(),
                };
                let (total_count, summaries) =
                    fetch_cluster_external(connection, cluster, options, E::META, deadline).await?;
                let next_cursor = if data.len() == plan.paging.size as usize {
                    self.entity_cursor(&plan, data.last())?
                } else {
                    None
                };
                Ok(Page {
                    data,
                    total_count,
                    summaries,
                    next_cursor,
                })
            },
            |page| page.data.len(),
        )
        .await
    }

    /// Executes a dynamic map page on a caller-owned connection.
    pub async fn search_map_on<E>(
        &self,
        connection: &mut PgConnection,
        query: impl QueryInput<E>,
        scope: &ServerScope<E>,
    ) -> Result<Page<RowMap>, PgSearchError>
    where
        E: SearchEntity,
    {
        self.search_map_on_with(connection, query, scope, &GroupScope::unrestricted(), None)
            .await
    }

    /// Dynamic external execution with server-owned HAVING and summaries.
    pub async fn search_map_on_with<E>(
        &self,
        connection: &mut PgConnection,
        query: impl QueryInput<E>,
        scope: &ServerScope<E>,
        group_scope: &GroupScope<E>,
        options: Option<&SearchOptions<E>>,
    ) -> Result<Page<RowMap>, PgSearchError>
    where
        E: SearchEntity,
    {
        self.observe::<E, _>(
            "search_map_on",
            async {
                self.validate_external_strategy()?;
                let plan = self.resolve_plan_with_scopes(
                    query,
                    scope,
                    group_scope,
                    QueryTarget::MapPage,
                    "search_map_on",
                )?;
                if plan.force_empty {
                    return Ok(empty_page());
                }
                let partition = self.extensions.resolve_partition(&plan, "search_map_on")?;
                let list = (plan.paging.size > 0)
                    .then(|| {
                        self.render(
                            &plan,
                            "search_map_on",
                            "list",
                            partition.as_deref(),
                            RenderRequest::List,
                        )
                    })
                    .transpose()?
                    .map(prepare)
                    .transpose()?;
                let cluster = prepare(self.render(
                    &plan,
                    "search_map_on",
                    "cluster",
                    partition.as_deref(),
                    RenderRequest::Cluster(options),
                )?)?;
                let deadline = self.deadline::<E>();
                let mut data = match list {
                    Some(list) => fetch_maps_external(connection, list, &plan, deadline).await?,
                    None => Vec::new(),
                };
                let (total_count, summaries) =
                    fetch_cluster_external(connection, cluster, options, E::META, deadline).await?;
                let next_cursor = if data.len() == plan.paging.size as usize {
                    self.map_cursor(&plan, data.last())?
                } else {
                    None
                };
                self.format_map_rows::<E>(&mut data)?;
                self.extensions
                    .filter_rows::<E>("search_map_on", &mut data)?;
                Ok(self
                    .extensions
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
                    .await?)
            },
            |page| page.data.len(),
        )
        .await
    }

    pub(crate) fn validate_external_strategy(&self) -> Result<(), PgSearchError> {
        if self.execution.strategy != ExecutionStrategy::SequentialBestEffort {
            return Err(PgSearchError::ExternalExecutionStrategy);
        }
        Ok(())
    }
}

fn empty_page<T>() -> Page<T> {
    Page::empty()
}
