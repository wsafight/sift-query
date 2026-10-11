use std::fmt::Write;

use sift_query_core::{PagingMode, QueryPlan, QueryTarget, ScalarValue, SearchEntity};

use super::{RenderState, render_filters, render_order, render_projection};
use crate::{PgRenderer, RenderError, RenderedPgQuery};

impl PgRenderer {
    #[must_use]
    pub fn supports_window_count<E: SearchEntity>(&self, plan: &QueryPlan<E>) -> bool {
        !plan.distinct
            && !plan.meta.is_grouped()
            && !matches!(plan.paging.mode, PagingMode::Cursor(_))
            && matches!(
                plan.target,
                QueryTarget::TypedPage | QueryTarget::MapPage | QueryTarget::StaticPage
            )
            && plan.paging.size > 0
    }

    pub fn render_window_list<E: SearchEntity>(
        &self,
        plan: &QueryPlan<E>,
    ) -> Result<RenderedPgQuery, RenderError> {
        if !self.supports_window_count(plan) {
            return Err(RenderError::WrongTarget);
        }
        let mut state = RenderState::default();
        let mut sql = format!(
            "SELECT {}, count(*) OVER() AS \"sq_total_count\" FROM {}",
            render_projection(plan, true)?,
            plan.meta.from
        );
        render_filters(plan, &mut state, &mut sql, false)?;
        render_order(plan, &mut state, &mut sql)?;
        let limit = state.bind(ScalarValue::I64(i64::from(plan.paging.size)));
        let offset = state.bind(ScalarValue::I64(
            i64::try_from(plan.paging.offset)
                .map_err(|_| RenderError::InvalidCondition("offset overflow".to_owned()))?,
        ));
        write!(sql, " LIMIT {limit} OFFSET {offset}").expect("writing to String cannot fail");
        Ok(state.finish(sql, plan.shape_fingerprint_seed()))
    }
}
