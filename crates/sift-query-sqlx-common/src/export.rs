use futures_util::StreamExt;
use sift_query_core::{QueryPlan, SearchEntity};
use tokio::io::{AsyncWrite, AsyncWriteExt};

use crate::{
    CsvColumn, CsvExportPolicy, ExportLimiter, Extensions, Observers, Page, RowMap, SearchError,
    SearchStream, TimeoutStage, validate_export_policy, write_record, write_row_batch, write_rows,
};

/// Writes a single pre-fetched `Page<RowMap>` as CSV with policy and concurrency enforcement.
pub async fn write_csv_page<W>(
    policy: CsvExportPolicy,
    columns: &[CsvColumn],
    limiter: Option<&ExportLimiter>,
    writer: &mut W,
    page: &Page<RowMap>,
) -> Result<u32, SearchError>
where
    W: AsyncWrite + Unpin,
{
    validate_export_policy(policy.max_rows, policy.timeout, policy.batch_size)?;
    let _permit = match limiter {
        Some(limiter) => Some(limiter.acquire_with_timeout(policy.timeout).await?),
        None => None,
    };
    if page.data.len() > policy.max_rows as usize {
        return Err(SearchError::ScanLimitExceeded {
            max_rows: policy.max_rows,
        });
    }
    tokio::time::timeout(policy.timeout, write_rows(writer, &page.data, columns))
        .await
        .map_err(|_| SearchError::Timeout(TimeoutStage::Export))?
}

/// Resolves and validates export columns against a validated `QueryPlan`.
pub fn resolve_export_columns<E: SearchEntity>(
    plan: &QueryPlan<E>,
    configured: &[CsvColumn],
) -> Result<Vec<CsvColumn>, SearchError> {
    let columns = if configured.is_empty() {
        plan.projection
            .iter()
            .filter_map(|id| E::META.field(*id))
            .map(|field| CsvColumn::new(field.public_name, field.public_name))
            .collect()
    } else {
        configured.to_vec()
    };
    let mut seen = std::collections::HashSet::with_capacity(columns.len());
    for column in &columns {
        let source = column.label_source.as_deref().unwrap_or(&column.field);
        let field = E::META
            .resolve_field(source)
            .ok_or_else(|| sift_query_core::ParamError::UnknownField(column.field.clone()))?;
        if !field.selectable || !plan.projection.contains(&field.id) || !seen.insert(&column.field)
        {
            return Err(sift_query_core::ParamError::NotSelectable(column.field.clone()).into());
        }
    }
    Ok(columns)
}

/// Streams a `SearchStream<RowMap>` into a CSV writer with batching, enrichment, and observation.
#[allow(clippy::too_many_arguments)]
pub async fn export_csv_stream<E, W>(
    policy: CsvExportPolicy,
    columns: &[CsvColumn],
    observers: &Observers,
    extensions: &Extensions,
    mut rows: SearchStream<RowMap>,
    writer: &mut W,
) -> Result<u32, SearchError>
where
    E: SearchEntity,
    W: AsyncWrite + Unpin,
{
    let export = async {
        tokio::time::timeout(policy.timeout, async {
            write_record(writer, columns.iter().map(|column| column.title.clone())).await?;
            let mut count = 0_u32;
            let mut batch = Vec::with_capacity(policy.batch_size as usize);
            while let Some(row) = rows.next().await {
                batch.push(row?);
                if batch.len() == policy.batch_size as usize {
                    count += write_enriched_batch::<E, W>(
                        extensions,
                        writer,
                        std::mem::take(&mut batch),
                        columns,
                    )
                    .await?;
                }
            }
            if !batch.is_empty() {
                count += write_enriched_batch::<E, W>(extensions, writer, batch, columns).await?;
            }
            writer.flush().await.map_err(SearchError::ExportIo)?;
            Ok(count)
        })
        .await
        .map_err(|_| SearchError::Timeout(TimeoutStage::Export))?
    };
    observers
        .observe(E::META.entity_name, "csv_export", export, |count| {
            *count as usize
        })
        .await
}

async fn write_enriched_batch<E, W>(
    extensions: &Extensions,
    writer: &mut W,
    rows: Vec<RowMap>,
    columns: &[CsvColumn],
) -> Result<u32, SearchError>
where
    E: SearchEntity,
    W: AsyncWrite + Unpin,
{
    let page = extensions
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
