//! MySQL and MariaDB execution backend for `SiftQuery`.

use std::time::Duration;

use sift_query_core::{
    ConfigError, ParamError, ParamParser, QueryInput, QueryPolicy, RawParams, RowValue,
    ScopeProvider, SearchEntity,
};
use sift_query_sqlx_common::{Dialect, Extensions, Renderer};
use sqlx::FromRow;
use sqlx::mysql::{MySqlPool, MySqlPoolOptions, MySqlRow};

mod backend;
mod execution;
pub(crate) use execution::decode::decode_map;
mod export;
mod router;
mod scan;
pub mod web {
    pub use sift_query_sqlx_common::web::*;
}

pub use backend::MySqlBackend;
use backend::decode_summary;
pub use export::CsvExporter;
pub use router::MySqlPoolRouter;
pub use sift_query_sqlx_common::{
    BackendCapability, BackendCapabilitySet, BackendKind, PoolRouteError, PoolSelector,
    backend_capabilities,
};
pub use sift_query_sqlx_common::{
    CsvColumn, CsvExportDescriptor, CsvExportPolicy, CursorPage, DictionaryLabelLoader,
    EnrichContext, ExportLimiter, ExtensionError, ExtensionFuture, Label, LabelEnricher,
    LabelFailurePolicy, LabelLoader, LabelTarget, MissingLabelPolicy, Page, PageEnricher,
    QueryObservation, QueryObserver, RowMap, SearchError, SummaryValue, TimeoutStage,
    TypedPageEnricher,
};
pub use sift_query_sqlx_common::{ScanFuture, ScanPolicy, SearchStream};

#[derive(Debug, Clone, Copy)]
pub struct MySqlExecutionPolicy {
    pub timeout: Duration,
}

impl Default for MySqlExecutionPolicy {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(5),
        }
    }
}

#[derive(Debug, Clone)]
pub struct MySqlSiftQuery {
    pub(crate) pool: MySqlPool,
    policy: QueryPolicy,
    execution: MySqlExecutionPolicy,
    renderer: Renderer,
    pub(crate) scan_gate: std::sync::Arc<tokio::sync::Semaphore>,
    pub(crate) scan_wait_gate: std::sync::Arc<tokio::sync::Semaphore>,
    pub(crate) extensions: Extensions,
    pub(crate) observers: sift_query_sqlx_common::Observers,
}

impl MySqlSiftQuery {
    #[must_use]
    pub fn new(pool: MySqlPool) -> Self {
        let (scan_gate, scan_wait_gate) = sift_query_sqlx_common::default_scan_gates();
        Self {
            pool,
            policy: QueryPolicy::default(),
            execution: MySqlExecutionPolicy::default(),
            renderer: Renderer::new(Dialect::MySql),
            scan_gate,
            scan_wait_gate,
            extensions: Extensions::default(),
            observers: sift_query_sqlx_common::Observers::default().with_backend("mysql"),
        }
    }

    pub async fn connect(url: &str) -> Result<Self, sqlx::Error> {
        Ok(Self::new(MySqlPool::connect(url).await?))
    }

    pub async fn connect_with(options: MySqlPoolOptions, url: &str) -> Result<Self, sqlx::Error> {
        Ok(Self::new(options.connect(url).await?))
    }

    #[must_use]
    pub fn with_policy(mut self, policy: QueryPolicy, execution: MySqlExecutionPolicy) -> Self {
        self.policy = policy;
        self.execution = execution;
        self
    }

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
        self.scan_gate = std::sync::Arc::new(tokio::sync::Semaphore::new(active));
        self.scan_wait_gate = std::sync::Arc::new(tokio::sync::Semaphore::new(waiting));
        Ok(self)
    }

    #[must_use]
    pub fn with_page_enricher<E>(mut self, enricher: E) -> Self
    where
        E: sift_query_sqlx_common::PageEnricher + 'static,
    {
        self.extensions.push_page_enricher(enricher);
        self
    }

    #[must_use]
    pub fn with_query_observer<O: QueryObserver + 'static>(mut self, observer: O) -> Self {
        self.observers = self.observers.with_query_observer(observer);
        self
    }

    #[must_use]
    pub fn with_slow_query_threshold(mut self, threshold: Duration) -> Self {
        self.observers = self.observers.with_slow_query_threshold(threshold);
        self
    }

    #[must_use]
    pub fn with_entity_slow_query_threshold<E: SearchEntity>(
        mut self,
        threshold: Duration,
    ) -> Self {
        self.observers = self
            .observers
            .with_entity_slow_query_threshold::<E>(threshold);
        self
    }

    pub fn parse<E: SearchEntity>(
        &self,
        raw: &RawParams,
    ) -> Result<sift_query_core::UserQuery<E>, ParamError> {
        ParamParser::new(&self.policy)?.parse(raw)
    }

    /// Executes a query after deriving its mandatory server scope from a
    /// trusted application context.
    pub async fn search_authorized<E, C, P>(
        &self,
        query: impl QueryInput<E>,
        context: &C,
        provider: &P,
    ) -> Result<Page<E>, SearchError>
    where
        E: SearchEntity + Send + Unpin + for<'r> FromRow<'r, MySqlRow>,
        P: ScopeProvider<C, E>,
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
    ) -> Result<Page<indexmap::IndexMap<String, RowValue>>, SearchError>
    where
        E: SearchEntity,
        P: ScopeProvider<C, E>,
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
    ) -> Result<i64, SearchError>
    where
        E: SearchEntity,
        P: ScopeProvider<C, E>,
    {
        let scope = provider.scope(context)?;
        self.count(query, &scope).await
    }

    pub(crate) fn format_map_rows<E: SearchEntity>(
        &self,
        rows: &mut [RowMap],
    ) -> Result<(), SearchError> {
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
                            error = Some(SearchError::Decode {
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
            return Err(error);
        }
        Ok(())
    }
}
