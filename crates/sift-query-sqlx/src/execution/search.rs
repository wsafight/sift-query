use super::*;

impl PgSiftQuery {
    /// Executes a cursor page with one bounded list query. The query must use
    /// the cursor paging mode (`cursor=` for the first page); no COUNT or
    /// summary query is issued.
    pub async fn search_cursor<E>(
        &self,
        query: impl QueryInput<E>,
        scope: &ServerScope<E>,
    ) -> Result<CursorPage<E>, PgSearchError>
    where
        E: SearchEntity + Send + Unpin + for<'row> FromRow<'row, PgRow>,
    {
        self.observe::<E, _>(
            "search_cursor",
            async {
                let mut plan =
                    self.resolve_plan(query, scope, QueryTarget::TypedPage, "search_cursor")?;
                let size = cursor_page_size(&mut plan)?;
                if plan.force_empty || size == 0 {
                    return Ok(CursorPage::empty());
                }
                let partition = self.extensions.resolve_partition(&plan, "search_cursor")?;
                let rendered = prepare(self.render::<E>(
                    &plan,
                    "search_cursor",
                    "list",
                    partition.as_deref(),
                    RenderRequest::List,
                )?)?;
                let deadline = self.deadline::<E>();
                let connection = self.acquire(deadline).await?;
                let (_, mut data) =
                    fetch_typed::<E>(connection, rendered, deadline, TimeoutStage::ListQuery)
                        .await?;
                let has_more = data.len() > size as usize;
                if has_more {
                    data.truncate(size as usize);
                }
                let next_cursor = if has_more {
                    self.entity_cursor(&plan, data.last())?
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

    /// Dynamic map equivalent of [`Self::search_cursor`].
    pub async fn search_map_cursor<E>(
        &self,
        query: impl QueryInput<E>,
        scope: &ServerScope<E>,
    ) -> Result<CursorPage<RowMap>, PgSearchError>
    where
        E: SearchEntity,
    {
        self.observe::<E, _>(
            "search_map_cursor",
            async {
                let mut plan =
                    self.resolve_plan(query, scope, QueryTarget::MapPage, "search_map_cursor")?;
                let size = cursor_page_size(&mut plan)?;
                if plan.force_empty || size == 0 {
                    return Ok(CursorPage::empty());
                }
                let partition = self
                    .extensions
                    .resolve_partition(&plan, "search_map_cursor")?;
                let rendered = prepare(self.render::<E>(
                    &plan,
                    "search_map_cursor",
                    "list",
                    partition.as_deref(),
                    RenderRequest::List,
                )?)?;
                let deadline = self.deadline::<E>();
                let connection = self.acquire(deadline).await?;
                let (_, mut data) = fetch_maps(connection, rendered, &plan, deadline).await?;
                let has_more = data.len() > size as usize;
                if has_more {
                    data.truncate(size as usize);
                }
                // Derive the token from the last database row before any
                // formatting or result filter can remove it. Otherwise a
                // post-filtered page could repeat the same underlying rows.
                let next_cursor = if has_more {
                    self.map_cursor(&plan, data.last())?
                } else {
                    None
                };
                self.format_map_rows::<E>(&mut data)?;
                self.extensions
                    .filter_rows::<E>("search_map_cursor", &mut data)?;
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
                    .await?;
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

    /// Executes a query after deriving its mandatory server scope from a
    /// trusted application context.
    pub async fn search_authorized<E, C, P>(
        &self,
        query: impl QueryInput<E>,
        context: &C,
        provider: &P,
    ) -> Result<Page<E>, PgSearchError>
    where
        E: SearchEntity + Send + Unpin + for<'row> FromRow<'row, PgRow>,
        P: sift_query_core::ScopeProvider<C, E>,
    {
        let scope = provider.scope(context)?;
        self.search(query, &scope).await
    }

    /// Dynamic map equivalent of [`Self::search_authorized`].
    pub async fn search_map_authorized<E, C, P>(
        &self,
        query: impl QueryInput<E>,
        context: &C,
        provider: &P,
    ) -> Result<Page<RowMap>, PgSearchError>
    where
        E: SearchEntity,
        P: sift_query_core::ScopeProvider<C, E>,
    {
        let scope = provider.scope(context)?;
        self.search_map(query, &scope).await
    }

    /// Count equivalent of [`Self::search_authorized`].
    pub async fn count_authorized<E, C, P>(
        &self,
        query: impl QueryInput<E>,
        context: &C,
        provider: &P,
    ) -> Result<i64, PgSearchError>
    where
        E: SearchEntity,
        P: sift_query_core::ScopeProvider<C, E>,
    {
        let scope = provider.scope(context)?;
        self.count(query, &scope).await
    }

    /// Runs typed page enrichment only when explicitly requested by the caller.
    pub async fn search_enriched<E, R>(
        &self,
        query: impl QueryInput<E>,
        scope: &ServerScope<E>,
        enricher: &R,
    ) -> Result<Page<E>, PgSearchError>
    where
        E: SearchEntity + Send + Unpin + for<'row> FromRow<'row, PgRow>,
        R: crate::TypedPageEnricher<E>,
    {
        self.observe::<E, _>(
            "search_enriched",
            async {
                let mut page = self.search(query, scope).await?;
                let expected_rows = page.data.len();
                page.data = tokio::time::timeout(
                    self.execution.timeout,
                    enricher.enrich(
                        page.data,
                        crate::EnrichContext {
                            entity_name: E::META.entity_name,
                            operation: "search_enriched",
                        },
                    ),
                )
                .await
                .map_err(|_| PgSearchError::Timeout(TimeoutStage::Enrichment))??;
                if page.data.len() != expected_rows {
                    return Err(crate::ExtensionError::rejected(
                        "typed_page_enricher",
                        "enrichment must preserve row count",
                    )
                    .into());
                }
                Ok(page)
            },
            |page| page.data.len(),
        )
        .await
    }

    pub async fn search<E>(
        &self,
        query: impl QueryInput<E>,
        scope: &ServerScope<E>,
    ) -> Result<Page<E>, PgSearchError>
    where
        E: SearchEntity + Send + Unpin + for<'row> FromRow<'row, PgRow>,
    {
        self.search_typed(query, scope, None).await
    }

    /// Executes a page with an additional server-owned HAVING scope.
    pub async fn search_with_group_scope<E>(
        &self,
        query: impl QueryInput<E>,
        scope: &ServerScope<E>,
        group_scope: &GroupScope<E>,
    ) -> Result<Page<E>, PgSearchError>
    where
        E: SearchEntity + Send + Unpin + for<'row> FromRow<'row, PgRow>,
    {
        self.search_typed_with_scopes(query, scope, group_scope, None)
            .await
    }

    pub async fn search_with<E>(
        &self,
        query: impl QueryInput<E>,
        scope: &ServerScope<E>,
        options: &SearchOptions<E>,
    ) -> Result<Page<E>, PgSearchError>
    where
        E: SearchEntity + Send + Unpin + for<'row> FromRow<'row, PgRow>,
    {
        self.search_typed(query, scope, Some(options)).await
    }

    async fn search_typed<E>(
        &self,
        query: impl QueryInput<E>,
        scope: &ServerScope<E>,
        options: Option<&SearchOptions<E>>,
    ) -> Result<Page<E>, PgSearchError>
    where
        E: SearchEntity + Send + Unpin + for<'row> FromRow<'row, PgRow>,
    {
        self.search_typed_with_scopes(query, scope, &GroupScope::unrestricted(), options)
            .await
    }

    /// Executes a typed page with both server scopes and optional summaries.
    pub async fn search_typed_with_scopes<E>(
        &self,
        query: impl QueryInput<E>,
        scope: &ServerScope<E>,
        group_scope: &GroupScope<E>,
        options: Option<&SearchOptions<E>>,
    ) -> Result<Page<E>, PgSearchError>
    where
        E: SearchEntity + Send + Unpin + for<'row> FromRow<'row, PgRow>,
    {
        self.observe::<E, _>(
            "search",
            async {
                let plan = self.resolve_plan_with_scopes(
                    query,
                    scope,
                    group_scope,
                    QueryTarget::TypedPage,
                    "search",
                )?;
                tracing::debug!("query plan ready");
                if plan.force_empty {
                    return Ok(Page::empty());
                }
                let partition = self.extensions.resolve_partition(&plan, "search")?;
                let deadline = self.deadline::<E>();
                if options.is_none()
                    && self.execution.count_strategy == CountStrategy::WindowWhenPossible
                    && !matches!(self.execution.strategy, ExecutionStrategy::Snapshot(_))
                    && self.renderer.supports_window_count(&plan)
                {
                    let window = prepare(self.render::<E>(
                        &plan,
                        "search",
                        "window_list",
                        partition.as_deref(),
                        RenderRequest::Window,
                    )?)?;
                    let fallback = prepare(self.render::<E>(
                        &plan,
                        "search",
                        "window_fallback_count",
                        partition.as_deref(),
                        RenderRequest::Cluster(None),
                    )?)?;
                    return self
                        .execute_window_typed::<E, E>(window, fallback, deadline)
                        .await;
                }
                let list = (plan.paging.size > 0)
                    .then(|| {
                        self.render::<E>(
                            &plan,
                            "search",
                            "list",
                            partition.as_deref(),
                            RenderRequest::List,
                        )
                    })
                    .transpose()?
                    .map(prepare)
                    .transpose()?;
                let cluster = prepare(self.render::<E>(
                    &plan,
                    "search",
                    "cluster",
                    partition.as_deref(),
                    RenderRequest::Cluster(options),
                )?)?;
                let mut page = self
                    .execute_typed_pair::<E, E>(list, cluster, options, deadline)
                    .await?;
                page.next_cursor = if page.data.len() == plan.paging.size as usize {
                    self.entity_cursor(&plan, page.data.last())?
                } else {
                    None
                };
                Ok(page)
            },
            |page| page.data.len(),
        )
        .await
    }

    pub async fn search_map<E>(
        &self,
        query: impl QueryInput<E>,
        scope: &ServerScope<E>,
    ) -> Result<Page<RowMap>, PgSearchError>
    where
        E: SearchEntity,
    {
        self.search_map_with_options(query, scope, None).await
    }

    /// Executes a dynamic map page with an additional server-owned HAVING scope.
    pub async fn search_map_with_group_scope<E>(
        &self,
        query: impl QueryInput<E>,
        scope: &ServerScope<E>,
        group_scope: &GroupScope<E>,
    ) -> Result<Page<RowMap>, PgSearchError>
    where
        E: SearchEntity,
    {
        self.search_map_with_scopes(query, scope, group_scope, None)
            .await
    }

    pub async fn search_map_with<E>(
        &self,
        query: impl QueryInput<E>,
        scope: &ServerScope<E>,
        options: &SearchOptions<E>,
    ) -> Result<Page<RowMap>, PgSearchError>
    where
        E: SearchEntity,
    {
        self.search_map_with_options(query, scope, Some(options))
            .await
    }

    async fn search_map_with_options<E>(
        &self,
        query: impl QueryInput<E>,
        scope: &ServerScope<E>,
        options: Option<&SearchOptions<E>>,
    ) -> Result<Page<RowMap>, PgSearchError>
    where
        E: SearchEntity,
    {
        self.search_map_with_scopes(query, scope, &GroupScope::unrestricted(), options)
            .await
    }

    /// Executes a map page with both server scopes and optional summaries.
    pub async fn search_map_with_scopes<E>(
        &self,
        query: impl QueryInput<E>,
        scope: &ServerScope<E>,
        group_scope: &GroupScope<E>,
        options: Option<&SearchOptions<E>>,
    ) -> Result<Page<RowMap>, PgSearchError>
    where
        E: SearchEntity,
    {
        self.observe::<E, _>(
            "search_map",
            async {
                let plan = self.resolve_plan_with_scopes(
                    query,
                    scope,
                    group_scope,
                    QueryTarget::MapPage,
                    "search_map",
                )?;
                tracing::debug!("query plan ready");
                if plan.force_empty {
                    return Ok(Page::empty());
                }
                let partition = self.extensions.resolve_partition(&plan, "search_map")?;
                let deadline = self.deadline::<E>();
                if options.is_none()
                    && self.execution.count_strategy == CountStrategy::WindowWhenPossible
                    && !matches!(self.execution.strategy, ExecutionStrategy::Snapshot(_))
                    && self.renderer.supports_window_count(&plan)
                {
                    let window = prepare(self.render::<E>(
                        &plan,
                        "search_map",
                        "window_list",
                        partition.as_deref(),
                        RenderRequest::Window,
                    )?)?;
                    let fallback = prepare(self.render::<E>(
                        &plan,
                        "search_map",
                        "window_fallback_count",
                        partition.as_deref(),
                        RenderRequest::Cluster(None),
                    )?)?;
                    let mut page = self
                        .execute_window_map(&plan, window, fallback, deadline)
                        .await?;
                    self.format_map_rows::<E>(&mut page.data)?;
                    self.extensions
                        .filter_rows::<E>("search_map", &mut page.data)?;
                    return Ok(self
                        .extensions
                        .enrich_page(E::META.entity_name, "search_map", page)
                        .await?);
                }
                let list = (plan.paging.size > 0)
                    .then(|| {
                        self.render::<E>(
                            &plan,
                            "search_map",
                            "list",
                            partition.as_deref(),
                            RenderRequest::List,
                        )
                    })
                    .transpose()?
                    .map(prepare)
                    .transpose()?;
                let cluster = prepare(self.render::<E>(
                    &plan,
                    "search_map",
                    "cluster",
                    partition.as_deref(),
                    RenderRequest::Cluster(options),
                )?)?;
                let mut page = self
                    .execute_map_pair(&plan, list, cluster, options, deadline)
                    .await?;
                page.next_cursor = if page.data.len() == plan.paging.size as usize {
                    self.map_cursor(&plan, page.data.last())?
                } else {
                    None
                };
                self.format_map_rows::<E>(&mut page.data)?;
                self.extensions
                    .filter_rows::<E>("search_map", &mut page.data)?;
                Ok(self
                    .extensions
                    .enrich_page(E::META.entity_name, "search_map", page)
                    .await?)
            },
            |page| page.data.len(),
        )
        .await
    }
}

pub(crate) fn cursor_page_size<E: SearchEntity>(
    plan: &mut QueryPlan<E>,
) -> Result<u32, PgSearchError> {
    if !matches!(plan.paging.mode, sift_query_core::PagingMode::Cursor(_)) {
        return Err(ParamError::InvalidValue {
            field: "cursor".to_owned(),
            reason: "cursor page API requires cursor paging mode".to_owned(),
        }
        .into());
    }
    let size = plan.paging.size;
    if size > 0 {
        plan.paging.size = size.checked_add(1).ok_or_else(|| {
            ParamError::PagingOutOfRange("cursor page size is too large".to_owned())
        })?;
    }
    Ok(size)
}
