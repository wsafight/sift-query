use indexmap::IndexMap;
use sift_query_core::RowValue;
use std::sync::Arc;
use tokio::io::{AsyncWrite, AsyncWriteExt};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use crate::SearchError;

/// Validates the limits shared by all backend CSV exporters. Keeping this in
/// common prevents a zero batch size or zero timeout from producing a busy
/// loop in one backend while another accepts it.
pub fn validate_export_policy(
    max_rows: u32,
    timeout: std::time::Duration,
    batch_size: u32,
) -> Result<(), SearchError> {
    if max_rows == 0 {
        return Err(SearchError::ScanLimitExceeded { max_rows: 0 });
    }
    if timeout.is_zero() {
        return Err(SearchError::Timeout(crate::TimeoutStage::Export));
    }
    if batch_size == 0 || batch_size > 16_384 {
        return Err(SearchError::Decode {
            column: "batch_size".to_owned(),
            reason: "batch size must be between 1 and 16384".to_owned(),
        });
    }
    Ok(())
}

/// HTTP metadata for a generated CSV response. The filename is validated so
/// it cannot inject response headers or path separators.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CsvExportDescriptor {
    pub filename: String,
    pub content_type: &'static str,
    pub content_disposition: String,
}

/// Bounds concurrent exports independently from query scan concurrency.
#[derive(Clone)]
pub struct ExportLimiter(Arc<Semaphore>);

impl std::fmt::Debug for ExportLimiter {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ExportLimiter")
            .finish_non_exhaustive()
    }
}

impl Default for ExportLimiter {
    fn default() -> Self {
        Self(Arc::new(Semaphore::new(1)))
    }
}

impl ExportLimiter {
    pub fn new(maximum: usize) -> Result<Self, sift_query_core::ConfigError> {
        if maximum == 0 || maximum > 1024 {
            return Err(sift_query_core::ConfigError::Invalid(
                "export concurrency must be between 1 and 1024".to_owned(),
            ));
        }
        Ok(Self(Arc::new(Semaphore::new(maximum))))
    }

    pub async fn acquire(&self) -> Result<OwnedSemaphorePermit, SearchError> {
        self.0
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| SearchError::Decode {
                column: "export".to_owned(),
                reason: "export limiter is closed".to_owned(),
            })
    }

    pub async fn acquire_with_timeout(
        &self,
        timeout: std::time::Duration,
    ) -> Result<OwnedSemaphorePermit, SearchError> {
        tokio::time::timeout(timeout, self.acquire())
            .await
            .map_err(|_| SearchError::Timeout(crate::TimeoutStage::Export))?
    }
}

impl CsvExportDescriptor {
    pub fn new(filename: impl Into<String>) -> Result<Self, String> {
        let filename = filename.into();
        if filename.is_empty()
            || filename.len() > 128
            || filename.contains(['/', '\\', '\r', '\n', '"'])
            || filename.bytes().any(|byte| byte < 0x20 || byte == 0x7f)
            || !filename.to_ascii_lowercase().ends_with(".csv")
        {
            return Err("CSV filename must be a safe .csv basename".to_owned());
        }
        Ok(Self {
            content_disposition: format!("attachment; filename=\"{filename}\""),
            filename,
            content_type: "text/csv; charset=utf-8",
        })
    }
}

/// A server-owned export column and optional trusted formatter.
#[derive(Debug, Clone)]
pub struct CsvColumn {
    pub field: String,
    pub title: String,
    /// A declared source field for a post-query label column.
    pub label_source: Option<String>,
    pub formatter: Option<fn(&RowValue) -> Result<String, String>>,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CsvExportPolicy {
    pub max_rows: u32,
    pub timeout: std::time::Duration,
    pub batch_size: u32,
}

impl Default for CsvExportPolicy {
    fn default() -> Self {
        Self {
            max_rows: 100_000,
            timeout: std::time::Duration::from_secs(120),
            batch_size: 256,
        }
    }
}

pub type RowMap = IndexMap<String, RowValue>;

pub async fn write_rows<W>(
    writer: &mut W,
    rows: &[RowMap],
    columns: &[CsvColumn],
) -> Result<u32, SearchError>
where
    W: AsyncWrite + Unpin,
{
    let columns = if columns.is_empty() {
        rows.first().map_or_else(Vec::new, |first| {
            first
                .keys()
                .map(|field| CsvColumn::new(field, field))
                .collect::<Vec<_>>()
        })
    } else {
        columns.to_vec()
    };
    if columns.is_empty() {
        return Ok(0);
    }
    let mut line = String::new();
    write_record_into(
        &mut *writer,
        &mut line,
        columns.iter().map(|column| column.title.clone()),
    )
    .await?;
    write_row_batch(writer, rows, &columns).await?;
    writer.flush().await.map_err(SearchError::ExportIo)?;
    Ok(u32::try_from(rows.len()).unwrap_or(u32::MAX))
}

pub fn column_values(row: &RowMap, columns: &[CsvColumn]) -> Result<Vec<String>, SearchError> {
    columns
        .iter()
        .map(|column| {
            let value = row.get(&column.field).unwrap_or(&RowValue::Null);
            column.formatter.map_or_else(
                || Ok(value.to_external_text()),
                |format| {
                    format(value).map_err(|reason| SearchError::Decode {
                        column: column.field.clone(),
                        reason,
                    })
                },
            )
        })
        .collect()
}

/// Writes one row without allocating an intermediate vector of cell strings.
pub async fn write_row_into<W>(
    writer: &mut W,
    line: &mut String,
    row: &RowMap,
    columns: &[CsvColumn],
) -> Result<(), SearchError>
where
    W: AsyncWrite + Unpin,
{
    line.clear();
    encode_row_into(line, row, columns)?;
    writer
        .write_all(line.as_bytes())
        .await
        .map_err(SearchError::ExportIo)
}

/// Writes a batch of rows using bounded output chunks. Formatting still
/// happens row by row, while the number of async writer calls is independent
/// of the row count for normal-sized records.
pub async fn write_row_batch<W>(
    writer: &mut W,
    rows: &[RowMap],
    columns: &[CsvColumn],
) -> Result<u32, SearchError>
where
    W: AsyncWrite + Unpin,
{
    const MAX_BUFFER_BYTES: usize = 64 * 1024;
    let mut output = String::new();
    for row in rows {
        let complete_rows = output.len();
        if let Err(error) = encode_row_into(&mut output, row, columns) {
            // Preserve completed records and never emit a partially formatted row.
            output.truncate(complete_rows);
            if !output.is_empty() {
                writer
                    .write_all(output.as_bytes())
                    .await
                    .map_err(SearchError::ExportIo)?;
            }
            return Err(error);
        }
        if output.len() >= MAX_BUFFER_BYTES {
            writer
                .write_all(output.as_bytes())
                .await
                .map_err(SearchError::ExportIo)?;
            output.clear();
        }
    }
    if !output.is_empty() {
        writer
            .write_all(output.as_bytes())
            .await
            .map_err(SearchError::ExportIo)?;
    }
    Ok(u32::try_from(rows.len()).unwrap_or(u32::MAX))
}

fn encode_row_into(
    line: &mut String,
    row: &RowMap,
    columns: &[CsvColumn],
) -> Result<(), SearchError> {
    line.reserve(columns.len().saturating_mul(8));
    for (index, column) in columns.iter().enumerate() {
        if index > 0 {
            line.push(',');
        }
        let value = row.get(&column.field).unwrap_or(&RowValue::Null);
        let raw = column.formatter.map_or_else(
            || Ok(value.to_external_text()),
            |format| {
                format(value).map_err(|reason| SearchError::Decode {
                    column: column.field.clone(),
                    reason,
                })
            },
        )?;
        append_field(&mut *line, &raw);
    }
    line.push('\n');
    Ok(())
}

pub async fn write_record<W, I>(writer: &mut W, values: I) -> Result<(), SearchError>
where
    W: AsyncWrite + Unpin,
    I: IntoIterator<Item = String>,
{
    let mut line = String::new();
    write_record_into(writer, &mut line, values).await
}

/// Writes one CSV record into a reusable line buffer.
pub async fn write_record_into<W, I>(
    writer: &mut W,
    line: &mut String,
    values: I,
) -> Result<(), SearchError>
where
    W: AsyncWrite + Unpin,
    I: IntoIterator<Item = String>,
{
    line.clear();
    for (index, raw) in values.into_iter().enumerate() {
        if index > 0 {
            line.push(',');
        }
        append_field(&mut *line, &raw);
    }
    line.push('\n');
    writer
        .write_all(line.as_bytes())
        .await
        .map_err(SearchError::ExportIo)
}

fn append_field(line: &mut String, raw: &str) {
    let protect_formula = matches!(
        raw.trim_start_matches([' ', '\t', '\r', '\n'])
            .as_bytes()
            .first(),
        Some(b'=' | b'+' | b'-' | b'@')
    ) || matches!(raw.as_bytes().first(), Some(b'\t' | b'\r' | b'\n'));
    let quoted = raw
        .bytes()
        .any(|byte| matches!(byte, b',' | b'"' | b'\n' | b'\r'));
    if quoted {
        line.push('"');
        if protect_formula {
            line.push('\'');
        }
        for character in raw.chars() {
            if character == '"' {
                line.push_str("\"\"");
            } else {
                line.push(character);
            }
        }
        line.push('"');
    } else {
        if protect_formula {
            line.push('\'');
        }
        line.push_str(raw);
    }
}
