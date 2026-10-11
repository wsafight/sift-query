use sift_query_core::{
    QueryInput, QueryPlan, SearchEntity, SearchOptions, SearchProjection, ServerScope,
};
use sqlx::FromRow;
use sqlx::postgres::PgRow;

use crate::cache::RenderRequest;
use crate::execution::io::{fetch_typed, prepare};
use crate::{CountStrategy, CursorPage, ExecutionStrategy, Page, PgSearchError, PgSiftQuery};

impl PgSiftQuery {
    /// Executes a static typed projection as a cursor page without a count.
    pub async fn search_as_cursor<E, P>(
        &self,
        query: impl QueryInput<E>,
        scope: &ServerScope<E>,
    ) -> Result<CursorPage<P>, PgSearchError>
    where
        E: SearchEntity,
        P: SearchProjection<E> + Send + Unpin + for<'row> FromRow<'row, PgRow>,
    {
        self.observe::<E, _>(
            "search_as_cursor",
            async {
                let query = query.into_user_query(&self.policy)?;
                let mut plan = QueryPlan::for_projection::<P>(&query, scope)?;
                let size = super::search::cursor_page_size(&mut plan)?;
                if plan.force_empty || size == 0 {
                    return Ok(CursorPage::empty());
                }
                let partition = self
                    .extensions
                    .resolve_partition(&plan, "search_as_cursor")?;
                let rendered = prepare(self.render::<E>(
                    &plan,
                    "search_as_cursor",
                    "static_projection",
                    partition.as_deref(),
                    RenderRequest::List,
                )?)?;
                let deadline = self.deadline::<E>();
                let connection = self.acquire(deadline).await?;
                let (_, mut data) = fetch_typed::<P>(
                    connection,
                    rendered,
                    deadline,
                    crate::TimeoutStage::ListQuery,
                )
                .await?;
                let has_more = data.len() > size as usize;
                if has_more {
                    data.truncate(size as usize);
                }
                let next_cursor = if has_more {
                    self.projection_cursor::<E, P>(&plan, data.last())?
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

    pub async fn search_as<E, P>(
        &self,
        query: impl QueryInput<E>,
        scope: &ServerScope<E>,
    ) -> Result<Page<P>, PgSearchError>
    where
        E: SearchEntity,
        P: SearchProjection<E> + Send + Unpin + for<'row> FromRow<'row, PgRow>,
    {
        self.search_as_with_options(query, scope, None).await
    }

    pub async fn search_as_with<E, P>(
        &self,
        query: impl QueryInput<E>,
        scope: &ServerScope<E>,
        options: &SearchOptions<E>,
    ) -> Result<Page<P>, PgSearchError>
    where
        E: SearchEntity,
        P: SearchProjection<E> + Send + Unpin + for<'row> FromRow<'row, PgRow>,
    {
        self.search_as_with_options(query, scope, Some(options))
            .await
    }

    async fn search_as_with_options<E, P>(
        &self,
        query: impl QueryInput<E>,
        scope: &ServerScope<E>,
        options: Option<&SearchOptions<E>>,
    ) -> Result<Page<P>, PgSearchError>
    where
        E: SearchEntity,
        P: SearchProjection<E> + Send + Unpin + for<'row> FromRow<'row, PgRow>,
    {
        self.observe::<E, _>(
            "search_as",
            async {
                let query = query.into_user_query(&self.policy)?;
                let plan = QueryPlan::for_projection::<P>(&query, scope)?;
                if plan.force_empty {
                    return Ok(Page::empty());
                }
                let partition = self.extensions.resolve_partition(&plan, "search_as")?;
                let deadline = self.deadline::<E>();
                if options.is_none()
                    && self.execution.count_strategy == CountStrategy::WindowWhenPossible
                    && !matches!(self.execution.strategy, ExecutionStrategy::Snapshot(_))
                    && self.renderer.supports_window_count(&plan)
                {
                    let window = prepare(self.render::<E>(
                        &plan,
                        "search_as",
                        "window_list",
                        partition.as_deref(),
                        RenderRequest::Window,
                    )?)?;
                    let fallback = prepare(self.render::<E>(
                        &plan,
                        "search_as",
                        "window_fallback_count",
                        partition.as_deref(),
                        RenderRequest::Cluster(None),
                    )?)?;
                    return self
                        .execute_window_typed::<E, P>(window, fallback, deadline)
                        .await;
                }
                let list = (plan.paging.size > 0)
                    .then(|| {
                        self.render::<E>(
                            &plan,
                            "search_as",
                            "static_projection",
                            partition.as_deref(),
                            RenderRequest::List,
                        )
                    })
                    .transpose()?
                    .map(prepare)
                    .transpose()?;
                let cluster = prepare(self.render::<E>(
                    &plan,
                    "search_as",
                    "cluster",
                    partition.as_deref(),
                    RenderRequest::Cluster(options),
                )?)?;
                let mut page = self
                    .execute_typed_pair::<E, P>(list, cluster, options, deadline)
                    .await?;
                page.next_cursor =
                    if matches!(plan.paging.mode, sift_query_core::PagingMode::Cursor(_))
                        && page.data.len() == plan.paging.size as usize
                    {
                        self.projection_cursor(&plan, page.data.last())?
                    } else {
                        None
                    };
                Ok(page)
            },
            |page| page.data.len(),
        )
        .await
    }
}
