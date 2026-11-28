use crate::{
    EnrichContext, ExtensionError, ExtensionFuture, Page, PageEnricher, RowMap, TypedPageEnricher,
};
use futures_util::future::try_join_all;
use sift_query_algorithm::TtlLruMap;
use sift_query_core::RowValue;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
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

    /// Builds bindings declared on `SearchEntity` fields. This keeps label
    /// configuration beside the field metadata while preserving the same
    /// runtime budgets and loader registry as manual bindings.
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

    fn validate_config(&self) -> Result<(), ExtensionError> {
        if self.timeout.is_zero() || self.max_ids == 0 {
            return Err(ExtensionError::rejected("label", "invalid label budget"));
        }
        if self.bindings.len() > 32 {
            return Err(ExtensionError::rejected("label", "too many label bindings"));
        }
        let mut seen_sources = HashSet::with_capacity(self.bindings.len());
        for (source, _) in &self.bindings {
            if !seen_sources.insert(source.as_str()) {
                return Err(ExtensionError::rejected(
                    "label",
                    "duplicate source binding",
                ));
            }
        }
        Ok(())
    }

    async fn resolve_groups(
        &self,
        groups: BTreeMap<String, BTreeSet<String>>,
        context: EnrichContext,
    ) -> Result<HashMap<String, Option<HashMap<String, String>>>, ExtensionError> {
        if groups.values().map(BTreeSet::len).sum::<usize>() > self.max_ids {
            return Err(ExtensionError::LabelIdLimit(self.max_ids));
        }
        let mut jobs = Vec::with_capacity(groups.len());
        for (key, ids) in groups {
            let mut matching = self.loaders.iter().filter(|loader| loader.supports(&key));
            let Some(loader) = matching.next().cloned() else {
                return Err(ExtensionError::rejected(
                    "label",
                    "missing or ambiguous loader",
                ));
            };
            if matching.next().is_some() {
                return Err(ExtensionError::rejected(
                    "label",
                    "missing or ambiguous loader",
                ));
            }
            jobs.push((key, ids, loader));
        }
        let results = try_join_all(
            jobs.into_iter()
                .map(|(key, ids, loader)| self.resolve_group(key, ids, loader, context)),
        )
        .await?;
        Ok(results.into_iter().collect())
    }

    async fn resolve_group(
        &self,
        key: String,
        ids: BTreeSet<String>,
        loader: Arc<dyn LabelLoader>,
        context: EnrichContext,
    ) -> Result<(String, Option<HashMap<String, String>>), ExtensionError> {
        let mut labels = HashMap::with_capacity(ids.len());
        let mut misses = Vec::new();
        if let Some(cache) = &self.cache {
            let now = cache.origin.elapsed();
            let mut entries = cache
                .entries
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let mut lookup = (context.entity_name.to_owned(), key.clone(), String::new());
            for id in &ids {
                lookup.2.clear();
                lookup.2.push_str(id);
                if let Some(label) = entries.get(&lookup, now) {
                    labels.insert(id.clone(), label.clone());
                } else {
                    misses.push(id.clone());
                }
            }
        } else {
            misses.extend(ids);
        }
        if !misses.is_empty() {
            let requested: HashSet<String> = misses.iter().cloned().collect();
            let fetched = timeout(self.timeout, async {
                let _permit = self
                    .gate
                    .acquire()
                    .await
                    .map_err(|_| ExtensionError::rejected("label", "loader gate closed"))?;
                loader.load(&key, misses).await
            })
            .await
            .map_err(|_| ExtensionError::LabelTimeout)
            .and_then(std::convert::identity);
            let fetched = match fetched {
                Ok(items) => items,
                Err(_) if matches!(self.failure, LabelFailurePolicy::KeepRows) => {
                    tracing::warn!(
                        outcome = "label_unavailable",
                        "label loading failed; keeping rows"
                    );
                    return Ok((key, None));
                }
                Err(error) => return Err(error),
            };
            let mut fresh = HashMap::with_capacity(fetched.len());
            for label in fetched {
                if !requested.contains(&label.id) || fresh.insert(label.id, label.label).is_some() {
                    return Err(ExtensionError::rejected(
                        "label",
                        "loader returned duplicate or unrequested IDs",
                    ));
                }
            }
            if let Some(cache) = &self.cache {
                let now = cache.origin.elapsed();
                let mut entries = cache
                    .entries
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                for (id, label) in &fresh {
                    entries.insert(
                        (context.entity_name.to_owned(), key.clone(), id.clone()),
                        label.clone(),
                        now,
                    );
                }
            }
            labels.extend(fresh);
        }
        Ok((key, Some(labels)))
    }

    async fn apply(
        &self,
        mut page: Page<RowMap>,
        context: EnrichContext,
    ) -> Result<Page<RowMap>, ExtensionError> {
        self.validate_config()?;
        if page.data.is_empty() {
            return Ok(page);
        }
        let mut groups = BTreeMap::<String, BTreeSet<String>>::new();
        for (source, key) in &self.bindings {
            let ids = groups.entry(key.clone()).or_default();
            for row in &page.data {
                if let Some(id) = row.get(source).and_then(row_key) {
                    ids.insert(id);
                }
            }
        }
        let resolved = self.resolve_groups(groups, context).await?;
        for (source, key) in &self.bindings {
            let Some(Some(labels)) = resolved.get(key) else {
                continue;
            };
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
        Ok(page)
    }

    async fn apply_typed<T: LabelTarget>(
        &self,
        mut rows: Vec<T>,
        context: EnrichContext,
    ) -> Result<Vec<T>, ExtensionError> {
        self.validate_config()?;
        if rows.is_empty() {
            return Ok(rows);
        }
        let mut groups = BTreeMap::<String, BTreeSet<String>>::new();
        for (source, key) in &self.bindings {
            let ids = groups.entry(key.clone()).or_default();
            for row in &rows {
                if let Some(id) = row.label_id(source) {
                    ids.insert(id);
                }
            }
        }
        let resolved = self.resolve_groups(groups, context).await?;
        for (source, key) in &self.bindings {
            let Some(Some(labels)) = resolved.get(key) else {
                continue;
            };
            for row in &mut rows {
                match row.label_id(source).and_then(|id| labels.get(&id)) {
                    Some(label) => row.set_label(source, Some(label))?,
                    None => match self.missing {
                        MissingLabelPolicy::Omit => {}
                        MissingLabelPolicy::Null => row.set_label(source, None)?,
                        MissingLabelPolicy::Error => {
                            return Err(ExtensionError::rejected("label", "missing label"));
                        }
                    },
                }
            }
        }
        Ok(rows)
    }
}
impl PageEnricher for LabelEnricher {
    fn enrich(&self, page: Page<RowMap>, context: EnrichContext) -> ExtensionFuture<Page<RowMap>> {
        let enricher = self.clone();
        Box::pin(async move { enricher.apply(page, context).await })
    }
}
impl<T: LabelTarget> TypedPageEnricher<T> for LabelEnricher {
    fn enrich(&self, rows: Vec<T>, context: EnrichContext) -> ExtensionFuture<Vec<T>> {
        let enricher = self.clone();
        Box::pin(async move { enricher.apply_typed(rows, context).await })
    }
}
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
