use std::sync::Arc;

use sift_query_core::{
    PredicateArena, PredicateNode, QueryInput, QueryTarget, SearchEntity, ServerScope,
};
use sqlx::{AssertSqlSafe, Row};
use tokio::time::{Instant, timeout_at};

use crate::cache::RenderRequest;
use crate::execution::io::{configure_statement_timeout, finish_statement, prepare};
use crate::{PgSearchError, PgSiftQuery, QueryShapeId, TimeoutStage};

/// A conservative, server-side index hint. It is advisory only and never
/// creates or changes database indexes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexAdvice {
    pub field: &'static str,
    pub reason: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticStage {
    pub phase: &'static str,
    pub shape_id: QueryShapeId,
    pub sql: Arc<str>,
    pub explain_json: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryDiagnosticReport {
    pub entity: &'static str,
    pub stages: Vec<DiagnosticStage>,
    pub index_advice: Vec<IndexAdvice>,
}

impl PgSiftQuery {
    /// Returns parameterized SQL, PostgreSQL JSON plans, and conservative
    /// index hints for an already authorized query. Callers should expose this
    /// admin-only operation behind their own authorization boundary.
    pub async fn diagnose<E: SearchEntity>(
        &self,
        query: impl QueryInput<E>,
        scope: &ServerScope<E>,
    ) -> Result<QueryDiagnosticReport, PgSearchError> {
        let plan = self.resolve_plan(query, scope, QueryTarget::MapPage, "diagnose")?;
        if plan.force_empty {
            return Ok(QueryDiagnosticReport {
                entity: E::META.entity_name,
                stages: Vec::new(),
                index_advice: Vec::new(),
            });
        }
        let list = self.render::<E>(&plan, "diagnose", "list", None, RenderRequest::List)?;
        let cluster = self.render::<E>(
            &plan,
            "diagnose",
            "cluster",
            None,
            RenderRequest::Cluster(None),
        )?;
        let deadline = self.deadline::<E>();
        let list = explain_stage(self, "list", list, deadline).await?;
        let cluster = explain_stage(self, "cluster", cluster, deadline).await?;
        Ok(QueryDiagnosticReport {
            entity: E::META.entity_name,
            stages: vec![list, cluster],
            index_advice: index_advice(E::META, &plan.row_predicate, &plan.order),
        })
    }
}

async fn explain_stage(
    search: &PgSiftQuery,
    phase: &'static str,
    rendered: crate::RenderedPgQuery,
    deadline: Instant,
) -> Result<DiagnosticStage, PgSearchError> {
    let shape_id = rendered.shape_id;
    let sql = Arc::clone(&rendered.sql);
    let prepared = prepare(rendered)?;
    let explain_sql: Arc<str> = format!("EXPLAIN (FORMAT JSON) {}", prepared.sql).into();
    let connection = search.acquire(deadline).await?;
    let mut connection =
        configure_statement_timeout(connection, deadline, TimeoutStage::SchemaValidation).await?;
    let result = timeout_at(
        deadline,
        sqlx::query_with(AssertSqlSafe(explain_sql), prepared.arguments)
            .fetch_one(&mut *connection),
    )
    .await;
    let Ok(result) = result else {
        crate::execution::io::close_hard(connection).await;
        return Err(PgSearchError::Timeout(TimeoutStage::SchemaValidation));
    };
    let (mut connection, row) =
        finish_statement(connection, result, TimeoutStage::SchemaValidation).await?;
    let explain_json = row
        .try_get::<sqlx::types::Json<serde_json::Value>, _>(0)
        .map(|value| value.0.to_string())?;
    let _ = &mut connection;
    Ok(DiagnosticStage {
        phase,
        shape_id,
        sql,
        explain_json,
    })
}

fn index_advice(
    meta: &'static sift_query_core::EntityMeta,
    predicate: &PredicateArena,
    order: &[sift_query_core::OrderBy],
) -> Vec<IndexAdvice> {
    let mut advice = Vec::new();
    for node in predicate.nodes() {
        let PredicateNode::Condition(condition) = node else {
            continue;
        };
        let Some(field) = meta.field(condition.field) else {
            continue;
        };
        if matches!(
            condition.operator,
            sift_query_core::Operator::Eq | sift_query_core::Operator::InList
        ) {
            advice.push(IndexAdvice {
                field: field.public_name,
                reason: "equality or IN filter; consider an index on this field",
            });
        }
    }
    for item in order {
        if let Some(field) = meta.field(item.field)
            && !advice.iter().any(|hint| hint.field == field.public_name)
        {
            advice.push(IndexAdvice {
                field: field.public_name,
                reason: "ordering field; consider including it in a composite index",
            });
        }
    }
    advice
}
