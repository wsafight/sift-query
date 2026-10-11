use super::*;

impl PgSiftQuery {
    pub async fn search_list<E>(
        &self,
        query: impl QueryInput<E>,
        scope: &ServerScope<E>,
    ) -> Result<Vec<E>, PgSearchError>
    where
        E: SearchEntity + Send + Unpin + for<'row> FromRow<'row, PgRow>,
    {
        self.observe::<E, _>(
            "search_list",
            async {
                let plan =
                    self.resolve_plan(query, scope, QueryTarget::TypedList, "search_list")?;
                if plan.force_empty || plan.paging.size == 0 {
                    return Ok(Vec::new());
                }
                let partition = self.extensions.resolve_partition(&plan, "search_list")?;
                let rendered = prepare(self.render::<E>(
                    &plan,
                    "search_list",
                    "list",
                    partition.as_deref(),
                    RenderRequest::List,
                )?)?;
                let deadline = self.deadline::<E>();
                let connection = self.acquire(deadline).await?;
                let (_, rows) =
                    fetch_typed::<E>(connection, rendered, deadline, TimeoutStage::ListQuery)
                        .await?;
                Ok(rows)
            },
            Vec::len,
        )
        .await
    }

    pub async fn search_first<E>(
        &self,
        query: impl QueryInput<E>,
        scope: &ServerScope<E>,
    ) -> Result<Option<E>, PgSearchError>
    where
        E: SearchEntity + Send + Unpin + for<'row> FromRow<'row, PgRow>,
    {
        self.observe::<E, _>(
            "search_first",
            async {
                let plan = self.resolve_plan(query, scope, QueryTarget::First, "search_first")?;
                if plan.force_empty {
                    return Ok(None);
                }
                let partition = self.extensions.resolve_partition(&plan, "search_first")?;
                let rendered = prepare(self.render::<E>(
                    &plan,
                    "search_first",
                    "list",
                    partition.as_deref(),
                    RenderRequest::List,
                )?)?;
                let deadline = self.deadline::<E>();
                let connection = self.acquire(deadline).await?;
                let (_, mut rows) =
                    fetch_typed::<E>(connection, rendered, deadline, TimeoutStage::ListQuery)
                        .await?;
                Ok(rows.pop())
            },
            |row| usize::from(row.is_some()),
        )
        .await
    }

    pub async fn count<E>(
        &self,
        query: impl QueryInput<E>,
        scope: &ServerScope<E>,
    ) -> Result<i64, PgSearchError>
    where
        E: SearchEntity,
    {
        self.observe::<E, _>(
            "count",
            async {
                let plan = self.resolve_plan(query, scope, QueryTarget::Count, "count")?;
                if plan.force_empty {
                    return Ok(0);
                }
                let partition = self.extensions.resolve_partition(&plan, "count")?;
                let rendered = prepare(self.render::<E>(
                    &plan,
                    "count",
                    "cluster",
                    partition.as_deref(),
                    RenderRequest::Cluster(None),
                )?)?;
                let deadline = self.deadline::<E>();
                let connection = self.acquire(deadline).await?;
                let (_, (count, _)) =
                    fetch_cluster::<E>(connection, rendered, None, E::META, deadline).await?;
                Ok(count)
            },
            |_| 0,
        )
        .await
    }

    pub async fn sum<E, T, K>(
        &self,
        query: impl QueryInput<E>,
        scope: &ServerScope<E>,
        field: FieldRef<E, T, K>,
    ) -> Result<Option<<T as PgSumType>::Output>, PgSearchError>
    where
        E: SearchEntity,
        T: PgSumType,
        K: SummableKind,
        <T as PgSumType>::Output: Send + Unpin + for<'row> Decode<'row, Postgres> + Type<Postgres>,
    {
        self.observe::<E, _>(
            "sum",
            async {
                let field_id = field.id();
                let plan = self.resolve_plan(query, scope, QueryTarget::Sum, "sum")?;
                if plan.force_empty {
                    return Ok(None);
                }
                let partition = self.extensions.resolve_partition(&plan, "sum")?;
                let rendered = prepare(self.render::<E>(
                    &plan,
                    "sum",
                    "cluster",
                    partition.as_deref(),
                    RenderRequest::Sum(field_id),
                )?)?;
                let deadline = self.deadline::<E>();
                let connection = self.acquire(deadline).await?;
                let (_, value) =
                    fetch_sum::<<T as PgSumType>::Output>(connection, rendered, deadline).await?;
                Ok(value)
            },
            |_| 0,
        )
        .await
    }
}
