use super::*;

impl PgSiftQuery {
    pub fn parse<E: SearchEntity>(
        &self,
        raw: &RawParams,
    ) -> Result<sift_query_core::UserQuery<E>, ParamError> {
        ParamParser::new(&self.policy)?.parse(raw)
    }

    pub(crate) fn deadline<E: SearchEntity>(&self) -> Instant {
        let entity_timeout = Duration::from_millis(E::META.limits.timeout_ms);
        Instant::now() + self.execution.timeout.min(entity_timeout)
    }

    pub(crate) fn format_map_rows<E: SearchEntity>(
        &self,
        rows: &mut [RowMap],
    ) -> Result<(), PgSearchError> {
        let mut error = None;
        self.policy.for_each_codec_binding::<E>(|field, codec| {
            if error.is_some() {
                return;
            }
            for row in rows.iter_mut() {
                if let Some(value) = row.get_mut(field.public_name) {
                    match codec.format_result(field, value.clone()) {
                        Ok(formatted) => *value = formatted,
                        Err(reason) => {
                            error = Some(crate::DecodeError {
                                column: field.public_name.to_owned(),
                                reason,
                            });
                            return;
                        }
                    }
                }
            }
        })?;
        if let Some(error) = error {
            return Err(error.into());
        }
        Ok(())
    }

    pub(crate) fn resolve_plan<E: SearchEntity>(
        &self,
        query: impl QueryInput<E>,
        scope: &ServerScope<E>,
        target: QueryTarget,
        operation: &'static str,
    ) -> Result<QueryPlan<E>, PgSearchError> {
        self.resolve_plan_with_scopes(query, scope, &GroupScope::unrestricted(), target, operation)
    }

    pub(crate) fn resolve_plan_with_scopes<E: SearchEntity>(
        &self,
        query: impl QueryInput<E>,
        scope: &ServerScope<E>,
        group_scope: &GroupScope<E>,
        target: QueryTarget,
        operation: &'static str,
    ) -> Result<QueryPlan<E>, PgSearchError> {
        let parse_started = Instant::now();
        let query = query.into_user_query(&self.policy)?;
        let parse_duration = elapsed_micros(parse_started);
        let plan_started = Instant::now();
        let plan = QueryPlan::new_with_scopes(&query, scope, group_scope, target)?;
        tracing::debug!(
            entity = E::META.entity_name,
            operation,
            parse_duration_us = parse_duration,
            plan_duration_us = elapsed_micros(plan_started),
            "planned SiftQuery operation"
        );
        Ok(plan)
    }

    pub(crate) fn render<E: SearchEntity>(
        &self,
        plan: &QueryPlan<E>,
        operation: &'static str,
        phase: &'static str,
        partition: Option<&str>,
        request: RenderRequest<'_, E>,
    ) -> Result<RenderedPgQuery, PgSearchError> {
        let started = Instant::now();
        let (base, cache_hit) = match &self.shape_cache {
            Some(cache) => cache.render(plan, &self.renderer, &request)?,
            None => (request.render(&self.renderer, plan)?, false),
        };
        let output = self
            .extensions
            .apply_rendered(plan, operation, phase, partition, base)?;
        tracing::Span::current().record("query_shape_id", output.shape_id.0);
        tracing::debug!(
            entity = E::META.entity_name,
            render_phase = phase,
            query_shape_id = output.shape_id.0,
            render_duration_us = elapsed_micros(started),
            shape_cache_hit = cache_hit,
            "rendered SiftQuery SQL"
        );
        Ok(output)
    }

    pub(crate) async fn acquire(
        &self,
        deadline: Instant,
    ) -> Result<sqlx::pool::PoolConnection<Postgres>, PgSearchError> {
        let started = Instant::now();
        let connection = timeout_at(deadline, self.pool.acquire())
            .await
            .map_err(|_| PgSearchError::Timeout(TimeoutStage::PoolAcquire))?
            .map_err(PgSearchError::Sql)?;
        tracing::debug!(
            pool_wait_duration_us = elapsed_micros(started),
            "acquired PostgreSQL connection"
        );
        Ok(connection)
    }
}

fn elapsed_micros(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX)
}
