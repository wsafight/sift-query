use sift_query_core::{QueryPlan, SearchEntity, SortOrder};

use super::{RenderError, State};

pub(super) fn render_cursor<E: SearchEntity>(
    plan: &QueryPlan<E>,
    values: &[sift_query_core::CursorValue],
    state: &mut State,
) -> Result<String, RenderError> {
    if values.len() != plan.order.len() {
        return Err(RenderError::InvalidCondition(
            "cursor value count does not match ordering".to_owned(),
        ));
    }
    let mut alternatives = Vec::new();
    for (index, (order, value)) in plan.order.iter().copied().zip(values).enumerate() {
        let mut terms = Vec::new();
        for (previous, previous_value) in plan.order.iter().zip(values).take(index) {
            let field = plan
                .meta
                .field(previous.field)
                .ok_or(RenderError::UnknownField(previous.field.0))?;
            terms.push(match previous_value {
                sift_query_core::CursorValue::Null => format!("{} IS NULL", field.column),
                sift_query_core::CursorValue::Scalar(value) => {
                    format!("{} = {}", field.column, state.bind(value.clone()))
                }
            });
        }
        let field = plan
            .meta
            .field(order.field)
            .ok_or(RenderError::UnknownField(order.field.0))?;
        let comparison = match (order.direction, order.nulls, value) {
            (_, _, sift_query_core::CursorValue::Null) => match order.nulls {
                sift_query_core::NullOrder::Last => None,
                sift_query_core::NullOrder::First => Some(format!("{} IS NOT NULL", field.column)),
            },
            (
                SortOrder::Asc,
                sift_query_core::NullOrder::Last,
                sift_query_core::CursorValue::Scalar(value),
            ) => Some(format!(
                "({} > {} OR {} IS NULL)",
                field.column,
                state.bind(value.clone()),
                field.column
            )),
            (
                SortOrder::Desc,
                sift_query_core::NullOrder::Last,
                sift_query_core::CursorValue::Scalar(value),
            ) => Some(format!(
                "({} < {} OR {} IS NULL)",
                field.column,
                state.bind(value.clone()),
                field.column
            )),
            (SortOrder::Asc, _, sift_query_core::CursorValue::Scalar(value)) => {
                Some(format!("{} > {}", field.column, state.bind(value.clone())))
            }
            (SortOrder::Desc, _, sift_query_core::CursorValue::Scalar(value)) => {
                Some(format!("{} < {}", field.column, state.bind(value.clone())))
            }
        };
        if let Some(comparison) = comparison {
            terms.push(comparison);
            alternatives.push(format!("({})", terms.join(" AND ")));
        }
    }
    Ok(if alternatives.is_empty() {
        "1 = 0".to_owned()
    } else {
        format!("({})", alternatives.join(" OR "))
    })
}
