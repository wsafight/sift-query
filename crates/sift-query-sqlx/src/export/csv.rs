use super::CsvColumn;
use crate::{PgSearchError, RowMap};
use tokio::io::{AsyncWrite, AsyncWriteExt};

pub(super) fn validate_export_policy(
    max_rows: u32,
    timeout: std::time::Duration,
    batch_size: u32,
) -> Result<(), PgSearchError> {
    if max_rows == 0 {
        return Err(PgSearchError::ScanLimitExceeded { max_rows: 0 });
    }
    if timeout.is_zero() {
        return Err(PgSearchError::Timeout(crate::TimeoutStage::Export));
    }
    if batch_size == 0 || batch_size > 16_384 {
        return Err(PgSearchError::Render(crate::RenderError::InvalidCondition(
            "batch size must be between 1 and 16384".to_owned(),
        )));
    }
    Ok(())
}

pub(super) async fn write_rows<W>(
    writer: &mut W,
    rows: &[RowMap],
    columns: &[CsvColumn],
) -> Result<u32, PgSearchError>
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
    writer.flush().await.map_err(PgSearchError::ExportIo)?;
    Ok(u32::try_from(rows.len()).unwrap_or(u32::MAX))
}

pub(super) async fn write_row_batch<W>(
    writer: &mut W,
    rows: &[RowMap],
    columns: &[CsvColumn],
) -> Result<u32, PgSearchError>
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
                    .map_err(PgSearchError::ExportIo)?;
            }
            return Err(error);
        }
        if output.len() >= MAX_BUFFER_BYTES {
            writer
                .write_all(output.as_bytes())
                .await
                .map_err(PgSearchError::ExportIo)?;
            output.clear();
        }
    }
    if !output.is_empty() {
        writer
            .write_all(output.as_bytes())
            .await
            .map_err(PgSearchError::ExportIo)?;
    }
    Ok(u32::try_from(rows.len()).unwrap_or(u32::MAX))
}

fn encode_row_into(
    line: &mut String,
    row: &RowMap,
    columns: &[CsvColumn],
) -> Result<(), PgSearchError> {
    line.reserve(columns.len().saturating_mul(8));
    for (index, column) in columns.iter().enumerate() {
        if index > 0 {
            line.push(',');
        }
        let value = row
            .get(&column.field)
            .unwrap_or(&sift_query_core::RowValue::Null);
        let raw = column.formatter.map_or_else(
            || Ok::<String, PgSearchError>(display_value(value)),
            |format| {
                format(value).map_err(|reason| {
                    crate::DecodeError {
                        column: column.field.clone(),
                        reason,
                    }
                    .into()
                })
            },
        )?;
        append_field(line, &raw);
    }
    line.push('\n');
    Ok(())
}

fn display_value(value: &sift_query_core::RowValue) -> String {
    value.to_external_text()
}

pub(super) async fn write_record<W, I>(writer: &mut W, values: I) -> Result<(), PgSearchError>
where
    W: AsyncWrite + Unpin,
    I: IntoIterator<Item = String>,
{
    let mut line = String::new();
    write_record_into(writer, &mut line, values).await
}

async fn write_record_into<W, I>(
    writer: &mut W,
    line: &mut String,
    values: I,
) -> Result<(), PgSearchError>
where
    W: AsyncWrite + Unpin,
    I: IntoIterator<Item = String>,
{
    line.clear();
    for (index, raw) in values.into_iter().enumerate() {
        if index > 0 {
            line.push(',');
        }
        append_field(line, &raw);
    }
    line.push('\n');
    writer
        .write_all(line.as_bytes())
        .await
        .map_err(PgSearchError::ExportIo)
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
