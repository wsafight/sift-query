use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

use sift_query_algorithm::BoundedLruMap;
use sift_query_core::ConfigError;

use crate::{QueryShapeId, RenderedPgQuery};

mod template;
use crate::{PgRenderer, RenderError};
use sift_query_core::{FieldId, QueryPlan, SearchEntity, SearchOptions};
pub(crate) use template::RenderRequest;
use template::{Template, TemplateKey};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum CacheKey {
    Rendered(QueryShapeId),
    Template(Arc<TemplateKey>),
}

#[derive(Debug, Clone)]
enum CacheEntry {
    Sql(Arc<str>),
    Template(Arc<Template>),
}

/// Cumulative observations for a bounded SQL-shape cache.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ShapeCacheStats {
    pub hits: u64,
    pub misses: u64,
    pub insertions: u64,
    pub evictions: u64,
    pub collisions: u64,
    pub entries: usize,
    pub capacity: usize,
}

/// Bounded LRU cache for SQL templates and value-free bind instructions.
///
/// Bind values are deliberately never cached. Template lookup compares the
/// complete structured key, preserving correctness across hash collisions.
/// The legacy `intern` entry point compares the complete SQL text.
#[derive(Debug)]
pub struct QueryShapeCache {
    state: RwLock<CacheState>,
}

#[derive(Debug)]
struct CacheState {
    entries: BoundedLruMap<CacheKey, CacheEntry>,
    hits: AtomicU64,
    misses: u64,
    insertions: u64,
    evictions: u64,
    collisions: u64,
}

impl QueryShapeCache {
    pub fn new(capacity: usize) -> Result<Self, ConfigError> {
        let entries = BoundedLruMap::new(capacity)
            .map_err(|error| ConfigError::Invalid(format!("shape cache {error}")))?;
        Ok(Self {
            state: RwLock::new(CacheState {
                entries,
                hits: AtomicU64::new(0),
                misses: 0,
                insertions: 0,
                evictions: 0,
                collisions: 0,
            }),
        })
    }

    /// Reuses the cached SQL allocation when this exact shape is present.
    #[must_use]
    pub fn intern(&self, mut query: RenderedPgQuery) -> (RenderedPgQuery, bool) {
        let mut state = self
            .state
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let key = CacheKey::Rendered(query.shape_id);
        if let Some(CacheEntry::Sql(sql)) = state.entries.get(&key).cloned() {
            if sql.as_ref() == query.sql.as_ref() {
                query.sql = sql;
                increment_hits(&state.hits);
                return (query, true);
            }
            state.collisions = state.collisions.saturating_add(1);
            state.misses = state.misses.saturating_add(1);
            return (query, false);
        }

        state.misses = state.misses.saturating_add(1);
        if state
            .entries
            .insert(key, CacheEntry::Sql(Arc::clone(&query.sql)))
        {
            state.evictions = state.evictions.saturating_add(1);
        }
        state.insertions = state.insertions.saturating_add(1);
        (query, false)
    }

    /// Renders on a miss; on a hit, only rebuilds this request's bind values.
    pub fn render_list<E: SearchEntity>(
        &self,
        plan: &QueryPlan<E>,
    ) -> Result<(RenderedPgQuery, bool), RenderError> {
        self.render(plan, &PgRenderer, &RenderRequest::List)
    }

    pub fn render_cluster<E: SearchEntity>(
        &self,
        plan: &QueryPlan<E>,
        options: Option<&SearchOptions<E>>,
    ) -> Result<(RenderedPgQuery, bool), RenderError> {
        self.render(plan, &PgRenderer, &RenderRequest::Cluster(options))
    }

    pub fn render_sum<E: SearchEntity>(
        &self,
        plan: &QueryPlan<E>,
        field: FieldId,
    ) -> Result<(RenderedPgQuery, bool), RenderError> {
        self.render(plan, &PgRenderer, &RenderRequest::Sum(field))
    }

    pub fn render_window_list<E: SearchEntity>(
        &self,
        plan: &QueryPlan<E>,
    ) -> Result<(RenderedPgQuery, bool), RenderError> {
        self.render(plan, &PgRenderer, &RenderRequest::Window)
    }

    pub(crate) fn render<E: SearchEntity>(
        &self,
        plan: &QueryPlan<E>,
        renderer: &PgRenderer,
        request: &RenderRequest<'_, E>,
    ) -> Result<(RenderedPgQuery, bool), RenderError> {
        if matches!(request, RenderRequest::Window) && !renderer.supports_window_count(plan) {
            return Err(RenderError::WrongTarget);
        }
        let key = CacheKey::Template(Arc::new(TemplateKey::new(plan, request)?));
        // A hit on the current MRU entry cannot change eviction order. Let
        // repeated readers proceed together, without weakening exact LRU.
        {
            let state = self
                .state
                .read()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(CacheEntry::Template(template)) = state.entries.peek_mru(&key) {
                let template = Arc::clone(template);
                increment_hits(&state.hits);
                drop(state);
                return template.bind(plan).map(|query| (query, true));
            }
        }
        {
            let mut state = self
                .state
                .write()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(CacheEntry::Template(template)) = state.entries.get(&key).cloned() {
                increment_hits(&state.hits);
                drop(state);
                return template.bind(plan).map(|query| (query, true));
            }
            state.misses = state.misses.saturating_add(1);
        }
        // Never hold the cache lock while rendering or rebuilding binds.
        let query = request.render(renderer, plan)?;
        let template = Arc::new(Template::new(plan, request, &query)?);
        let mut state = self
            .state
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.entries.insert(key, CacheEntry::Template(template)) {
            state.evictions = state.evictions.saturating_add(1);
        }
        state.insertions = state.insertions.saturating_add(1);
        Ok((query, false))
    }

    #[must_use]
    pub fn stats(&self) -> ShapeCacheStats {
        let state = self
            .state
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        ShapeCacheStats {
            hits: state.hits.load(Ordering::Relaxed),
            misses: state.misses,
            insertions: state.insertions,
            evictions: state.evictions,
            collisions: state.collisions,
            entries: state.entries.len(),
            capacity: state.entries.capacity(),
        }
    }

    pub fn clear(&self) {
        let mut state = self
            .state
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.entries.clear();
    }
}

fn increment_hits(hits: &AtomicU64) {
    let _ = hits.try_update(Ordering::Relaxed, Ordering::Relaxed, |count| {
        Some(count.saturating_add(1))
    });
}
