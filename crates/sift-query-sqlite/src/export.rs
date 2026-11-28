use sift_query_core::{ConfigError, QueryInput, SearchEntity, ServerScope};
use sift_query_sqlx_common::{
    ExportLimiter, export_csv_stream, resolve_export_columns, validate_export_policy,
    write_csv_page,
};
use tokio::io::AsyncWrite;

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
        write_csv_page(
            self.policy,
            &self.columns,
            self.limiter.as_ref(),
            writer,
            page,
        )
        .await
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
        let columns = resolve_export_columns::<E>(&plan, &self.columns)?;
        let rows = search.scan_map_stream(
            &query,
            scope,
            ScanPolicy {
                max_rows: self.policy.max_rows,
                batch_size: self.policy.batch_size,
                timeout: self.policy.timeout,
            },
        )?;
        export_csv_stream::<E, W>(
            self.policy,
            &columns,
            &search.observers,
            &search.extensions,
            rows,
            writer,
        )
        .await
    }
}
