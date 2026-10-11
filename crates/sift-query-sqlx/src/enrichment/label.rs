//! PostgreSQL label enrichment.
//!
//! This module deliberately keeps PostgreSQL's `ExtensionError`, `Page`, and
//! extension registry contract. The portable implementation in
//! `sift-query-sqlx-common` serves the MySQL/SQLite backends, whose errors and
//! page types are separate public contracts.

use crate::{
    EnrichContext, ExtensionError, ExtensionFuture, Page, PageEnricher, RowMap, TypedPageEnricher,
};
use sift_query_algorithm::TtlLruMap;
use sift_query_core::RowValue;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;
use tokio::time::timeout;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Label {
    pub id: String,
    pub label: String,
}
pub trait LabelLoader: Send + Sync {
    fn supports(&self, key: &str) -> bool;
    fn load(&self, key: &str, ids: Vec<String>) -> ExtensionFuture<Vec<Label>>;
}

#[derive(Debug, Clone, Copy, Default)]
pub enum MissingLabelPolicy {
    #[default]
    Omit,
    Null,
    Error,
}
#[derive(Debug, Clone, Copy, Default)]
pub enum LabelFailurePolicy {
    #[default]
    Error,
    KeepRows,
}

/// Typed opt-in: implementations explicitly name the source fields and writable
/// label fields. `set_label` must not modify database fields or ordering keys.
pub trait LabelTarget: Send + 'static {
    fn label_id(&self, source: &str) -> Option<String>;
    fn set_label(&mut self, source: &str, label: Option<&str>) -> Result<(), ExtensionError>;
}

type LabelKey = (String, String, String);
type Cache = Arc<LabelCache>;

struct LabelCache {
    origin: Instant,
    entries: Mutex<TtlLruMap<LabelKey, String>>,
}

#[derive(Clone)]
pub struct LabelEnricher {
    bindings: Vec<(String, String)>,
    loaders: Vec<Arc<dyn LabelLoader>>,
    max_ids: usize,
    timeout: Duration,
    gate: Arc<Semaphore>,
    missing: MissingLabelPolicy,
    failure: LabelFailurePolicy,
    cache: Option<Cache>,
}
impl std::fmt::Debug for LabelEnricher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LabelEnricher")
            .field("bindings", &self.bindings)
            .field("max_ids", &self.max_ids)
            .field("timeout", &self.timeout)
            .field("cached", &self.cache.is_some())
            .finish_non_exhaustive()
    }
}
impl Default for LabelEnricher {
    fn default() -> Self {
        Self::new()
    }
}
impl LabelEnricher {
    #[must_use]
    pub fn new() -> Self {
        Self {
            bindings: Vec::new(),
            loaders: Vec::new(),
            max_ids: 512,
            timeout: Duration::from_secs(2),
            gate: Arc::new(Semaphore::new(4)),
            missing: MissingLabelPolicy::Omit,
            failure: LabelFailurePolicy::Error,
            cache: None,
        }
    }
    #[must_use]
    pub fn bind(mut self, source: impl Into<String>, key: impl Into<String>) -> Self {
        self.bindings.push((source.into(), key.into()));
        self
    }

    #[must_use]
    pub fn from_entity<E: sift_query_core::SearchEntity>(self) -> Self {
        E::label_bindings()
            .iter()
            .fold(self, |enricher, (source, key)| enricher.bind(*source, *key))
    }
    #[must_use]
    pub fn with_loader<L: LabelLoader + 'static>(mut self, loader: L) -> Self {
        self.loaders.push(Arc::new(loader));
        self
    }
    #[must_use]
    pub fn with_budget(mut self, max_ids: usize, timeout: Duration) -> Self {
        self.max_ids = max_ids;
        self.timeout = timeout;
        self
    }
    #[must_use]
    pub fn with_missing_policy(mut self, policy: MissingLabelPolicy) -> Self {
        self.missing = policy;
        self
    }
    #[must_use]
    pub fn with_failure_policy(mut self, policy: LabelFailurePolicy) -> Self {
        self.failure = policy;
        self
    }
    pub fn with_concurrency(
        mut self,
        maximum: usize,
    ) -> Result<Self, sift_query_core::ConfigError> {
        if maximum == 0 || maximum > 1024 {
            return Err(sift_query_core::ConfigError::Invalid(
                "invalid label concurrency".to_owned(),
            ));
        }
        self.gate = Arc::new(Semaphore::new(maximum));
        Ok(self)
    }
    /// Optional positive-result cache. Cache ownership is this enricher and its
    /// clones, never global. Use distinct instances for different tenant/locale
    /// dictionaries; remote request-dependent loaders should leave it disabled.
    pub fn with_cache(
        mut self,
        capacity: usize,
        ttl: Duration,
    ) -> Result<Self, sift_query_core::ConfigError> {
        if capacity == 0 || capacity > 16_384 || ttl.is_zero() {
            return Err(sift_query_core::ConfigError::Invalid(
                "invalid label cache budget".to_owned(),
            ));
        }
        self.cache = Some(Arc::new(LabelCache {
            origin: Instant::now(),
            entries: Mutex::new(TtlLruMap::new(capacity, ttl).map_err(|_| {
                sift_query_core::ConfigError::Invalid("invalid label cache budget".to_owned())
            })?),
        }));
        Ok(self)
    }

    async fn apply(
        &self,
        mut page: Page<RowMap>,
        context: EnrichContext,
    ) -> Result<Page<RowMap>, ExtensionError> {
        if self.timeout.is_zero() || self.max_ids == 0 {
            return Err(ExtensionError::rejected("label", "invalid label budget"));
        }
        if self.bindings.len() > 32 {
            return Err(ExtensionError::rejected("label", "too many label bindings"));
        }
        if page.data.is_empty() {
            return Ok(page);
        }
        let mut groups = BTreeMap::<String, BTreeSet<String>>::new();
        let mut seen_sources = BTreeSet::new();
        for (source, key) in &self.bindings {
            if !seen_sources.insert(source) {
                return Err(ExtensionError::rejected(
                    "label",
                    "duplicate source binding",
                ));
            }
            groups.entry(key.clone()).or_default();
            for row in &page.data {
                if let Some(id) = row.get(source).and_then(row_key) {
                    groups.entry(key.clone()).or_default().insert(id);
                }
            }
        }
        if groups.values().map(BTreeSet::len).sum::<usize>() > self.max_ids {
            return Err(ExtensionError::LabelIdLimit(self.max_ids));
        }
        for (key, ids) in groups {
            let loaders = self
                .loaders
                .iter()
                .filter(|loader| loader.supports(&key))
                .collect::<Vec<_>>();
            if loaders.len() != 1 {
                return Err(ExtensionError::rejected(
                    "label",
                    "missing or ambiguous loader",
                ));
            }
            let mut labels = HashMap::new();
            let mut misses = Vec::new();
            for id in &ids {
                let cached = self.cache.as_ref().and_then(|cache| {
                    cache
                        .entries
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .get(
                            &(context.entity_name.to_owned(), key.clone(), id.clone()),
                            cache.origin.elapsed(),
                        )
                        .cloned()
                });
                if let Some(label) = cached {
                    labels.insert(id.clone(), label);
                } else {
                    misses.push(id.clone());
                }
            }
            if !misses.is_empty() {
                let loaded = timeout(self.timeout, async {
                    let _permit = self
                        .gate
                        .acquire()
                        .await
                        .map_err(|_| ExtensionError::rejected("label", "loader gate closed"))?;
                    loaders[0].load(&key, misses.clone()).await
                })
                .await
                .map_err(|_| ExtensionError::LabelTimeout)
                .and_then(std::convert::identity);
                let loaded = match loaded {
                    Ok(loaded) => loaded,
                    Err(_) if matches!(self.failure, LabelFailurePolicy::KeepRows) => {
                        tracing::warn!(
                            outcome = "label_unavailable",
                            "label loading failed; keeping rows"
                        );
                        continue;
                    }
                    Err(error) => return Err(error),
                };
                let mut fresh = HashMap::new();
                for label in loaded {
                    if !misses.contains(&label.id) || fresh.insert(label.id, label.label).is_some()
                    {
                        return Err(ExtensionError::rejected(
                            "label",
                            "loader returned duplicate or unrequested IDs",
                        ));
                    }
                }
                // Cache only after the complete batch is validated. Failed or
                // malicious responses cannot poison the shared cache.
                if let Some(cache) = &self.cache {
                    let now = cache.origin.elapsed();
                    let mut cache = cache
                        .entries
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    for (id, label) in &fresh {
                        cache.insert(
                            (context.entity_name.to_owned(), key.clone(), id.clone()),
                            label.clone(),
                            now,
                        );
                    }
                }
                labels.extend(fresh);
            }
            for (source, binding_key) in &self.bindings {
                if binding_key != &key {
                    continue;
                }
                let target = format!("{source}Label");
                for row in &mut page.data {
                    if row.contains_key(&target) {
                        return Err(ExtensionError::rejected(
                            "label",
                            "label target already exists",
                        ));
                    }
                    match row
                        .get(source)
                        .and_then(row_key)
                        .and_then(|id| labels.get(&id))
                    {
                        Some(label) => {
                            row.insert(target.clone(), RowValue::Text(label.clone()));
                        }
                        None => match self.missing {
                            MissingLabelPolicy::Omit => {}
                            MissingLabelPolicy::Null => {
                                row.insert(target.clone(), RowValue::Null);
                            }
                            MissingLabelPolicy::Error => {
                                return Err(ExtensionError::rejected("label", "missing label"));
                            }
                        },
                    }
                }
            }
        }
        Ok(page)
    }
}

impl PageEnricher for LabelEnricher {
    fn enrich(&self, page: Page<RowMap>, context: EnrichContext) -> ExtensionFuture<Page<RowMap>> {
        let enricher = self.clone();
        Box::pin(async move { enricher.apply(page, context).await })
    }
}
impl<T: LabelTarget> TypedPageEnricher<T> for LabelEnricher {
    fn enrich(&self, mut rows: Vec<T>, context: EnrichContext) -> ExtensionFuture<Vec<T>> {
        let enricher = self.clone();
        Box::pin(async move {
            let maps = rows
                .iter()
                .map(|row| {
                    enricher
                        .bindings
                        .iter()
                        .filter_map(|(source, _)| {
                            row.label_id(source)
                                .map(|id| (source.clone(), RowValue::Text(id)))
                        })
                        .collect()
                })
                .collect();
            let page = enricher
                .apply(
                    Page {
                        data: maps,
                        total_count: 0,
                        summaries: Vec::new(),
                        next_cursor: None,
                    },
                    context,
                )
                .await?;
            for (row, map) in rows.iter_mut().zip(&page.data) {
                for (source, _) in &enricher.bindings {
                    if let Some(value) = map.get(&format!("{source}Label")) {
                        row.set_label(
                            source,
                            match value {
                                RowValue::Text(label) => Some(label),
                                _ => None,
                            },
                        )?;
                    }
                }
            }
            Ok(rows)
        })
    }
}

/// Enum/local dictionaries share the same one-batch loader contract as remote services.
#[derive(Debug, Clone, Default)]
pub struct DictionaryLabelLoader {
    dictionaries: HashMap<String, HashMap<String, String>>,
}
impl DictionaryLabelLoader {
    #[must_use]
    pub fn with_labels(
        mut self,
        key: impl Into<String>,
        labels: impl IntoIterator<Item = (String, String)>,
    ) -> Self {
        self.dictionaries
            .insert(key.into(), labels.into_iter().collect());
        self
    }
}
impl LabelLoader for DictionaryLabelLoader {
    fn supports(&self, key: &str) -> bool {
        self.dictionaries.contains_key(key)
    }
    fn load(&self, key: &str, ids: Vec<String>) -> ExtensionFuture<Vec<Label>> {
        let result = self
            .dictionaries
            .get(key)
            .map(|dict| {
                ids.into_iter()
                    .filter_map(|id| {
                        dict.get(&id).map(|label| Label {
                            id: id.clone(),
                            label: label.clone(),
                        })
                    })
                    .collect()
            })
            .ok_or_else(|| ExtensionError::rejected("label", "missing dictionary"));
        Box::pin(async move { result })
    }
}
fn row_key(value: &RowValue) -> Option<String> {
    match value {
        RowValue::Text(value) => Some(value.clone()),
        RowValue::I32(value) => Some(value.to_string()),
        RowValue::I64(value) => Some(value.to_string()),
        #[cfg(feature = "uuid")]
        RowValue::Uuid(value) => Some(value.to_string()),
        _ => None,
    }
}
