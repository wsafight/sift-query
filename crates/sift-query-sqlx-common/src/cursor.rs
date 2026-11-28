use sift_query_core::{CursorValue, PagingMode, ParamError, QueryPlan, QueryPolicy, SearchEntity};

use crate::{RowMap, SearchError};

/// Validates cursor paging mode and bumps `plan.paging.size` by 1 to probe `has_more`.
pub fn cursor_page_size<E: SearchEntity>(plan: &mut QueryPlan<E>) -> Result<u32, SearchError> {
    if !matches!(plan.paging.mode, PagingMode::Cursor(_)) {
        return Err(ParamError::InvalidValue {
            field: "cursor".to_owned(),
            reason: "cursor page API requires cursor paging mode".to_owned(),
        }
        .into());
    }
    let size = plan.paging.size;
    if size > 0 {
        plan.paging.size = size.checked_add(1).ok_or_else(|| {
            ParamError::PagingOutOfRange("cursor page size is too large".to_owned())
        })?;
    }
    Ok(size)
}

/// Encodes the next cursor token from the last entity row when cursor paging is active.
pub fn entity_cursor<E: SearchEntity>(
    policy: &QueryPolicy,
    plan: &QueryPlan<E>,
    row: Option<&E>,
) -> Result<Option<String>, SearchError> {
    if !matches!(plan.paging.mode, PagingMode::Cursor(_)) || plan.paging.size == 0 {
        return Ok(None);
    }
    let Some(row) = row else {
        return Ok(None);
    };
    let codec = policy
        .cursor_codec()
        .ok_or_else(|| ParamError::InvalidValue {
            field: policy.names().cursor.clone(),
            reason: "cursor pagination is not configured".to_owned(),
        })?;
    let values = plan
        .order
        .iter()
        .map(|order| {
            row.cursor_value(order.field)
                .ok_or_else(|| ParamError::InvalidValue {
                    field: policy.names().cursor.clone(),
                    reason: "entity cannot extract a cursor ordering value".to_owned(),
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Some(codec.encode::<E>(&plan.order, &values)?))
}

/// Encodes the next cursor token from the last unformatted `RowMap` when cursor paging is active.
pub fn map_cursor<E: SearchEntity>(
    policy: &QueryPolicy,
    plan: &QueryPlan<E>,
    row: Option<&RowMap>,
) -> Result<Option<String>, SearchError> {
    if !matches!(plan.paging.mode, PagingMode::Cursor(_)) || plan.paging.size == 0 {
        return Ok(None);
    }
    let Some(row) = row else {
        return Ok(None);
    };
    let codec = policy
        .cursor_codec()
        .ok_or_else(|| ParamError::InvalidValue {
            field: policy.names().cursor.clone(),
            reason: "cursor pagination is not configured".to_owned(),
        })?;
    let values = plan
        .order
        .iter()
        .map(|order| {
            let field = E::META
                .field(order.field)
                .ok_or_else(|| ParamError::UnknownField(format!("#{}", order.field.0)))?;
            let value = row
                .get(field.public_name)
                .ok_or_else(|| ParamError::InvalidValue {
                    field: policy.names().cursor.clone(),
                    reason: format!("cursor field {} is not projected", field.public_name),
                })?;
            CursorValue::from_row_value(value)
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Some(codec.encode::<E>(&plan.order, &values)?))
}
