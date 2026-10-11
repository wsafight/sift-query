use futures_util::StreamExt;
use sift_query_core::{ConfigError, QueryInput, SearchEntity, ServerScope};
use tokio::io::{AsyncWrite, AsyncWriteExt};

use sift_query_sqlx_common::{
    ExportLimiter, validate_export_policy, write_record, write_row_batch, write_rows,
};

use crate::{CsvColumn, CsvExportPolicy, Page, RowMap, ScanPolicy, SearchError, SqliteSiftQuery};

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

    #[must_use]
    pub fn with_columns(mut self, columns: impl IntoIterator<Item = CsvColumn>) -> Self {
        self.columns = columns.into_iter().collect();
        self
    }

    pub fn with_concurrency(mut self, maximum: usize) -> Result<Self, ConfigError> {
        self.limiter = Some(ExportLimiter::new(maximum)?);
        Ok(self)
    }

    pub async fn write_page<W>(
        &self,
        writer: &mut W,
        page: &Page<RowMap>,
    ) -> Result<u32, SearchError>
    where
        W: AsyncWrite + Unpin,
    {
        validate_export_policy(
            self.policy.max_rows,
            self.policy.timeout,
            self.policy.batch_size,
        )?;
        let _permit = match &self.limiter {
            Some(limiter) => Some(limiter.acquire_with_timeout(self.policy.timeout).await?),
            None => None,
        };
        if page.data.len() > self.policy.max_rows as usize {
            return Err(SearchError::ScanLimitExceeded {
                max_rows: self.policy.max_rows,
            });
        }
        tokio::time::timeout(
            self.policy.timeout,
            write_rows(writer, &page.data, &self.columns),
        )
        .await
        .map_err(|_| SearchError::Timeout(sift_query_sqlx_common::TimeoutStage::Export))?
    }

    pub async fn export<E, W>(
        &self,
        search: &SqliteSiftQuery,
        query: impl QueryInput<E>,
        scope: &ServerScope<E>,
        writer: &mut W,
    ) -> Result<u32, SearchError>
    where
        E: SearchEntity + Send + Unpin + 'static,
        W: AsyncWrite + Unpin,
    {
        validate_export_policy(
            self.policy.max_rows,
            self.policy.timeout,
            self.policy.batch_size,
        )?;
        let _permit = match &self.limiter {
            Some(limiter) => Some(limiter.acquire_with_timeout(self.policy.timeout).await?),
            None => None,
        };
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
        validate_columns::<E>(&plan, &columns)?;
        let mut rows = search.scan_map_stream(
            &query,
            scope,
            ScanPolicy {
                max_rows: self.policy.max_rows,
                batch_size: self.policy.batch_size,
                timeout: self.policy.timeout,
            },
        )?;
        let export = async {
            tokio::time::timeout(self.policy.timeout, async {
                write_record(writer, columns.iter().map(|column| column.title.clone())).await?;
                let mut count = 0_u32;
                let mut batch = Vec::with_capacity(self.policy.batch_size as usize);
                while let Some(row) = rows.next().await {
                    batch.push(row?);
                    if batch.len() == self.policy.batch_size as usize {
                        count += write_batch::<E, W>(
                            search,
                            writer,
                            std::mem::take(&mut batch),
                            &columns,
                        )
                        .await?;
                    }
                }
                if !batch.is_empty() {
                    count += write_batch::<E, W>(search, writer, batch, &columns).await?;
                }
                writer.flush().await.map_err(SearchError::ExportIo)?;
                Ok(count)
            })
            .await
            .map_err(|_| SearchError::Timeout(sift_query_sqlx_common::TimeoutStage::Export))?
        };
        search
            .observers
            .observe(E::META.entity_name, "csv_export", export, |count| {
                *count as usize
            })
            .await
    }
}

fn validate_columns<E: SearchEntity>(
    plan: &sift_query_core::QueryPlan<E>,
    columns: &[CsvColumn],
) -> Result<(), SearchError> {
    let mut seen = std::collections::HashSet::new();
    for column in columns {
        let source = column.label_source.as_deref().unwrap_or(&column.field);
        let field = E::META
            .resolve_field(source)
            .ok_or_else(|| sift_query_core::ParamError::UnknownField(column.field.clone()))?;
        if !field.selectable || !plan.projection.contains(&field.id) || !seen.insert(&column.field)
        {
            return Err(sift_query_core::ParamError::NotSelectable(column.field.clone()).into());
        }
    }
    Ok(())
}

async fn write_batch<E, W>(
    search: &SqliteSiftQuery,
    writer: &mut W,
    rows: Vec<RowMap>,
    columns: &[CsvColumn],
) -> Result<u32, SearchError>
where
    E: SearchEntity,
    W: AsyncWrite + Unpin,
{
    let page = search
        .extensions
        .enrich_page(
            E::META.entity_name,
            "csv_export",
            Page {
                data: rows,
                total_count: 0,
                summaries: Vec::new(),
                next_cursor: None,
            },
        )
        .await
        .map_err(SearchError::Extension)?;
    write_row_batch(writer, &page.data, columns).await
}
