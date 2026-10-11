use sift_query_core::{
    FacetBucket, FacetRequest, FacetResult, QueryInput, QueryTarget, SearchEntity, ServerScope,
};
use sqlx::Row;
use sqlx::postgres::{PgConnection, PgRow};
use tokio::time::{Instant, timeout_at};

use crate::execution::io::{configure_statement_timeout, finish_statement, prepare};
use crate::{PgSearchError, PgSiftQuery, RowMap, TimeoutStage};

/// A page plus server-declared facet buckets.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct FacetPage<T> {
    pub page: crate::Page<T>,
    pub facets: Vec<FacetResult>,
}

/// Named result wrapper useful when facets are fetched independently.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct FacetResultSet {
    pub facets: Vec<FacetResult>,
}

impl PgSiftQuery {
    /// Executes bounded server-declared facets using the same query and row
    /// authorization scope. Facet fields and limits are validated before a
    /// connection is acquired.
    pub async fn facets<E: SearchEntity>(
        &self,
        query: impl QueryInput<E>,
        scope: &ServerScope<E>,
        requests: &[FacetRequest],
    ) -> Result<Vec<FacetResult>, PgSearchError> {
        if requests.len() > 8 {
            return Err(sift_query_core::ParamError::TooComplex("facet count").into());
        }
        if requests.is_empty() {
            return Ok(Vec::new());
        }
        let plan = self.resolve_plan(query, scope, QueryTarget::Count, "facets")?;
        let prepared = self.prepare_facet_queries(&plan, requests)?;
        let mut output = Vec::with_capacity(prepared.len());
        let deadline = self.deadline::<E>();
        for (prepared, field, value_type) in prepared {
            let connection = self.acquire(deadline).await?;
            output.push(fetch_facet(connection, prepared, field, value_type, deadline).await?);
        }
        Ok(output)
    }

    /// Convenience operation for dynamic list pages with facets.
    pub async fn search_map_with_facets<E: SearchEntity>(
        &self,
        query: impl QueryInput<E> + Clone,
        scope: &ServerScope<E>,
        requests: &[FacetRequest],
    ) -> Result<FacetPage<RowMap>, PgSearchError> {
        let page = self.search_map(query.clone(), scope).await?;
        let facets = self.facets(query, scope, requests).await?;
        Ok(FacetPage { page, facets })
    }

    /// Executes a facet on a caller-owned connection. This is useful when the
    /// caller has opened a read-only transaction and needs the list and facet
    /// queries to observe one snapshot.
    pub async fn facets_on<E: SearchEntity>(
        &self,
        connection: &mut PgConnection,
        query: impl QueryInput<E>,
        scope: &ServerScope<E>,
        requests: &[FacetRequest],
    ) -> Result<Vec<FacetResult>, PgSearchError> {
        if requests.is_empty() {
            return Ok(Vec::new());
        }
        self.validate_external_strategy()?;
        let plan = self.resolve_plan(query, scope, QueryTarget::Count, "facets_on")?;
        let prepared = self.prepare_facet_queries(&plan, requests)?;
        let deadline = self.deadline::<E>();
        let mut output = Vec::with_capacity(prepared.len());
        for (prepared, field, value_type) in prepared {
            output.push(
                fetch_facet_external(connection, prepared, field, value_type, deadline).await?,
            );
        }
        Ok(output)
    }

    /// Executes a map page and its facets on one caller-owned connection.
    /// Begin a read-only transaction before calling this method to obtain a
    /// consistent snapshot for both results.
    pub async fn search_map_with_facets_on<E: SearchEntity>(
        &self,
        connection: &mut PgConnection,
        query: impl QueryInput<E> + Clone,
        scope: &ServerScope<E>,
        requests: &[FacetRequest],
    ) -> Result<FacetPage<RowMap>, PgSearchError> {
        let page = self.search_map_on(connection, query.clone(), scope).await?;
        let facets = self.facets_on(connection, query, scope, requests).await?;
        Ok(FacetPage { page, facets })
    }

    fn prepare_facet_queries<E: SearchEntity>(
        &self,
        plan: &sift_query_core::QueryPlan<E>,
        requests: &[FacetRequest],
    ) -> Result<
        Vec<(
            crate::execution::io::PreparedPgQuery,
            &'static str,
            sift_query_core::ValueType,
        )>,
        PgSearchError,
    > {
        if requests.len() > 8 {
            return Err(sift_query_core::ParamError::TooComplex("facet count").into());
        }
        if requests.is_empty() {
            return Ok(Vec::new());
        }
        if plan.meta.is_grouped() {
            return Err(sift_query_core::ParamError::InvalidValue {
                field: "facet".to_owned(),
                reason: "facets require a non-grouped read model".to_owned(),
            }
            .into());
        }
        let partition = self.extensions.resolve_partition(plan, "facets")?;
        let mut output = Vec::with_capacity(requests.len());
        for request in requests {
            if request.schema_fingerprint() != plan.meta.schema_fingerprint {
                return Err(sift_query_core::ParamError::UnknownField(
                    "facet entity schema mismatch".to_owned(),
                )
                .into());
            }
            if request.limit() == 0 || request.limit() > 1_000 {
                return Err(sift_query_core::ParamError::InvalidValue {
                    field: "facet.limit".to_owned(),
                    reason: "facet limit must be between 1 and 1000".to_owned(),
                }
                .into());
            }
            let field = plan.meta.field(request.field_id()).ok_or_else(|| {
                sift_query_core::ParamError::UnknownField(format!("#{}", request.field_id().0))
            })?;
            plan.validate_field_access([request.field_id()])?;
            if !field.selectable
                || !field.sortable
                || field.kind == sift_query_core::FieldKind::FilterOnly
            {
                return Err(sift_query_core::ParamError::InvalidValue {
                    field: field.public_name.to_owned(),
                    reason: "facet field must be selectable and sortable".to_owned(),
                }
                .into());
            }
            if !field.value_type.supports_cursor() {
                return Err(sift_query_core::ParamError::InvalidValue {
                    field: field.public_name.to_owned(),
                    reason: "facet field type is not orderable".to_owned(),
                }
                .into());
            }
            let rendered = self.renderer.render_facet_with_options(
                plan,
                request.field_id(),
                request.limit(),
                request.includes_null(),
                request.excludes_self_filter(),
            )?;
            let rendered = self.extensions.apply_rendered(
                plan,
                "facets",
                "facet",
                partition.as_deref(),
                rendered,
            )?;
            output.push((prepare(rendered)?, field.public_name, field.value_type));
        }
        Ok(output)
    }
}

async fn fetch_facet(
    connection: sqlx::pool::PoolConnection<sqlx::Postgres>,
    prepared: crate::execution::io::PreparedPgQuery,
    field: &'static str,
    value_type: sift_query_core::ValueType,
    deadline: Instant,
) -> Result<FacetResult, PgSearchError> {
    let mut connection =
        configure_statement_timeout(connection, deadline, TimeoutStage::ClusterQuery).await?;
    let result = timeout_at(
        deadline,
        sqlx::query_with(prepared.sql(), prepared.arguments).fetch_all(&mut *connection),
    )
    .await;
    let Ok(result) = result else {
        crate::execution::io::close_hard(connection).await;
        return Err(PgSearchError::Timeout(TimeoutStage::ClusterQuery));
    };
    let (_connection, rows) =
        finish_statement(connection, result, TimeoutStage::ClusterQuery).await?;
    decode_facet_rows(&rows, field, value_type)
}

async fn fetch_facet_external(
    connection: &mut PgConnection,
    prepared: crate::execution::io::PreparedPgQuery,
    field: &'static str,
    value_type: sift_query_core::ValueType,
    deadline: Instant,
) -> Result<FacetResult, PgSearchError> {
    let result = timeout_at(
        deadline,
        sqlx::query_with(prepared.sql(), prepared.arguments).fetch_all(&mut *connection),
    )
    .await;
    let rows = result.map_err(|_| PgSearchError::Timeout(TimeoutStage::ClusterQuery))??;
    decode_facet_rows(&rows, field, value_type)
}

fn decode_facet_rows(
    rows: &[PgRow],
    field: &'static str,
    value_type: sift_query_core::ValueType,
) -> Result<FacetResult, PgSearchError> {
    let mut buckets = Vec::with_capacity(rows.len());
    let mut represented = 0_i64;
    let mut total = 0_i64;
    for row in rows {
        let value = crate::postgres::decode::decode_row_value(row, "sq_key", value_type)?;
        let count = row.try_get::<i64, _>("sq_count")?;
        total = row.try_get::<i64, _>("sq_total")?;
        represented = represented.saturating_add(count);
        buckets.push(FacetBucket { value, count });
    }
    Ok(FacetResult {
        field,
        buckets,
        other_count: total.saturating_sub(represented).max(0),
    })
}
