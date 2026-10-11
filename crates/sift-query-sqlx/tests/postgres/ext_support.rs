use super::support::*;

/// Rewrites every query onto the unqualified `wide` table.
pub(super) struct RewriteToWide;

impl PartitionResolver for RewriteToWide {
    fn resolve(
        &self,
        _context: &QueryContext<'_>,
    ) -> Result<Option<PartitionName>, ExtensionError> {
        Ok(Some(PartitionName::new("wide")?))
    }
}

/// Rejects SQL that already carries a partition rewrite.
pub(super) struct RejectWide;

impl SqlInterceptor for RejectWide {
    fn intercept(
        &self,
        _context: &SqlContext,
        sql: &str,
    ) -> Result<Option<String>, ExtensionError> {
        assert!(sql.contains("FROM \"wide\""), "{sql}");
        Err(ExtensionError::rejected(
            "test",
            "partitioned queries are disabled",
        ))
    }
}

pub(super) struct DropTitle;

impl ResultFilter for DropTitle {
    fn filter(
        &self,
        _context: &ResultContext,
        _row_index: usize,
        row: &mut sift_query_sqlx::RowMap,
    ) -> Result<ResultFilterDecision, ExtensionError> {
        if row.get("title") == Some(&RowValue::Text("beta".to_owned())) {
            Ok(ResultFilterDecision::Discard)
        } else {
            Ok(ResultFilterDecision::Keep)
        }
    }
}

pub(super) struct NullPartition;

impl PartitionResolver for NullPartition {
    fn resolve(
        &self,
        _context: &QueryContext<'_>,
    ) -> Result<Option<PartitionName>, ExtensionError> {
        Ok(None)
    }
}
