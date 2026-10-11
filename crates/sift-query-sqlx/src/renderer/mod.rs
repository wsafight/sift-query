use std::fmt::Write;
use std::sync::Arc;

use sift_query_algorithm::{FNV1A_64_OFFSET_BASIS, StableHasher64};
use sift_query_core::{
    BucketRequest, FieldId, FieldKind, PagingMode, QueryPlan, QueryTarget, ScalarValue,
    SearchEntity, SearchOptions, TypedList, ValueType,
};

use crate::RenderError;

mod cursor;
pub(crate) mod predicate;
mod types;
mod window;

use cursor::render_cursor;
use predicate::render_arena;
use types::{render_aggregate, supports_aggregate};

#[derive(Debug, Clone, PartialEq)]
pub enum PgBindValue {
    Scalar(ScalarValue),
    Array(TypedList),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct QueryShapeId(pub u64);

#[derive(Debug, Clone, PartialEq)]
pub struct RenderedPgQuery {
    pub sql: Arc<str>,
    pub binds: Vec<PgBindValue>,
    pub shape_id: QueryShapeId,
}

#[derive(Debug, Clone, Default)]
pub struct PgRenderer;

impl PgRenderer {
    /// Renders a bounded time or numeric bucket aggregation using only a
    /// server-declared bucket request.
    pub fn render_bucket<E: SearchEntity>(
        &self,
        plan: &QueryPlan<E>,
        request: &BucketRequest<E>,
    ) -> Result<RenderedPgQuery, RenderError> {
        let field_id = request.field_id();
        plan.validate_field_access([field_id])
            .map_err(|error| RenderError::InvalidCondition(error.to_string()))?;
        let field = plan
            .meta
            .field(field_id)
            .ok_or(RenderError::UnknownField(field_id.0))?;
        if !field.selectable || field.kind == FieldKind::FilterOnly {
            return Err(RenderError::InvalidCondition(
                "bucket field must be selectable".to_owned(),
            ));
        }
        let mut state = RenderState::default();
        let mut sql = sql_buffer(plan);
        match request {
            BucketRequest::Time(request) => {
                if !is_time_value_type(field.value_type) {
                    return Err(RenderError::InvalidCondition(
                        "time buckets require a date/time field".to_owned(),
                    ));
                }
                let unit = request.unit().sql_name();
                let timezone = request.timezone();
                let null_label = state.bind(ScalarValue::Text("null".to_owned()));
                write!(
                    sql,
                    "SELECT CASE WHEN {field} IS NULL THEN {null_label} ELSE (date_trunc('{unit}', {field} AT TIME ZONE '{timezone}'))::text END AS \"sq_bucket\", count(*)::bigint AS \"sq_count\" FROM {from}",
                    field = field.column,
                    null_label = null_label,
                    unit = unit,
                    timezone = timezone,
                    from = plan.meta.from
                )
                .expect("writing to String cannot fail");
            }
            BucketRequest::Numeric(request) => {
                if !matches!(
                    field.value_type,
                    ValueType::I32 | ValueType::I64 | ValueType::F32 | ValueType::F64
                ) || request
                    .boundaries()
                    .iter()
                    .any(|value| value.value_type() != field.value_type)
                {
                    return Err(RenderError::InvalidCondition(
                        "numeric buckets require matching numeric boundaries".to_owned(),
                    ));
                }
                sql.push_str("SELECT CASE WHEN ");
                sql.push_str(field.column);
                sql.push_str(" IS NULL THEN ");
                sql.push_str(&state.bind(ScalarValue::Text("null".to_owned())));
                for (index, boundary) in request.boundaries().iter().enumerate() {
                    write!(
                        sql,
                        " WHEN {} < {} THEN {}",
                        field.column,
                        state.bind(boundary.clone()),
                        state.bind(ScalarValue::Text(sift_query_core::numeric_bucket_label(
                            request.boundaries(),
                            index
                        )))
                    )
                    .expect("writing to String cannot fail");
                }
                write!(
                    sql,
                    " ELSE {} END AS \"sq_bucket\", count(*)::bigint AS \"sq_count\" FROM {}",
                    state.bind(ScalarValue::Text(sift_query_core::numeric_bucket_label(
                        request.boundaries(),
                        request.boundaries().len()
                    ))),
                    plan.meta.from
                )
                .expect("writing to String cannot fail");
            }
        }
        render_filters(plan, &mut state, &mut sql, false)?;
        match request {
            BucketRequest::Time(_) => sql.push_str(" GROUP BY sq_bucket ORDER BY sq_bucket ASC"),
            BucketRequest::Numeric(_) => {
                write!(
                    sql,
                    " GROUP BY sq_bucket ORDER BY min({}) ASC NULLS LAST",
                    field.column
                )
                .expect("writing to String cannot fail");
            }
        }
        Ok(state.finish(sql, plan.shape_fingerprint_seed()))
    }

    /// Renders a bounded, server-declared facet query over a row-phase field.
    pub fn render_facet<E: SearchEntity>(
        &self,
        plan: &QueryPlan<E>,
        field_id: FieldId,
        limit: u32,
        include_null: bool,
    ) -> Result<RenderedPgQuery, RenderError> {
        self.render_facet_with_options(plan, field_id, limit, include_null, false)
    }

    pub fn render_facet_with_options<E: SearchEntity>(
        &self,
        plan: &QueryPlan<E>,
        field_id: FieldId,
        limit: u32,
        include_null: bool,
        exclude_self_filter: bool,
    ) -> Result<RenderedPgQuery, RenderError> {
        let field = plan
            .meta
            .field(field_id)
            .ok_or(RenderError::UnknownField(field_id.0))?;
        if plan.meta.is_grouped() {
            return Err(RenderError::InvalidCondition(
                "facets require a row-phase entity".to_owned(),
            ));
        }
        let mut state = RenderState::default();
        let mut sql = sql_buffer(plan);
        if plan.distinct {
            write!(
                sql,
                "SELECT sq_facet AS \"sq_key\", count(*)::bigint AS \"sq_count\", sum(count(*)) OVER ()::bigint AS \"sq_total\" FROM (SELECT DISTINCT {}, {} AS sq_facet FROM {}",
                render_projection(plan, false)?,
                field.column,
                plan.meta.from
            )
            .expect("writing to String cannot fail");
            let filter_start = sql.len();
            render_filters_excluding(
                plan,
                &mut state,
                &mut sql,
                false,
                exclude_self_filter.then_some(field_id),
            )?;
            if !include_null {
                if sql.len() > filter_start {
                    sql.push_str(" AND ");
                } else {
                    sql.push_str(" WHERE ");
                }
                write!(sql, "{} IS NOT NULL", field.column).expect("writing to String cannot fail");
            }
            sql.push_str(
                ") sq_facet_source GROUP BY sq_facet ORDER BY count(*) DESC, sq_facet ASC LIMIT ",
            );
            // The LIMIT placeholder is appended through the normal bind state
            // so the statement remains fully parameterized.
            let placeholder = state.bind(ScalarValue::I64(i64::from(limit)));
            sql.push_str(&placeholder);
            return Ok(state.finish(sql, plan.shape_fingerprint_seed()));
        }
        write!(
            sql,
            "SELECT {} AS \"sq_key\", count(*)::bigint AS \"sq_count\", sum(count(*)) OVER ()::bigint AS \"sq_total\" FROM {}",
            field.column, plan.meta.from
        )
        .expect("writing to String cannot fail");
        let filter_start = sql.len();
        render_filters_excluding(
            plan,
            &mut state,
            &mut sql,
            false,
            exclude_self_filter.then_some(field_id),
        )?;
        if !include_null {
            if sql.len() == filter_start {
                sql.push_str(" WHERE ");
            } else {
                sql.push_str(" AND ");
            }
            write!(sql, "{} IS NOT NULL", field.column).expect("writing to String cannot fail");
        }
        write!(
            sql,
            " GROUP BY {} ORDER BY count(*) DESC, {} ASC LIMIT {}",
            field.column,
            field.column,
            state.bind(ScalarValue::I64(i64::from(limit)))
        )
        .expect("writing to String cannot fail");
        Ok(state.finish(sql, plan.shape_fingerprint_seed()))
    }

    pub fn render_list<E: SearchEntity>(
        &self,
        plan: &QueryPlan<E>,
    ) -> Result<RenderedPgQuery, RenderError> {
        if !matches!(
            plan.target,
            QueryTarget::TypedPage
                | QueryTarget::MapPage
                | QueryTarget::TypedList
                | QueryTarget::First
                | QueryTarget::StaticPage
        ) {
            return Err(RenderError::WrongTarget);
        }
        let mut state = RenderState::default();
        let mut sql = sql_buffer(plan);
        sql.push_str("SELECT ");
        if plan.distinct {
            sql.push_str("DISTINCT ");
        }
        sql.push_str(&render_projection(plan, true)?);
        write!(sql, " FROM {}", plan.meta.from).expect("writing to String cannot fail");
        render_filters(plan, &mut state, &mut sql, true)?;
        render_group_by(plan, &mut sql);
        render_having(plan, &mut state, &mut sql)?;
        render_order(plan, &mut state, &mut sql)?;
        let limit = state.bind(ScalarValue::I64(i64::from(plan.paging.size)));
        write!(sql, " LIMIT {limit}").expect("writing to String cannot fail");
        if !matches!(plan.paging.mode, PagingMode::Cursor(_)) {
            let offset = state.bind(ScalarValue::I64(
                i64::try_from(plan.paging.offset)
                    .map_err(|_| RenderError::InvalidCondition("offset overflow".to_owned()))?,
            ));
            write!(sql, " OFFSET {offset}").expect("writing to String cannot fail");
        }
        Ok(state.finish(sql, plan.shape_fingerprint_seed()))
    }

    pub fn render_count<E: SearchEntity>(
        &self,
        plan: &QueryPlan<E>,
    ) -> Result<RenderedPgQuery, RenderError> {
        self.render_cluster(plan, None)
    }

    pub fn render_cluster<E: SearchEntity>(
        &self,
        plan: &QueryPlan<E>,
        options: Option<&SearchOptions<E>>,
    ) -> Result<RenderedPgQuery, RenderError> {
        let requests = options.map_or(&[][..], SearchOptions::requests);
        for request in requests {
            plan.validate_field_access([request.field])
                .map_err(|error| RenderError::InvalidCondition(error.to_string()))?;
            if !supports_aggregate(request.function, request.input_type) {
                return Err(RenderError::InvalidCondition(
                    "summary field type does not support the PostgreSQL aggregate".to_owned(),
                ));
            }
        }
        let needs_subquery = plan.meta.is_grouped() || plan.distinct;
        let mut state = RenderState::default();
        let mut sql = sql_buffer(plan);
        if needs_subquery {
            sql.push_str("SELECT count(*) AS \"sq_count\"");
            for (index, request) in requests.iter().enumerate() {
                write!(
                    sql,
                    ", {} AS \"sq_{index}\"",
                    render_aggregate(request.function, &format!("sq_{index}"))
                )
                .expect("writing to String cannot fail");
            }
            sql.push_str(" FROM (SELECT ");
            if plan.distinct {
                sql.push_str("DISTINCT ");
                sql.push_str(&render_projection(plan, false)?);
                for (index, request) in requests.iter().enumerate() {
                    if !plan.projection.contains(&request.field) {
                        return Err(RenderError::InvalidCondition(
                            "DISTINCT summaries require the summarized field in the final projection"
                                .to_owned(),
                        ));
                    }
                    let field = plan
                        .meta
                        .field(request.field)
                        .ok_or(RenderError::UnknownField(request.field.0))?;
                    write!(sql, ", {} AS \"sq_{index}\"", field.column)
                        .expect("writing to String cannot fail");
                }
            } else if requests.is_empty() {
                sql.push('1');
            } else {
                for (index, request) in requests.iter().enumerate() {
                    if index > 0 {
                        sql.push_str(", ");
                    }
                    let field = plan
                        .meta
                        .field(request.field)
                        .ok_or(RenderError::UnknownField(request.field.0))?;
                    write!(sql, "{} AS \"sq_{index}\"", field.column)
                        .expect("writing to String cannot fail");
                }
            }
            write!(sql, " FROM {}", plan.meta.from).expect("writing to String cannot fail");
            render_filters(plan, &mut state, &mut sql, false)?;
            render_group_by(plan, &mut sql);
            render_having(plan, &mut state, &mut sql)?;
            sql.push_str(") sq_cluster");
        } else {
            sql.push_str("SELECT count(*) AS \"sq_count\"");
            for (index, request) in requests.iter().enumerate() {
                let field = plan
                    .meta
                    .field(request.field)
                    .ok_or(RenderError::UnknownField(request.field.0))?;
                write!(
                    sql,
                    ", {} AS \"sq_{index}\"",
                    render_aggregate(request.function, field.column)
                )
                .expect("writing to String cannot fail");
            }
            write!(sql, " FROM {}", plan.meta.from).expect("writing to String cannot fail");
            render_filters(plan, &mut state, &mut sql, false)?;
        }
        Ok(state.finish(sql, plan.shape_fingerprint_seed()))
    }

    pub fn render_sum<E: SearchEntity>(
        &self,
        plan: &QueryPlan<E>,
        field_id: FieldId,
    ) -> Result<RenderedPgQuery, RenderError> {
        let field = plan
            .meta
            .field(field_id)
            .ok_or(RenderError::UnknownField(field_id.0))?;
        plan.validate_field_access([field_id])
            .map_err(|error| RenderError::InvalidCondition(error.to_string()))?;
        let mut state = RenderState::default();
        let mut sql = sql_buffer(plan);
        if plan.distinct {
            let alias = field
                .result_alias
                .ok_or_else(|| RenderError::MissingAlias(field.public_name.to_owned()))?;
            if !plan.projection.contains(&field_id) {
                return Err(RenderError::InvalidCondition(
                    "SUM over DISTINCT entities requires the field in the final projection"
                        .to_owned(),
                ));
            }
            write!(
                sql,
                "SELECT sum({}) AS \"sq_sum\" FROM (SELECT DISTINCT {} FROM {}",
                quote_ident(alias),
                render_projection(plan, true)?,
                plan.meta.from
            )
            .expect("writing to String cannot fail");
            render_filters(plan, &mut state, &mut sql, false)?;
            render_group_by(plan, &mut sql);
            render_having(plan, &mut state, &mut sql)?;
            sql.push_str(") sq_sum_source");
        } else if plan.meta.is_grouped() {
            sql.push_str("SELECT sum(\"sq_value\") AS \"sq_sum\" FROM (SELECT ");
            write!(
                sql,
                "{} AS \"sq_value\" FROM {}",
                field.column, plan.meta.from
            )
            .expect("writing to String cannot fail");
            render_filters(plan, &mut state, &mut sql, false)?;
            render_group_by(plan, &mut sql);
            render_having(plan, &mut state, &mut sql)?;
            sql.push_str(") sq_sum_source");
        } else {
            write!(
                sql,
                "SELECT sum({}) AS \"sq_sum\" FROM {}",
                field.column, plan.meta.from
            )
            .expect("writing to String cannot fail");
            render_filters(plan, &mut state, &mut sql, false)?;
        }
        Ok(state.finish(sql, plan.shape_fingerprint_seed()))
    }
}

mod helpers;

use helpers::{
    RenderState, is_time_value_type, quote_ident, render_filters, render_filters_excluding,
    render_group_by, render_having, render_order, render_projection, sql_buffer,
};

pub(crate) fn hash_shape(schema_fingerprint: u64, sql: &str) -> u64 {
    let mut hash = StableHasher64::new(schema_fingerprint ^ FNV1A_64_OFFSET_BASIS);
    hash.write(sql);
    hash.finish()
}
#[cfg(test)]
mod tests;
