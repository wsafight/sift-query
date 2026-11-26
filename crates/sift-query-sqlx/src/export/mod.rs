use std::time::Duration;

use futures_util::StreamExt;
use sift_query_core::{QueryInput, QueryPlan, SearchEntity, ServerScope};
pub use sift_query_sqlx_common::{CsvColumn, CsvExportPolicy};
use sift_query_sqlx_common::{
    ExportLimiter, SearchError as CommonSearchError, validate_export_policy, write_record,
    write_row_batch, write_rows,
};
use tokio::io::{AsyncWrite, AsyncWriteExt};

use crate::{PgSearchError, PgSiftQuery, RowMap, ScanPolicy, SearchStream};

fn map_csv_error(error: CommonSearchError) -> PgSearchError {
    match error {
        CommonSearchError::ScanLimitExceeded { max_rows } => {
            PgSearchError::ScanLimitExceeded { max_rows }
        }
        CommonSearchError::Timeout(_) => PgSearchError::Timeout(crate::TimeoutStage::Export),
        CommonSearchError::ExportIo(io) => PgSearchError::ExportIo(io),
        CommonSearchError::Decode { column, reason } => {
            if column == "batch_size" {
                PgSearchError::Render(crate::RenderError::InvalidCondition(reason))
            } else if column == "export" {
                PgSearchError::ParallelGateClosed
            } else {
                PgSearchError::Decode(crate::DecodeError { column, reason })
            }
        }
        other => PgSearchError::Render(crate::RenderError::InvalidCondition(other.to_string())),
    }
}

#[derive(Debug, Clone)]
pub struct CsvExporter {
    policy: CsvExportPolicy,
    columns: Vec<CsvColumn>,
    limiter: Option<ExportLimiter>,
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
        self.limiter = Some(ExportLimiter::new(maximum)?);
        Ok(self)
    }

    async fn acquire(
        &self,
        timeout: Duration,
    ) -> Result<Option<tokio::sync::OwnedSemaphorePermit>, PgSearchError> {
        let Some(limiter) = &self.limiter else {
            return Ok(None);
        };
        limiter
            .acquire_with_timeout(timeout)
            .await
            .map(Some)
            .map_err(map_csv_error)
    }

    fn validate_policy(&self) -> Result<(), PgSearchError> {
        validate_export_policy(
            self.policy.max_rows,
            self.policy.timeout,
            self.policy.batch_size,
        )
        .map_err(map_csv_error)
    }

    fn resolve_columns<E: SearchEntity>(
        &self,
        plan: &QueryPlan<E>,
    ) -> Result<Vec<CsvColumn>, PgSearchError> {
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
                .ok_or_else(|| sift_query_core::ParamError::UnknownField(column.field.clone()))?;
            if !field.selectable
                || !plan.projection.contains(&field.id)
                || !unique.insert(&column.field)
            {
                return Err(
                    sift_query_core::ParamError::NotSelectable(column.field.clone()).into(),
                );
            }
        }
        Ok(columns)
    }

    fn scan_policy(&self) -> ScanPolicy {
        ScanPolicy {
            max_rows: self.policy.max_rows,
            batch_size: self.policy.batch_size,
            timeout: self.policy.timeout,
        }
    }

    pub async fn write_page<W>(
        &self,
        writer: &mut W,
        page: &crate::Page<RowMap>,
    ) -> Result<u32, PgSearchError>
    where
        W: AsyncWrite + Unpin,
    {
        self.validate_policy()?;
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
        .map_err(map_csv_error)
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
        self.validate_policy()?;
        let _permit = self.acquire(self.policy.timeout).await?;
        search
            .observe::<E, _>(
                "csv_export",
                async {
                    let query = query.into_user_query(&search.policy)?;
                    let plan =
                        QueryPlan::new(&query, scope, sift_query_core::QueryTarget::MapPage)?;
                    let columns = self.resolve_columns(&plan)?;
                    let rows = search.scan_map_stream(&query, scope, self.scan_policy())?;
                    self.write_stream::<E, W>(search, "csv_export", rows, &columns, writer)
                        .await
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
        self.validate_policy()?;
        let _permit = self.acquire(self.policy.timeout).await?;
        search
            .observe::<E, _>(
                "csv_export_rls",
                async {
                    let query = query.into_user_query(&search.policy)?;
                    let plan =
                        QueryPlan::new(&query, scope, sift_query_core::QueryTarget::MapPage)?;
                    let columns = self.resolve_columns(&plan)?;
                    let rows =
                        search.scan_map_stream_rls(context, &query, scope, self.scan_policy())?;
                    self.write_stream::<E, W>(search, "csv_export_rls", rows, &columns, writer)
                        .await
                },
                |count| *count as usize,
            )
            .await
    }

    async fn write_stream<E: SearchEntity, W: AsyncWrite + Unpin>(
        &self,
        search: &PgSiftQuery,
        operation: &'static str,
        mut rows: SearchStream<RowMap>,
        columns: &[CsvColumn],
        writer: &mut W,
    ) -> Result<u32, PgSearchError> {
        let future = async {
            tracing::Span::current().record("rows_written", 0u64);
            write_record(writer, columns.iter().map(|column| column.title.clone()))
                .await
                .map_err(map_csv_error)?;
            let mut count = 0_u32;
            let mut batch = Vec::with_capacity(self.policy.batch_size as usize);
            while let Some(row) = rows.next().await {
                batch.push(row?);
                if batch.len() == self.policy.batch_size as usize {
                    count += self
                        .write_batch::<E, W>(
                            search,
                            operation,
                            writer,
                            std::mem::take(&mut batch),
                            columns,
                        )
                        .await?;
                    tracing::Span::current().record("rows_written", count);
                }
            }
            if !batch.is_empty() {
                count += self
                    .write_batch::<E, W>(search, operation, writer, batch, columns)
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
                operation,
                outcome = "partial_failure",
                "CSV export failed; output may be partial"
            );
        }
        result
    }

    async fn write_batch<E: SearchEntity, W: AsyncWrite + Unpin>(
        &self,
        search: &PgSiftQuery,
        operation: &'static str,
        writer: &mut W,
        mut data: Vec<RowMap>,
        columns: &[CsvColumn],
    ) -> Result<u32, PgSearchError> {
        search.extensions.filter_rows::<E>(operation, &mut data)?;
        let page = search
            .extensions
            .enrich_page(
                E::META.entity_name,
                operation,
                crate::Page {
                    data,
                    total_count: 0,
                    summaries: Vec::new(),
                    next_cursor: None,
                },
            )
            .await?;
        write_row_batch(writer, &page.data, columns)
            .await
            .map_err(map_csv_error)
    }
}
