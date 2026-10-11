use super::*;

impl PgSiftQuery {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            policy: QueryPolicy::default(),
            execution: PgExecutionPolicy::default(),
            renderer: PgRenderer,
            parallel_gate: Arc::new(Semaphore::new(16)),
            scan_gate: Arc::new(Semaphore::new(4)),
            scan_wait_gate: Arc::new(Semaphore::new(16)),
            shape_cache: None,
            extensions: crate::enrichment::PgExtensions::default(),
            observers: crate::observe::Observers::default(),
        }
    }

    pub fn with_policy(
        pool: PgPool,
        policy: QueryPolicy,
        execution: PgExecutionPolicy,
    ) -> Result<Self, ConfigError> {
        if execution.timeout.is_zero() || execution.max_parallel_queries == 0 {
            return Err(ConfigError::Invalid(
                "PostgreSQL timeout and parallel query limit must be greater than zero".to_owned(),
            ));
        }
        if execution.shape_cache_capacity > 16_384 {
            return Err(ConfigError::Invalid(
                "PostgreSQL shape cache capacity cannot exceed 16384".to_owned(),
            ));
        }
        let parallel_gate = Arc::new(Semaphore::new(execution.max_parallel_queries));
        let shape_cache = if execution.shape_cache_capacity > 0 {
            Some(Arc::new(crate::QueryShapeCache::new(
                execution.shape_cache_capacity,
            )?))
        } else {
            None
        };
        Ok(Self {
            pool,
            policy,
            execution,
            renderer: PgRenderer,
            parallel_gate,
            scan_gate: Arc::new(Semaphore::new(4)),
            scan_wait_gate: Arc::new(Semaphore::new(16)),
            shape_cache,
            extensions: crate::enrichment::PgExtensions::default(),
            observers: crate::observe::Observers::default(),
        })
    }

    #[must_use]
    pub fn shape_cache_stats(&self) -> Option<ShapeCacheStats> {
        self.shape_cache.as_ref().map(|cache| cache.stats())
    }

    pub fn clear_shape_cache(&self) {
        if let Some(cache) = &self.shape_cache {
            cache.clear();
        }
    }

    /// Configures a separate active/waiting scan budget shared by instance clones.
    pub fn with_scan_concurrency(
        mut self,
        active: usize,
        waiting: usize,
    ) -> Result<Self, ConfigError> {
        if active == 0 || active > 1024 || waiting > 16_384 {
            return Err(ConfigError::Invalid(
                "invalid scan concurrency budget".to_owned(),
            ));
        }
        self.scan_gate = Arc::new(Semaphore::new(active));
        self.scan_wait_gate = Arc::new(Semaphore::new(waiting));
        Ok(self)
    }

    #[must_use]
    pub fn with_partition_resolver<R>(mut self, resolver: R) -> Self
    where
        R: PartitionResolver + 'static,
    {
        self.extensions.set_partition_resolver(Arc::new(resolver));
        self
    }

    #[must_use]
    pub fn with_sql_interceptor<I>(mut self, interceptor: I) -> Self
    where
        I: SqlInterceptor + 'static,
    {
        self.extensions.push_sql_interceptor(Arc::new(interceptor));
        self
    }

    #[must_use]
    pub fn with_result_filter<F>(mut self, filter: F) -> Self
    where
        F: ResultFilter + 'static,
    {
        self.extensions.push_result_filter(Arc::new(filter));
        self
    }

    #[must_use]
    pub fn with_page_enricher<E>(mut self, enricher: E) -> Self
    where
        E: crate::PageEnricher + 'static,
    {
        self.extensions.push_page_enricher(Arc::new(enricher));
        self
    }

    #[must_use]
    pub fn query_builder<E: SearchEntity>(&self) -> QueryBuilder<'_, E> {
        QueryBuilder::new(&self.policy)
    }
}
