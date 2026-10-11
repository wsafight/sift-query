use sift_query_core::{
    BucketRequest, BucketResult, QueryInput, QueryTarget, SearchEntity, ServerScope,
};
use sqlx::Row;
use tokio::time::timeout_at;

use crate::execution::io::{configure_statement_timeout, finish_statement, prepare};
use crate::{PgSearchError, PgSiftQuery, TimeoutStage};

impl PgSiftQuery {
    /// Executes bounded time-series or numeric buckets with the same filters
    /// and authorization scope as the source query.
    pub async fn buckets<E: SearchEntity>(
        &self,
        query: impl QueryInput<E>,
        scope: &ServerScope<E>,
        requests: &[BucketRequest<E>],
    ) -> Result<Vec<BucketResult>, PgSearchError> {
        if requests.len() > 8 {
            return Err(sift_query_core::ParamError::TooComplex("bucket count").into());
        }
        if requests.is_empty() {
            return Ok(Vec::new());
        }
        let plan = self.resolve_plan(query, scope, QueryTarget::Count, "buckets")?;
        let mut output = Vec::with_capacity(requests.len());
        for request in requests {
            if request.schema_fingerprint() != E::META.schema_fingerprint {
                return Err(sift_query_core::ParamError::UnknownField(
                    "bucket entity schema mismatch".to_owned(),
                )
                .into());
            }
            let field = E::META.field(request.field_id()).ok_or_else(|| {
                sift_query_core::ParamError::UnknownField(format!("#{}", request.field_id().0))
            })?;
            let rendered = self.renderer.render_bucket(&plan, request)?;
            let prepared = prepare(rendered)?;
            let deadline = self.deadline::<E>();
            let connection = self.acquire(deadline).await?;
            let mut connection =
                configure_statement_timeout(connection, deadline, TimeoutStage::ClusterQuery)
                    .await?;
            let result = timeout_at(
                deadline,
                sqlx::query_with(prepared.sql(), prepared.arguments).fetch_all(&mut *connection),
            )
            .await;
            let Ok(result) = result else {
                crate::execution::io::close_hard(connection).await;
                return Err(PgSearchError::Timeout(TimeoutStage::ClusterQuery));
            };
            let (mut connection, rows) =
                finish_statement(connection, result, TimeoutStage::ClusterQuery).await?;
            let mut buckets = Vec::with_capacity(rows.len());
            for row in rows {
                buckets.push(sift_query_core::BucketEntry {
                    label: row.try_get::<String, _>("sq_bucket")?,
                    count: row.try_get::<i64, _>("sq_count")?,
                });
            }
            let _ = &mut connection;
            output.push(BucketResult {
                field: field.public_name,
                buckets,
            });
        }
        Ok(output)
    }
}
