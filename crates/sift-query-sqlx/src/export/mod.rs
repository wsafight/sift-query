use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;
use sift_query_core::{QueryInput, SearchEntity, ServerScope};
use tokio::io::{AsyncWrite, AsyncWriteExt};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
mod csv;
use csv::{write_record, write_row_batch, write_rows};

use crate::{PgSearchError, PgSiftQuery, RowMap, ScanPolicy};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CsvExportPolicy {
    pub max_rows: u32,
    pub timeout: Duration,
    pub batch_size: u32,
}

impl Default for CsvExportPolicy {
    fn default() -> Self {
        Self {
            max_rows: 100_000,
            timeout: Duration::from_secs(120),
            batch_size: 256,
        }
    }
}

/// A server-owned export column and optional trusted formatter.
#[derive(Debug, Clone)]
pub struct CsvColumn {
    pub field: String,
    pub title: String,
    /// A declared source field for a post-query label column.
    pub label_source: Option<String>,
    pub formatter: Option<fn(&sift_query_core::RowValue) -> Result<String, String>>,
}

impl CsvColumn {
    #[must_use]
    pub fn new(field: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            title: title.into(),
            label_source: None,
            formatter: None,
        }
    }

    #[must_use]
    pub fn label(source: impl Into<String>, title: impl Into<String>) -> Self {
        let source = source.into();
        Self {
            field: format!("{source}Label"),
            title: title.into(),
            label_source: Some(source),
            formatter: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CsvExporter {
    policy: CsvExportPolicy,
    columns: Vec<CsvColumn>,
    limiter: Option<Arc<Semaphore>>,
}

impl CsvExporter {
    #[must_use]
    pub const fn new(policy: CsvExportPolicy) -> Self {
        Self {
            policy,
            columns: Vec::new(),
            limiter: None,
        }
    }

    #[must_use]
    pub const fn policy(&self) -> CsvExportPolicy {
        self.policy
    }

    /// Fixes the export whitelist, headings and order on the server. Empty
    /// columns use the entity's selectable projection in metadata order.
    #[must_use]
    pub fn with_columns(mut self, columns: impl IntoIterator<Item = CsvColumn>) -> Self {
        self.columns = columns.into_iter().collect();
        self
    }

    pub fn with_concurrency(
        mut self,
        maximum: usize,
    ) -> Result<Self, sift_query_core::ConfigError> {
        if maximum == 0 || maximum > 1024 {
            return Err(sift_query_core::ConfigError::Invalid(
                "export concurrency must be between 1 and 1024".to_owned(),
            ));
        }
        self.limiter = Some(Arc::new(Semaphore::new(maximum)));
        Ok(self)
    }

    async fn acquire(
        &self,
        timeout: Duration,
    ) -> Result<Option<OwnedSemaphorePermit>, PgSearchError> {
        let Some(limiter) = &self.limiter else {
            return Ok(None);
        };
        tokio::time::timeout(timeout, limiter.clone().acquire_owned())
            .await
            .map_err(|_| PgSearchError::Timeout(crate::TimeoutStage::Export))?
            .map(Some)
            .map_err(|_| PgSearchError::ParallelGateClosed)
    }

    pub async fn write_page<W>(
        &self,
        writer: &mut W,
        page: &crate::Page<RowMap>,
    ) -> Result<u32, PgSearchError>
    where
        W: AsyncWrite + Unpin,
    {
        csv::validate_export_policy(
            self.policy.max_rows,
            self.policy.timeout,
            self.policy.batch_size,
        )?;
        let _permit = self.acquire(self.policy.timeout).await?;
        if page.data.len() > self.policy.max_rows as usize {
            return Err(PgSearchError::ScanLimitExceeded {
                max_rows: self.policy.max_rows,
            });
        }
        tokio::time::timeout(
            self.policy.timeout,
            write_rows(writer, &page.data, &self.columns),
        )
        .await
        .map_err(|_| PgSearchError::Timeout(crate::TimeoutStage::Export))?
    }

    /// Runs a bounded dynamic scan and writes escaped CSV to the caller's
    /// response or file writer.
    pub async fn export<E, W>(
        &self,
        search: &PgSiftQuery,
        query: impl QueryInput<E>,
        scope: &ServerScope<E>,
        writer: &mut W,
    ) -> Result<u32, PgSearchError>
    where
        E: SearchEntity + Send + Unpin + 'static,
        W: AsyncWrite + Unpin,
    {
        csv::validate_export_policy(
            self.policy.max_rows,
            self.policy.timeout,
            self.policy.batch_size,
        )?;
        let _permit = self.acquire(self.policy.timeout).await?;
        search
            .observe::<E, _>(
                "csv_export",
                async {
                    let query = query.into_user_query(&search.policy)?;
                    let plan = sift_query_core::QueryPlan::new(
                        &query,
                        scope,
                        sift_query_core::QueryTarget::MapPage,
                    )?;
                    let columns = if self.columns.is_empty() {
                        plan.projection
                            .iter()
                            .filter_map(|id| E::META.field(*id))
                            .map(|field| CsvColumn::new(field.public_name, field.public_name))
                            .collect()
                    } else {
                        self.columns.clone()
                    };
                    let mut unique = std::collections::HashSet::new();
                    for column in &columns {
                        let field = E::META
                            .resolve_field(column.label_source.as_deref().unwrap_or(&column.field))
                            .ok_or_else(|| {
                                sift_query_core::ParamError::UnknownField(column.field.clone())
                            })?;
                        if !field.selectable
                            || !plan.projection.contains(&field.id)
                            || !unique.insert(&column.field)
                        {
                            return Err(sift_query_core::ParamError::NotSelectable(
                                column.field.clone(),
                            )
                            .into());
                        }
                    }
                    let mut rows = search.scan_map_stream(
                        &query,
                        scope,
                        ScanPolicy {
                            max_rows: self.policy.max_rows,
                            batch_size: self.policy.batch_size,
                            timeout: self.policy.timeout,
                        },
                    )?;
                    let future = async {
                        tracing::Span::current().record("rows_written", 0u64);
                        write_record(writer, columns.iter().map(|column| column.title.clone()))
                            .await?;
                        let mut count = 0_u32;
                        let mut batch = Vec::with_capacity(self.policy.batch_size as usize);
                        while let Some(row) = rows.next().await {
                            batch.push(row?);
                            if batch.len() == self.policy.batch_size as usize {
                                count += self
                                    .write_batch::<E, W>(
                                        search,
                                        writer,
                                        std::mem::take(&mut batch),
                                        &columns,
                                    )
                                    .await?;
                                tracing::Span::current().record("rows_written", count);
                            }
                        }
                        if !batch.is_empty() {
                            count += self
                                .write_batch::<E, W>(search, writer, batch, &columns)
                                .await?;
                            tracing::Span::current().record("rows_written", count);
                        }
                        writer.flush().await.map_err(PgSearchError::ExportIo)?;
                        Ok(count)
                    };
                    let result = tokio::time::timeout(self.policy.timeout, future)
                        .await
                        .map_err(|_| PgSearchError::Timeout(crate::TimeoutStage::Export))?;
                    if result.is_err() {
                        tracing::warn!(
                            operation = "csv_export",
                            outcome = "partial_failure",
                            "CSV export failed; output may be partial"
                        );
                    }
                    result
                },
                |count| *count as usize,
            )
            .await
    }

    #[cfg(feature = "rls")]
    pub async fn export_rls<E, W>(
        &self,
        search: &PgSiftQuery,
        context: &crate::RlsContext,
        query: impl QueryInput<E>,
        scope: &ServerScope<E>,
        writer: &mut W,
    ) -> Result<u32, PgSearchError>
    where
        E: SearchEntity + Unpin,
        W: AsyncWrite + Unpin,
    {
        csv::validate_export_policy(
            self.policy.max_rows,
            self.policy.timeout,
            self.policy.batch_size,
        )?;
        let _permit = self.acquire(self.policy.timeout).await?;
        let query = query.into_user_query(&search.policy)?;
        let plan =
            sift_query_core::QueryPlan::new(&query, scope, sift_query_core::QueryTarget::MapPage)?;
        let columns = if self.columns.is_empty() {
            plan.projection
                .iter()
                .filter_map(|id| E::META.field(*id))
                .map(|field| CsvColumn::new(field.public_name, field.public_name))
                .collect()
        } else {
            self.columns.clone()
        };
        let mut seen = std::collections::HashSet::new();
        for column in &columns {
            let field = E::META
                .resolve_field(column.label_source.as_deref().unwrap_or(&column.field))
                .ok_or_else(|| sift_query_core::ParamError::UnknownField(column.field.clone()))?;
            if !field.selectable
                || !plan.projection.contains(&field.id)
                || !seen.insert(&column.field)
            {
                return Err(
                    sift_query_core::ParamError::NotSelectable(column.field.clone()).into(),
                );
            }
        }
        let mut rows = search.scan_map_stream_rls(
            context,
            &query,
            scope,
            ScanPolicy {
                max_rows: self.policy.max_rows,
                batch_size: self.policy.batch_size,
                timeout: self.policy.timeout,
            },
        )?;
        tokio::time::timeout(self.policy.timeout, async {
            write_record(writer, columns.iter().map(|column| column.title.clone())).await?;
            let mut count = 0;
            let mut batch = Vec::with_capacity(self.policy.batch_size as usize);
            while let Some(row) = rows.next().await {
                batch.push(row?);
                if batch.len() == self.policy.batch_size as usize {
                    count += self
                        .write_batch::<E, W>(search, writer, std::mem::take(&mut batch), &columns)
                        .await?;
                }
            }
            if !batch.is_empty() {
                count += self
                    .write_batch::<E, W>(search, writer, batch, &columns)
                    .await?;
            }
            writer.flush().await.map_err(PgSearchError::ExportIo)?;
            Ok(count)
        })
        .await
        .map_err(|_| PgSearchError::Timeout(crate::TimeoutStage::Export))?
    }

    async fn write_batch<E: SearchEntity, W: AsyncWrite + Unpin>(
        &self,
        search: &PgSiftQuery,
        writer: &mut W,
        mut data: Vec<RowMap>,
        columns: &[CsvColumn],
    ) -> Result<u32, PgSearchError> {
        search
            .extensions
            .filter_rows::<E>("csv_export", &mut data)?;
        let page = search
            .extensions
            .enrich_page(
                E::META.entity_name,
                "csv_export",
                crate::Page {
                    data,
                    total_count: 0,
                    summaries: Vec::new(),
                    next_cursor: None,
                },
            )
            .await?;
        write_row_batch(writer, &page.data, columns).await
    }
}
