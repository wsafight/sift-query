use sift_query_core::{CursorValue, NullOrder, QueryPlan, SearchEntity, SortOrder};

use super::RenderState;
use crate::RenderError;

pub(super) fn render_cursor<E: SearchEntity>(
    plan: &QueryPlan<E>,
    values: &[CursorValue],
    state: &mut RenderState,
) -> Result<String, RenderError> {
    if values.len() != plan.order.len() {
        return Err(RenderError::InvalidCondition(
            "cursor value count does not match ordering".to_owned(),
        ));
    }
    let mut alternatives = Vec::new();
    for (index, (order, value)) in plan.order.iter().copied().zip(values).enumerate() {
        let mut terms = Vec::new();
        for (previous_order, previous_value) in plan.order.iter().zip(values).take(index) {
            let field = plan
                .meta
                .field(previous_order.field)
                .ok_or(RenderError::UnknownField(previous_order.field.0))?;
            terms.push(cursor_equal(
                field.column,
                previous_value,
                E::cursor_key_non_null(previous_order.field),
                state,
            ));
        }
        let field = plan
            .meta
            .field(order.field)
            .ok_or(RenderError::UnknownField(order.field.0))?;
        if E::cursor_key_non_null(order.field) && matches!(value, CursorValue::Null) {
            return Err(RenderError::InvalidCondition(
                "non-null cursor key cannot contain NULL".to_owned(),
            ));
        }
        if let Some(comparison) = cursor_after(
            field.column,
            order,
            value,
            E::cursor_key_non_null(order.field),
            state,
        ) {
            terms.push(comparison);
            alternatives.push(format!("({})", terms.join(" AND ")));
        }
    }
    if alternatives.is_empty() {
        Ok("1 = 0".to_owned())
    } else {
        Ok(format!("({})", alternatives.join(" OR ")))
    }
}

fn cursor_equal(
    column: &str,
    value: &CursorValue,
    non_null: bool,
    state: &mut RenderState,
) -> String {
    match value {
        CursorValue::Null => format!("{column} IS NULL"),
        CursorValue::Scalar(value) => {
            let placeholder = state.bind(value.clone());
            if non_null {
                format!("{column} = {placeholder}")
            } else {
                format!("{column} IS NOT DISTINCT FROM {placeholder}")
            }
        }
    }
}

fn cursor_after(
    column: &str,
    order: sift_query_core::OrderBy,
    value: &CursorValue,
    non_null: bool,
    state: &mut RenderState,
) -> Option<String> {
    if non_null && let CursorValue::Scalar(value) = value {
        let placeholder = state.bind(value.clone());
        let comparison = match order.direction {
            SortOrder::Asc => ">",
            SortOrder::Desc => "<",
        };
        return Some(format!("{column} {comparison} {placeholder}"));
    }
    match (order.direction, order.nulls, value) {
        (SortOrder::Asc | SortOrder::Desc, NullOrder::Last, CursorValue::Null) => None,
        (SortOrder::Asc | SortOrder::Desc, NullOrder::First, CursorValue::Null) => {
            Some(format!("{column} IS NOT NULL"))
        }
        (SortOrder::Asc, NullOrder::Last, CursorValue::Scalar(value)) => {
            let placeholder = state.bind(value.clone());
            Some(format!("({column} > {placeholder} OR {column} IS NULL)"))
        }
        (SortOrder::Desc, NullOrder::Last, CursorValue::Scalar(value)) => {
            let placeholder = state.bind(value.clone());
            Some(format!("({column} < {placeholder} OR {column} IS NULL)"))
        }
        (SortOrder::Asc, NullOrder::First, CursorValue::Scalar(value)) => {
            let placeholder = state.bind(value.clone());
            Some(format!("{column} > {placeholder}"))
        }
        (SortOrder::Desc, NullOrder::First, CursorValue::Scalar(value)) => {
            let placeholder = state.bind(value.clone());
            Some(format!("{column} < {placeholder}"))
        }
    }
}
#[cfg(test)]
mod tests;
