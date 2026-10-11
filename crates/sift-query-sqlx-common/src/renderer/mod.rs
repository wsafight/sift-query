use std::fmt::Write;

use sift_query_algorithm::{FNV1A_64_OFFSET_BASIS, StableHasher64};
use sift_query_core::{
    AggregateFn, FieldKind, PagingMode, QueryPlan, QueryTarget, ScalarValue, SearchEntity,
    SearchOptions, SortOrder, TypedList, ValueType,
};

use crate::template::{RenderRequest, render_cached};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Dialect {
    MySql,
    Sqlite,
}

impl Dialect {
    fn quote(self, identifier: &str) -> String {
        match self {
            Self::MySql => format!("`{}`", identifier.replace('`', "``")),
            Self::Sqlite => format!("\"{}\"", identifier.replace('"', "\"\"")),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RenderedQuery {
    pub sql: String,
    pub binds: Vec<ScalarValue>,
    pub shape_id: u64,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct SummaryValue {
    pub field: &'static str,
    pub function: AggregateFn,
    pub value_type: ValueType,
    pub value: Option<sift_query_core::RowValue>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RenderError {
    #[error("query plan references unknown field {0}")]
    UnknownField(u16),
    #[error("field {0} has no result alias")]
    MissingAlias(String),
    #[error("condition {0} has invalid values")]
    InvalidCondition(String),
    #[error("query target cannot be rendered by this operation")]
    WrongTarget,
}

#[derive(Debug, Clone, Copy)]
pub struct Renderer {
    pub dialect: Dialect,
}

#[allow(clippy::trivially_copy_pass_by_ref, clippy::unused_self)]
impl Renderer {
    #[must_use]
    pub const fn new(dialect: Dialect) -> Self {
        Self { dialect }
    }

    pub fn render_list<E: SearchEntity>(
        &self,
        plan: &QueryPlan<E>,
    ) -> Result<RenderedQuery, RenderError> {
        render_cached(*self, plan, RenderRequest::List, || {
            self.render_list_uncached(plan)
        })
    }

    fn render_list_uncached<E: SearchEntity>(
        &self,
        plan: &QueryPlan<E>,
    ) -> Result<RenderedQuery, RenderError> {
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
        let mut state = State::default();
        let mut sql = String::with_capacity(128);
        sql.push_str("SELECT ");
        if plan.distinct {
            sql.push_str("DISTINCT ");
        }
        sql.push_str(&self.projection(plan, true)?);
        write!(sql, " FROM {}", plan.meta.from).expect("String write");
        self.filters(plan, &mut state, &mut sql, true)?;
        self.group_by(plan, &mut sql);
        self.having(plan, &mut state, &mut sql)?;
        self.order(plan, &mut state, &mut sql)?;
        let limit = state.bind(ScalarValue::I64(i64::from(plan.paging.size)));
        write!(sql, " LIMIT {limit}").expect("String write");
        if !matches!(plan.paging.mode, PagingMode::Cursor(_)) {
            let offset = state.bind(ScalarValue::I64(
                i64::try_from(plan.paging.offset)
                    .map_err(|_| RenderError::InvalidCondition("offset overflow".to_owned()))?,
            ));
            write!(sql, " OFFSET {offset}").expect("String write");
        }
        Ok(state.finish(sql, plan.policy_fingerprint_seed()))
    }

    pub fn render_count<E: SearchEntity>(
        &self,
        plan: &QueryPlan<E>,
    ) -> Result<RenderedQuery, RenderError> {
        render_cached(*self, plan, RenderRequest::Count, || {
            self.render_count_uncached(plan)
        })
    }

    fn render_count_uncached<E: SearchEntity>(
        &self,
        plan: &QueryPlan<E>,
    ) -> Result<RenderedQuery, RenderError> {
        let mut state = State::default();
        let mut sql = String::with_capacity(128);
        if plan.meta.is_grouped() || plan.distinct {
            sql.push_str("SELECT COUNT(*) AS sq_count FROM (SELECT ");
            if plan.distinct {
                sql.push_str("DISTINCT ");
            }
            sql.push_str(&self.projection(plan, false)?);
            write!(sql, " FROM {}) sq_source", plan.meta.from).expect("String write");
            // Rebuild the subquery in the correct order. The temporary suffix is removed below.
            let start = sql.find(" FROM ").expect("FROM");
            let projection_end = sql[start..].find(") sq_source").expect("source") + start;
            let mut body = String::new();
            write!(body, "SELECT ").expect("String write");
            if plan.distinct {
                body.push_str("DISTINCT ");
            }
            body.push_str(&self.projection(plan, false)?);
            write!(body, " FROM {}", plan.meta.from).expect("String write");
            self.filters(plan, &mut state, &mut body, false)?;
            self.group_by(plan, &mut body);
            self.having(plan, &mut state, &mut body)?;
            sql.truncate(start);
            sql.push_str(" FROM (");
            sql.push_str(&body);
            sql.push_str(") sq_source");
            let _ = projection_end;
        } else {
            sql.push_str("SELECT COUNT(*) AS sq_count");
            write!(sql, " FROM {}", plan.meta.from).expect("String write");
            self.filters(plan, &mut state, &mut sql, false)?;
        }
        Ok(state.finish(sql, plan.policy_fingerprint_seed()))
    }

    /// Render the count and optional summary aggregates for a page.
    ///
    /// Grouped and DISTINCT entities use a subquery so the count remains the
    /// number of result groups/rows while summaries are evaluated over that
    /// same result set. All values still flow through the normal bind state.
    pub fn render_cluster<E: SearchEntity>(
        &self,
        plan: &QueryPlan<E>,
        options: Option<&SearchOptions<E>>,
    ) -> Result<RenderedQuery, RenderError> {
        render_cached(*self, plan, RenderRequest::Cluster(options), || {
            self.render_cluster_uncached(plan, options)
        })
    }

    fn render_cluster_uncached<E: SearchEntity>(
        &self,
        plan: &QueryPlan<E>,
        options: Option<&SearchOptions<E>>,
    ) -> Result<RenderedQuery, RenderError> {
        let requests = options.map_or(&[][..], SearchOptions::requests);
        for request in requests {
            plan.validate_field_access([request.field])
                .map_err(|error| RenderError::InvalidCondition(error.to_string()))?;
            if !supports_aggregate(request.function, request.input_type) {
                return Err(RenderError::InvalidCondition(
                    "summary field type does not support the SQL aggregate".to_owned(),
                ));
            }
        }

        let mut state = State::default();
        let mut sql = String::with_capacity(192);
        let count_alias = self.dialect.quote("sq_count");
        let needs_subquery = plan.meta.is_grouped() || plan.distinct;
        if needs_subquery {
            write!(sql, "SELECT COUNT(*) AS {count_alias}").expect("String write");
            for (index, request) in requests.iter().enumerate() {
                let alias = self.dialect.quote(&format!("sq_{index}"));
                write!(
                    sql,
                    ", {} AS {alias}",
                    render_aggregate(request.function, &alias)
                )
                .expect("String write");
            }
            sql.push_str(" FROM (SELECT ");
            if plan.distinct {
                sql.push_str("DISTINCT ");
                // MySQL derived tables require unique column names. Preserve
                // all DISTINCT expressions with separate internal aliases.
                for (index, field_id) in plan.projection.iter().enumerate() {
                    if index > 0 {
                        sql.push_str(", ");
                    }
                    let field = plan
                        .meta
                        .field(*field_id)
                        .ok_or(RenderError::UnknownField(field_id.0))?;
                    let alias = self.dialect.quote(&format!("sq_projection_{index}"));
                    write!(sql, "{} AS {alias}", field.column).expect("String write");
                }
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
                    let alias = self.dialect.quote(&format!("sq_{index}"));
                    write!(sql, ", {} AS {alias}", field.column).expect("String write");
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
                    let alias = self.dialect.quote(&format!("sq_{index}"));
                    write!(sql, "{} AS {alias}", field.column).expect("String write");
                }
            }
            write!(sql, " FROM {}", plan.meta.from).expect("String write");
            self.filters(plan, &mut state, &mut sql, false)?;
            self.group_by(plan, &mut sql);
            self.having(plan, &mut state, &mut sql)?;
            sql.push_str(") sq_cluster");
        } else {
            write!(sql, "SELECT COUNT(*) AS {count_alias}").expect("String write");
            for (index, request) in requests.iter().enumerate() {
                let field = plan
                    .meta
                    .field(request.field)
                    .ok_or(RenderError::UnknownField(request.field.0))?;
                let alias = self.dialect.quote(&format!("sq_{index}"));
                write!(
                    sql,
                    ", {} AS {alias}",
                    render_aggregate(request.function, field.column)
                )
                .expect("String write");
            }
            write!(sql, " FROM {}", plan.meta.from).expect("String write");
            self.filters(plan, &mut state, &mut sql, false)?;
        }
        Ok(state.finish(sql, plan.policy_fingerprint_seed()))
    }

    fn projection<E: SearchEntity>(
        &self,
        plan: &QueryPlan<E>,
        aliases: bool,
    ) -> Result<String, RenderError> {
        let mut out = String::new();
        for (index, field_id) in plan.projection.iter().enumerate() {
            if index > 0 {
                out.push_str(", ");
            }
            let field = plan
                .meta
                .field(*field_id)
                .ok_or(RenderError::UnknownField(field_id.0))?;
            out.push_str(field.column);
            if aliases {
                let alias = plan
                    .projection_aliases
                    .as_ref()
                    .and_then(|values| values.get(index).copied())
                    .or(field.result_alias)
                    .ok_or_else(|| RenderError::MissingAlias(field.public_name.to_owned()))?;
                write!(out, " AS {}", self.dialect.quote(alias)).expect("String write");
            }
        }
        if out.is_empty() {
            return Err(RenderError::InvalidCondition("empty projection".to_owned()));
        }
        Ok(out)
    }

    fn filters<E: SearchEntity>(
        &self,
        plan: &QueryPlan<E>,
        state: &mut State,
        sql: &mut String,
        include_cursor: bool,
    ) -> Result<(), RenderError> {
        let mut has_predicate = false;
        if let Some(base) = plan.meta.base_where {
            sql.push_str(" WHERE ");
            sql.push_str(base);
            has_predicate = true;
        }
        if let Some(value) = render_arena(plan.meta, &plan.server_predicate, state, self.dialect)? {
            if has_predicate {
                sql.push_str(" AND ");
            } else {
                sql.push_str(" WHERE ");
                has_predicate = true;
            }
            sql.push_str(&value);
        }
        if let Some(value) = render_arena(plan.meta, &plan.row_predicate, state, self.dialect)? {
            if has_predicate {
                sql.push_str(" AND ");
            } else {
                sql.push_str(" WHERE ");
                has_predicate = true;
            }
            sql.push_str(&value);
        }
        if include_cursor && let PagingMode::Cursor(Some(cursor)) = &plan.paging.mode {
            if has_predicate {
                sql.push_str(" AND ");
            } else {
                sql.push_str(" WHERE ");
            }
            sql.push_str(&render_cursor(plan, cursor.values(), state)?);
        }
        Ok(())
    }

    fn group_by<E: SearchEntity>(&self, plan: &QueryPlan<E>, sql: &mut String) {
        let mut first = true;
        for field in plan
            .meta
            .fields
            .iter()
            .filter(|field| field.kind == FieldKind::GroupKey)
        {
            sql.push_str(if first { " GROUP BY " } else { ", " });
            sql.push_str(field.column);
            first = false;
        }
    }

    fn having<E: SearchEntity>(
        &self,
        plan: &QueryPlan<E>,
        state: &mut State,
        sql: &mut String,
    ) -> Result<(), RenderError> {
        let mut has_predicate = false;
        if let Some(base) = plan.meta.fixed_having {
            sql.push_str(" HAVING ");
            sql.push_str(base);
            has_predicate = true;
        }
        if let Some(value) =
            render_arena(plan.meta, &plan.server_group_predicate, state, self.dialect)?
        {
            if has_predicate {
                sql.push_str(" AND ");
            } else {
                sql.push_str(" HAVING ");
                has_predicate = true;
            }
            sql.push_str(&value);
        }
        if let Some(value) = render_arena(plan.meta, &plan.group_predicate, state, self.dialect)? {
            if has_predicate {
                sql.push_str(" AND ");
            } else {
                sql.push_str(" HAVING ");
            }
            sql.push_str(&value);
        }
        Ok(())
    }

    fn order<E: SearchEntity>(
        &self,
        plan: &QueryPlan<E>,
        state: &mut State,
        sql: &mut String,
    ) -> Result<(), RenderError> {
        if plan.order.is_empty() {
            return Ok(());
        }
        sql.push_str(" ORDER BY ");
        for (index, order) in plan.order.iter().enumerate() {
            if index > 0 {
                sql.push_str(", ");
            }
            let field = plan
                .meta
                .field(order.field)
                .ok_or(RenderError::UnknownField(order.field.0))?;
            if let Some(config) = E::condition_order(order.field) {
                let value = plan
                    .condition_order_value(order.field)
                    .map_err(|error| RenderError::InvalidCondition(error.to_string()))?
                    .ok_or_else(|| {
                        RenderError::InvalidCondition("missing order input".to_owned())
                    })?;
                sql.push_str(
                    &config
                        .expression
                        .template()
                        .replace("{field}", field.column)
                        .replace("{0}", state.bind(value.clone())),
                );
            } else {
                sql.push_str(field.column);
            }
            if matches!(order.direction, SortOrder::Asc) {
                sql.push_str(" IS NULL ASC, ");
                sql.push_str(field.column);
            }
            sql.push_str(match order.direction {
                SortOrder::Asc => " ASC",
                SortOrder::Desc => " DESC",
            });
        }
        Ok(())
    }
}

#[derive(Default)]
struct State {
    binds: Vec<ScalarValue>,
}

impl State {
    fn bind(&mut self, value: ScalarValue) -> &'static str {
        self.binds.push(value);
        "?"
    }

    fn bind_list(&mut self, list: TypedList) -> String {
        let count = list.values.len();
        self.binds.extend(list.values);
        let mut placeholders = String::with_capacity(count.saturating_mul(3));
        for index in 0..count {
            if index > 0 {
                placeholders.push_str(", ");
            }
            placeholders.push('?');
        }
        placeholders
    }

    fn finish(self, sql: String, seed: u64) -> RenderedQuery {
        let mut hasher = StableHasher64::new(seed ^ FNV1A_64_OFFSET_BASIS);
        hasher.write(&sql);
        RenderedQuery {
            sql,
            binds: self.binds,
            shape_id: hasher.finish(),
        }
    }
}

fn render_aggregate(function: AggregateFn, field: &str) -> String {
    match function {
        AggregateFn::Sum => format!("SUM({field})"),
        AggregateFn::Avg => format!("AVG({field})"),
        AggregateFn::Min => format!("MIN({field})"),
        AggregateFn::Max => format!("MAX({field})"),
        AggregateFn::Custom(aggregate) => aggregate.template().replace("{field}", field),
    }
}

fn supports_aggregate(function: AggregateFn, value_type: ValueType) -> bool {
    match function {
        AggregateFn::Sum | AggregateFn::Avg => {
            if matches!(
                value_type,
                ValueType::I32 | ValueType::I64 | ValueType::F32 | ValueType::F64
            ) {
                return true;
            }
            #[cfg(feature = "decimal")]
            if matches!(value_type, ValueType::Decimal) {
                return true;
            }
            false
        }
        AggregateFn::Min | AggregateFn::Max => {
            if matches!(
                value_type,
                ValueType::Bool
                    | ValueType::I32
                    | ValueType::I64
                    | ValueType::F32
                    | ValueType::F64
                    | ValueType::Text
            ) {
                return true;
            }
            #[cfg(feature = "decimal")]
            if matches!(value_type, ValueType::Decimal) {
                return true;
            }
            #[cfg(feature = "chrono")]
            if matches!(
                value_type,
                ValueType::Date | ValueType::Time | ValueType::DateTime | ValueType::Timestamp
            ) {
                return true;
            }
            #[cfg(feature = "uuid")]
            if matches!(value_type, ValueType::Uuid) {
                return true;
            }
            false
        }
        AggregateFn::Custom(_) => true,
    }
}

/// Returns the SQL driver's result type for a summary request.
pub fn summary_output_type(
    function: AggregateFn,
    input: ValueType,
) -> Result<ValueType, RenderError> {
    let output = match function {
        AggregateFn::Sum => match input {
            ValueType::I32 | ValueType::I64 => ValueType::I64,
            ValueType::F32 | ValueType::F64 => ValueType::F64,
            #[cfg(feature = "decimal")]
            ValueType::Decimal => ValueType::Decimal,
            _ => {
                return Err(RenderError::InvalidCondition(
                    "field type does not support SUM".to_owned(),
                ));
            }
        },
        AggregateFn::Avg => match input {
            ValueType::I32 | ValueType::I64 | ValueType::F32 | ValueType::F64 => ValueType::F64,
            #[cfg(feature = "decimal")]
            ValueType::Decimal => ValueType::Decimal,
            _ => {
                return Err(RenderError::InvalidCondition(
                    "field type does not support AVG".to_owned(),
                ));
            }
        },
        AggregateFn::Min | AggregateFn::Max => input,
        AggregateFn::Custom(aggregate) => aggregate.output_type(),
    };
    Ok(output)
}

mod cursor;
mod predicate;
pub(crate) use predicate::like_pattern;

use cursor::render_cursor;
use predicate::render_arena;

trait PlanFingerprint {
    fn policy_fingerprint_seed(&self) -> u64;
}

impl<E: SearchEntity> PlanFingerprint for QueryPlan<E> {
    fn policy_fingerprint_seed(&self) -> u64 {
        // QueryPlan deliberately keeps its policy seed private. Entity metadata is
        // still a stable shape namespace for portable backends.
        self.meta.schema_fingerprint
    }
}
