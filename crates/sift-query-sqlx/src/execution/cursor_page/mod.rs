use sift_query_core::{
    CursorValue, PagingMode, ParamError, QueryPlan, SearchEntity, SearchProjection,
};

use crate::{PgSearchError, PgSiftQuery, RowMap};

impl PgSiftQuery {
    pub(crate) fn entity_cursor<E: SearchEntity>(
        &self,
        plan: &QueryPlan<E>,
        row: Option<&E>,
    ) -> Result<Option<String>, PgSearchError> {
        if !matches!(plan.paging.mode, PagingMode::Cursor(_)) || plan.paging.size == 0 {
            return Ok(None);
        }
        let Some(row) = row else {
            return Ok(None);
        };
        let codec = self
            .policy
            .cursor_codec()
            .ok_or_else(|| ParamError::InvalidValue {
                field: self.policy.names().cursor.clone(),
                reason: "cursor pagination is not configured".to_owned(),
            })?;
        let values = plan
            .order
            .iter()
            .map(|order| {
                row.cursor_value(order.field)
                    .ok_or_else(|| ParamError::InvalidValue {
                        field: self.policy.names().cursor.clone(),
                        reason: "entity cannot extract a cursor ordering value".to_owned(),
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;
        codec
            .encode::<E>(&plan.order, &values)
            .map(Some)
            .map_err(PgSearchError::Param)
    }

    pub(crate) fn map_cursor<E: SearchEntity>(
        &self,
        plan: &QueryPlan<E>,
        row: Option<&RowMap>,
    ) -> Result<Option<String>, PgSearchError> {
        if !matches!(plan.paging.mode, PagingMode::Cursor(_)) || plan.paging.size == 0 {
            return Ok(None);
        }
        let Some(row) = row else {
            return Ok(None);
        };
        let codec = self
            .policy
            .cursor_codec()
            .ok_or_else(|| ParamError::InvalidValue {
                field: self.policy.names().cursor.clone(),
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
                        field: self.policy.names().cursor.clone(),
                        reason: format!("cursor field {} is not projected", field.public_name),
                    })?;
                CursorValue::from_row_value(value)
            })
            .collect::<Result<Vec<_>, _>>()?;
        codec
            .encode::<E>(&plan.order, &values)
            .map(Some)
            .map_err(PgSearchError::Param)
    }

    pub(crate) fn projection_cursor<E, P>(
        &self,
        plan: &QueryPlan<E>,
        row: Option<&P>,
    ) -> Result<Option<String>, PgSearchError>
    where
        E: SearchEntity,
        P: SearchProjection<E>,
    {
        if !matches!(plan.paging.mode, PagingMode::Cursor(_)) || plan.paging.size == 0 {
            return Ok(None);
        }
        let Some(row) = row else {
            return Ok(None);
        };
        let codec = self
            .policy
            .cursor_codec()
            .ok_or_else(|| ParamError::InvalidValue {
                field: self.policy.names().cursor.clone(),
                reason: "cursor pagination is not configured".to_owned(),
            })?;
        let values = plan
            .order
            .iter()
            .map(|order| {
                row.cursor_value(order.field)
                    .ok_or_else(|| ParamError::InvalidValue {
                        field: self.policy.names().cursor.clone(),
                        reason: "projection cannot extract a cursor ordering value".to_owned(),
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;
        codec
            .encode::<E>(&plan.order, &values)
            .map(Some)
            .map_err(PgSearchError::Param)
    }
}
#[cfg(test)]
mod tests;
