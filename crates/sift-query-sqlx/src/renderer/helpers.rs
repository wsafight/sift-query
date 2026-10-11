use std::fmt::Write;

use sift_query_core::{
    FieldId, FieldKind, PagingMode, QueryPlan, ScalarValue, SearchEntity, SortOrder, TypedList,
    ValueType,
};

use super::{
    PgBindValue, QueryShapeId, RenderError, RenderedPgQuery, render_arena, render_cursor, types,
};

pub(super) fn is_time_value_type(value_type: ValueType) -> bool {
    match value_type {
        #[cfg(feature = "chrono")]
        ValueType::Date | ValueType::DateTime | ValueType::Timestamp => true,
        _ => false,
    }
}

pub(super) fn render_order<E: SearchEntity>(
    plan: &QueryPlan<E>,
    state: &mut RenderState,
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
                .ok_or_else(|| RenderError::InvalidCondition("missing order input".to_owned()))?;
            let placeholder = format!(
                "{}::{}",
                state.bind(value.clone()),
                types::bind_type(value.value_type())?
            );
            sql.push_str(
                &config
                    .expression
                    .template()
                    .replace("{field}", field.column)
                    .replace("{0}", &placeholder),
            );
        } else {
            sql.push_str(field.column);
        }
        sql.push_str(match order.direction {
            SortOrder::Asc => " ASC NULLS LAST",
            SortOrder::Desc => " DESC NULLS FIRST",
        });
    }
    Ok(())
}

#[derive(Default)]
pub(super) struct RenderState {
    binds: Vec<PgBindValue>,
}

impl RenderState {
    pub(super) fn bind(&mut self, value: ScalarValue) -> String {
        self.binds.push(PgBindValue::Scalar(value));
        format!("${}", self.binds.len())
    }

    pub(super) fn bind_array(&mut self, value: TypedList) -> String {
        self.binds.push(PgBindValue::Array(value));
        format!("${}", self.binds.len())
    }

    pub(super) fn finish(self, sql: String, schema_fingerprint: u64) -> RenderedPgQuery {
        RenderedPgQuery {
            shape_id: QueryShapeId(super::hash_shape(schema_fingerprint, &sql)),
            sql: sql.into(),
            binds: self.binds,
        }
    }
}

pub(super) fn render_projection<E: SearchEntity>(
    plan: &QueryPlan<E>,
    aliases: bool,
) -> Result<String, RenderError> {
    let mut output = String::with_capacity(plan.projection.len() * 48);
    for (index, field_id) in plan.projection.iter().enumerate() {
        if index > 0 {
            output.push_str(", ");
        }
        let field = plan
            .meta
            .field(*field_id)
            .ok_or(RenderError::UnknownField(field_id.0))?;
        output.push_str(field.column);
        if aliases {
            let alias = plan
                .projection_aliases
                .as_ref()
                .and_then(|aliases| aliases.get(index).copied())
                .or(field.result_alias)
                .ok_or_else(|| RenderError::MissingAlias(field.public_name.to_owned()))?;
            write!(output, " AS {}", quote_ident(alias)).expect("writing to String cannot fail");
        }
    }
    if output.is_empty() {
        return Err(RenderError::InvalidCondition("empty projection".to_owned()));
    }
    Ok(output)
}

pub(super) fn render_filters<E: SearchEntity>(
    plan: &QueryPlan<E>,
    state: &mut RenderState,
    sql: &mut String,
    include_cursor: bool,
) -> Result<(), RenderError> {
    render_filters_excluding(plan, state, sql, include_cursor, None)
}

pub(super) fn render_filters_excluding<E: SearchEntity>(
    plan: &QueryPlan<E>,
    state: &mut RenderState,
    sql: &mut String,
    include_cursor: bool,
    exclude_field: Option<FieldId>,
) -> Result<(), RenderError> {
    let mut predicates = Vec::new();
    if let Some(base) = plan.meta.base_where {
        predicates.push(base.to_owned());
    }
    if let Some(rendered) = render_arena(plan.meta, &plan.server_predicate, state)? {
        predicates.push(rendered);
    }
    let filtered_row;
    let row_predicate = if let Some(field) = exclude_field {
        filtered_row = plan.row_predicate.without_field(field);
        &filtered_row
    } else {
        &plan.row_predicate
    };
    if let Some(rendered) = render_arena(plan.meta, row_predicate, state)? {
        predicates.push(rendered);
    }
    if include_cursor && let PagingMode::Cursor(Some(cursor)) = &plan.paging.mode {
        predicates.push(render_cursor(plan, cursor.values(), state)?);
    }
    if !predicates.is_empty() {
        sql.push_str(" WHERE ");
        sql.push_str(&predicates.join(" AND "));
    }
    Ok(())
}

pub(super) fn render_group_by<E: SearchEntity>(plan: &QueryPlan<E>, sql: &mut String) {
    for (index, field) in plan
        .meta
        .fields
        .iter()
        .filter(|field| field.kind == FieldKind::GroupKey)
        .enumerate()
    {
        if index == 0 {
            sql.push_str(" GROUP BY ");
        } else {
            sql.push_str(", ");
        }
        sql.push_str(field.column);
    }
}

pub(super) fn sql_buffer<E: SearchEntity>(plan: &QueryPlan<E>) -> String {
    let nodes = plan.server_predicate.nodes().len()
        + plan.row_predicate.nodes().len()
        + plan.server_group_predicate.nodes().len()
        + plan.group_predicate.nodes().len();
    String::with_capacity(
        128 + plan.meta.from.len()
            + plan.projection.len() * 48
            + nodes * 48
            + plan.order.len() * 40,
    )
}

pub(super) fn render_having<E: SearchEntity>(
    plan: &QueryPlan<E>,
    state: &mut RenderState,
    sql: &mut String,
) -> Result<(), RenderError> {
    let mut predicates = Vec::new();
    if let Some(fixed) = plan.meta.fixed_having {
        predicates.push(fixed.to_owned());
    }
    if let Some(rendered) = render_arena(plan.meta, &plan.server_group_predicate, state)? {
        predicates.push(rendered);
    }
    if let Some(rendered) = render_arena(plan.meta, &plan.group_predicate, state)? {
        predicates.push(rendered);
    }
    if !predicates.is_empty() {
        sql.push_str(" HAVING ");
        sql.push_str(&predicates.join(" AND "));
    }
    Ok(())
}

pub(super) fn quote_ident(input: &str) -> String {
    format!("\"{}\"", input.replace('"', "\"\""))
}
