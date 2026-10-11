use std::fmt;
pub(crate) mod label;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use sift_query_core::{EntityMeta, Paging, PredicateArena, QueryPlan, QueryTarget, SearchEntity};
use thiserror::Error;
use tokio::time::timeout;

use crate::renderer::hash_shape;
use crate::{Page, QueryShapeId, RenderedPgQuery, RowMap};

pub type ExtensionFuture<T> = Pin<Box<dyn Future<Output = Result<T, ExtensionError>> + Send>>;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum ExtensionError {
    #[error("invalid PostgreSQL {kind} identifier: {value}")]
    InvalidIdentifier { kind: &'static str, value: String },
    #[error("{extension} rejected the query: {message}")]
    Rejected {
        extension: &'static str,
        message: String,
    },
    #[error("partition source could not be applied to rendered SQL")]
    PartitionSourceNotFound,
    #[error("label loader timed out")]
    LabelTimeout,
    #[error("label loader exceeded its id budget of {0}")]
    LabelIdLimit(usize),
}

impl ExtensionError {
    #[must_use]
    pub fn rejected(extension: &'static str, message: impl Into<String>) -> Self {
        Self::Rejected {
            extension,
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartitionName {
    schema: Option<String>,
    table: String,
    alias: Option<String>,
}

impl PartitionName {
    pub fn new(table: impl Into<String>) -> Result<Self, ExtensionError> {
        let table = table.into();
        validate_identifier("table", &table)?;
        Ok(Self {
            schema: None,
            table,
            alias: None,
        })
    }

    pub fn qualified(
        schema: impl Into<String>,
        table: impl Into<String>,
    ) -> Result<Self, ExtensionError> {
        let schema = schema.into();
        let table = table.into();
        validate_identifier("schema", &schema)?;
        validate_identifier("table", &table)?;
        Ok(Self {
            schema: Some(schema),
            table,
            alias: None,
        })
    }

    pub fn with_alias(mut self, alias: impl Into<String>) -> Result<Self, ExtensionError> {
        let alias = alias.into();
        validate_identifier("alias", &alias)?;
        self.alias = Some(alias);
        Ok(self)
    }

    fn render(&self) -> String {
        let mut output = self.schema.as_ref().map_or_else(String::new, |schema| {
            format!("{}.", quote_identifier(schema))
        });
        output.push_str(&quote_identifier(&self.table));
        if let Some(alias) = &self.alias {
            output.push_str(" AS ");
            output.push_str(&quote_identifier(alias));
        }
        output
    }
}

pub struct QueryContext<'a> {
    pub entity: &'static EntityMeta,
    pub target: QueryTarget,
    pub operation: &'static str,
    pub server_predicate: &'a PredicateArena,
    pub server_group_predicate: &'a PredicateArena,
    pub row_predicate: &'a PredicateArena,
    pub group_predicate: &'a PredicateArena,
    pub paging: &'a Paging,
}

impl fmt::Debug for QueryContext<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("QueryContext")
            .field("entity", &self.entity.entity_name)
            .field("target", &self.target)
            .field("operation", &self.operation)
            .field("page_size", &self.paging.size)
            .field(
                "paging_mode",
                &match self.paging.mode {
                    sift_query_core::PagingMode::Page { .. } => "page",
                    sift_query_core::PagingMode::Offset => "offset",
                    sift_query_core::PagingMode::Cursor(_) => "cursor",
                },
            )
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SqlContext {
    pub entity_name: &'static str,
    pub target: QueryTarget,
    pub operation: &'static str,
    pub phase: &'static str,
    pub shape_id: QueryShapeId,
}

#[derive(Debug, Clone, Copy)]
pub struct ResultContext {
    pub entity_name: &'static str,
    pub operation: &'static str,
    pub target: QueryTarget,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResultFilterDecision {
    Keep,
    Discard,
}

pub trait PartitionResolver: Send + Sync {
    fn resolve(&self, context: &QueryContext<'_>) -> Result<Option<PartitionName>, ExtensionError>;
}

pub trait SqlInterceptor: Send + Sync {
    /// Returns replacement SQL, or `None` to keep the current SQL text.
    /// Bind values are intentionally not exposed to interceptors.
    fn intercept(&self, context: &SqlContext, sql: &str) -> Result<Option<String>, ExtensionError>;
}

pub trait ResultFilter: Send + Sync {
    /// Mutates or discards a decoded dynamic row. Database `total_count` is not
    /// changed when a row is discarded.
    fn filter(
        &self,
        context: &ResultContext,
        row_index: usize,
        row: &mut RowMap,
    ) -> Result<ResultFilterDecision, ExtensionError>;
}

/// Context passed to post-query, whole-page enrichers.
#[derive(Debug, Clone, Copy)]
pub struct EnrichContext {
    pub entity_name: &'static str,
    pub operation: &'static str,
}

/// Asynchronous page-level result enhancement. Implementations should batch
/// external lookups and must not change count, summaries, or cursor metadata.
pub trait PageEnricher: Send + Sync {
    fn enrich(&self, page: Page<RowMap>, context: EnrichContext) -> ExtensionFuture<Page<RowMap>>;
}

/// Explicit typed opt-in: implementers declare which presentation fields may
/// be filled and preserve database fields and ordering keys.
pub trait TypedPageEnricher<T>: Send + Sync {
    fn enrich(&self, rows: Vec<T>, context: EnrichContext) -> ExtensionFuture<Vec<T>>;
}

#[derive(Clone, Default)]
pub(crate) struct PgExtensions {
    partition_resolver: Option<Arc<dyn PartitionResolver>>,
    sql_interceptors: Vec<Arc<dyn SqlInterceptor>>,
    result_filters: Vec<Arc<dyn ResultFilter>>,
    page_enrichers: Vec<Arc<dyn PageEnricher>>,
}

impl fmt::Debug for PgExtensions {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PgExtensions")
            .field("has_partition_resolver", &self.partition_resolver.is_some())
            .field("sql_interceptors", &self.sql_interceptors.len())
            .field("result_filters", &self.result_filters.len())
            .field("page_enrichers", &self.page_enrichers.len())
            .finish()
    }
}

impl PgExtensions {
    pub(crate) fn set_partition_resolver(&mut self, resolver: Arc<dyn PartitionResolver>) {
        self.partition_resolver = Some(resolver);
    }

    pub(crate) fn push_sql_interceptor(&mut self, interceptor: Arc<dyn SqlInterceptor>) {
        self.sql_interceptors.push(interceptor);
    }

    pub(crate) fn push_result_filter(&mut self, filter: Arc<dyn ResultFilter>) {
        self.result_filters.push(filter);
    }

    pub(crate) fn push_page_enricher(&mut self, enricher: Arc<dyn PageEnricher>) {
        self.page_enrichers.push(enricher);
    }

    pub(crate) async fn enrich_page(
        &self,
        entity_name: &'static str,
        operation: &'static str,
        mut page: Page<RowMap>,
    ) -> Result<Page<RowMap>, ExtensionError> {
        let context = EnrichContext {
            entity_name,
            operation,
        };
        for enricher in &self.page_enrichers {
            let count = page.total_count;
            let summaries = page.summaries.clone();
            let cursor = page.next_cursor.clone();
            let rows = page.data.len();
            page = timeout(Duration::from_secs(5), enricher.enrich(page, context))
                .await
                .map_err(|_| ExtensionError::LabelTimeout)??;
            if page.total_count != count
                || page.summaries != summaries
                || page.next_cursor != cursor
                || page.data.len() != rows
            {
                return Err(ExtensionError::rejected(
                    "page_enricher",
                    "enrichment must preserve row count, count, summaries and cursor",
                ));
            }
        }
        Ok(page)
    }

    pub(crate) fn resolve_partition<E: SearchEntity>(
        &self,
        plan: &QueryPlan<E>,
        operation: &'static str,
    ) -> Result<Option<String>, ExtensionError> {
        let context = QueryContext {
            entity: plan.meta,
            target: plan.target,
            operation,
            server_predicate: &plan.server_predicate,
            server_group_predicate: &plan.server_group_predicate,
            row_predicate: &plan.row_predicate,
            group_predicate: &plan.group_predicate,
            paging: &plan.paging,
        };
        self.partition_resolver
            .as_ref()
            .map(|resolver| resolver.resolve(&context))
            .transpose()
            .map(|partition| partition.flatten().map(|partition| partition.render()))
    }

    pub(crate) fn apply_rendered<E: SearchEntity>(
        &self,
        plan: &QueryPlan<E>,
        operation: &'static str,
        phase: &'static str,
        partition: Option<&str>,
        mut query: RenderedPgQuery,
    ) -> Result<RenderedPgQuery, ExtensionError> {
        if partition.is_none() && self.sql_interceptors.is_empty() {
            return Ok(query);
        }
        let mut sql = query.sql.to_string();
        if let Some(partition) = partition {
            let source = format!(" FROM {}", plan.meta.from);
            if !sql.contains(&source) {
                return Err(ExtensionError::PartitionSourceNotFound);
            }
            sql = sql.replace(&source, &format!(" FROM {partition}"));
        }
        for interceptor in &self.sql_interceptors {
            let context = SqlContext {
                entity_name: plan.meta.entity_name,
                target: plan.target,
                operation,
                phase,
                shape_id: query.shape_id,
            };
            if let Some(replacement) = interceptor.intercept(&context, &sql)? {
                sql = replacement;
            }
        }
        if sql.as_str() != query.sql.as_ref() {
            query.shape_id = QueryShapeId(hash_shape(query.shape_id.0, &sql));
            query.sql = sql.into();
        }
        Ok(query)
    }

    pub(crate) fn filter_rows<E: SearchEntity>(
        &self,
        operation: &'static str,
        rows: &mut Vec<RowMap>,
    ) -> Result<(), ExtensionError> {
        if self.result_filters.is_empty() {
            return Ok(());
        }
        let context = ResultContext {
            entity_name: E::META.entity_name,
            operation,
            target: QueryTarget::MapPage,
        };
        let mut retained = Vec::with_capacity(rows.len());
        for (index, mut row) in rows.drain(..).enumerate() {
            let mut decision = ResultFilterDecision::Keep;
            for filter in &self.result_filters {
                if filter.filter(&context, index, &mut row)? == ResultFilterDecision::Discard {
                    decision = ResultFilterDecision::Discard;
                    break;
                }
            }
            if decision == ResultFilterDecision::Keep {
                retained.push(row);
            }
        }
        *rows = retained;
        Ok(())
    }
}

fn validate_identifier(kind: &'static str, value: &str) -> Result<(), ExtensionError> {
    let mut bytes = value.bytes();
    let valid = bytes
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == b'_')
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'$'));
    if valid {
        Ok(())
    } else {
        Err(ExtensionError::InvalidIdentifier {
            kind,
            value: value.to_owned(),
        })
    }
}

fn quote_identifier(value: &str) -> String {
    format!("\"{value}\"")
}
#[cfg(test)]
mod tests;
