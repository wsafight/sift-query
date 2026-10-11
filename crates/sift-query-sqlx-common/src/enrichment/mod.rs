use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use crate::{Page, RowMap};
use thiserror::Error;

mod label;
pub use label::{
    DictionaryLabelLoader, Label, LabelEnricher, LabelFailurePolicy, LabelLoader, LabelTarget,
    MissingLabelPolicy,
};

pub type ExtensionFuture<T> = Pin<Box<dyn Future<Output = Result<T, ExtensionError>> + Send>>;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum ExtensionError {
    #[error("{extension} rejected the query: {message}")]
    Rejected {
        extension: &'static str,
        message: String,
    },
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

#[derive(Debug, Clone, Copy)]
pub struct EnrichContext {
    pub entity_name: &'static str,
    pub operation: &'static str,
}

pub trait PageEnricher: Send + Sync {
    fn enrich(&self, page: Page<RowMap>, context: EnrichContext) -> ExtensionFuture<Page<RowMap>>;
}

pub trait TypedPageEnricher<T>: Send + Sync {
    fn enrich(&self, rows: Vec<T>, context: EnrichContext) -> ExtensionFuture<Vec<T>>;
}

#[derive(Clone, Default)]
pub struct Extensions {
    page_enrichers: Vec<std::sync::Arc<dyn PageEnricher>>,
}

impl std::fmt::Debug for Extensions {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Extensions")
            .field("page_enrichers", &self.page_enrichers.len())
            .finish()
    }
}

impl Extensions {
    pub fn push_page_enricher<E: PageEnricher + 'static>(&mut self, enricher: E) {
        self.page_enrichers.push(std::sync::Arc::new(enricher));
    }

    pub async fn enrich_page(
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
            page = tokio::time::timeout(Duration::from_secs(5), enricher.enrich(page, context))
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
}
