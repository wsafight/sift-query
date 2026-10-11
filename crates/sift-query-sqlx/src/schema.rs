use std::sync::Arc;

use sift_query_core::{QueryBuilder, QueryTarget, SearchEntity, ServerScope, ValueType};
use sqlx::{AssertSqlSafe, Column, Executor, SqlSafeStr, TypeInfo};
use tokio::time::timeout_at;

use crate::cache::RenderRequest;
use crate::execution::io::{configure_statement_timeout, finish_statement, prepare};
use crate::{PgSearchError, PgSiftQuery, TimeoutStage};

/// What the schema contract validator checked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchemaValidationTarget {
    /// Validate the entity's complete dynamic projection and query source.
    Entity,
}

/// Successful result from [`PgSiftQuery::validate_schema`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaColumnReport {
    pub alias: String,
    pub expected_type: ValueType,
    pub database_type: String,
    pub nullable: Option<bool>,
    pub compatible: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaValidationReport {
    pub entity: &'static str,
    pub target: SchemaValidationTarget,
    pub query_shape_id: crate::QueryShapeId,
    pub columns: Vec<SchemaColumnReport>,
}

impl PgSiftQuery {
    /// Validates an entity's source, projection, predicates, grouping and
    /// aliases by asking PostgreSQL to plan the generated query.
    ///
    /// `EXPLAIN` plans the statement without reading result rows. This makes
    /// the method suitable for application startup and CI contract checks;
    /// it does not inspect `information_schema` and does not mutate the
    /// database.
    pub async fn validate_schema<E: SearchEntity>(
        &self,
    ) -> Result<SchemaValidationReport, PgSearchError> {
        let query = QueryBuilder::<E>::new(&self.policy).build()?;
        let scope = ServerScope::<E>::unrestricted();
        let plan = self.resolve_plan(&query, &scope, QueryTarget::MapPage, "schema_validation")?;
        let rendered = self.render::<E>(
            &plan,
            "schema_validation",
            "list",
            None,
            RenderRequest::List,
        )?;
        let prepared = prepare(rendered)?;
        let deadline = self.deadline::<E>();
        let sql: Arc<str> = format!("EXPLAIN (FORMAT JSON) {}", prepared.sql).into();
        let connection = self.acquire(deadline).await?;
        let mut connection =
            configure_statement_timeout(connection, deadline, TimeoutStage::SchemaValidation)
                .await?;
        let result = timeout_at(
            deadline,
            sqlx::query_with(AssertSqlSafe(sql), prepared.arguments).fetch_one(&mut *connection),
        )
        .await;
        let Ok(result) = result else {
            crate::execution::io::close_hard(connection).await;
            return Err(PgSearchError::Timeout(TimeoutStage::SchemaValidation));
        };
        let _ = finish_statement(connection, result, TimeoutStage::SchemaValidation).await?;
        let connection = self.acquire(deadline).await?;
        let mut connection =
            configure_statement_timeout(connection, deadline, TimeoutStage::SchemaValidation)
                .await?;
        let described = timeout_at(
            deadline,
            (&mut *connection).describe(AssertSqlSafe(Arc::clone(&prepared.sql)).into_sql_str()),
        )
        .await;
        let Ok(described) = described else {
            crate::execution::io::close_hard(connection).await;
            return Err(PgSearchError::Timeout(TimeoutStage::SchemaValidation));
        };
        let (_, described) =
            finish_statement(connection, described, TimeoutStage::SchemaValidation).await?;
        let mut columns = Vec::with_capacity(plan.projection.len());
        let mut mismatches = Vec::new();
        for (index, field_id) in plan.projection.iter().enumerate() {
            let field =
                E::META
                    .field(*field_id)
                    .ok_or(sift_query_core::ParamError::UnknownField(format!(
                        "#{}",
                        field_id.0
                    )))?;
            let alias = plan
                .projection_aliases
                .as_ref()
                .and_then(|aliases| aliases.get(index).copied())
                .or(field.result_alias)
                .ok_or_else(|| {
                    sift_query_core::ParamError::NotSelectable(field.public_name.to_owned())
                })?;
            let Some(column) = described.columns().get(index) else {
                mismatches.push(format!("missing result column {alias}"));
                columns.push(SchemaColumnReport {
                    alias: alias.to_owned(),
                    expected_type: field.value_type,
                    database_type: "<missing>".to_owned(),
                    nullable: None,
                    compatible: false,
                });
                continue;
            };
            let database_type = column.type_info().name().to_owned();
            let compatible = column.name() == alias
                && postgres_type_compatible(database_type.as_str(), field.value_type);
            if !compatible {
                mismatches.push(format!(
                    "{}: expected alias/type {}/ {:?}, got {}/{}",
                    field.public_name,
                    alias,
                    field.value_type,
                    column.name(),
                    database_type
                ));
            }
            columns.push(SchemaColumnReport {
                alias: alias.to_owned(),
                expected_type: field.value_type,
                database_type,
                nullable: described.nullable(index),
                compatible,
            });
        }
        if described.columns().len() != plan.projection.len() {
            mismatches.push(format!(
                "expected {} result columns, got {}",
                plan.projection.len(),
                described.columns().len()
            ));
        }
        if !mismatches.is_empty() {
            return Err(crate::SchemaValidationError {
                entity: E::META.entity_name,
                details: mismatches.join("; "),
            }
            .into());
        }
        Ok(SchemaValidationReport {
            entity: E::META.entity_name,
            target: SchemaValidationTarget::Entity,
            query_shape_id: prepared.shape_id,
            columns,
        })
    }
}

fn postgres_type_compatible(actual: &str, expected: ValueType) -> bool {
    match expected {
        ValueType::Bool => matches!(actual, "BOOL" | "BOOLEAN" | "bool" | "boolean"),
        ValueType::I32 => matches!(actual, "INT4" | "INTEGER" | "int4" | "integer"),
        ValueType::I64 => matches!(actual, "INT8" | "BIGINT" | "int8" | "bigint"),
        ValueType::F32 => matches!(actual, "FLOAT4" | "REAL" | "float4" | "real"),
        ValueType::F64 => matches!(
            actual,
            "FLOAT8" | "DOUBLE PRECISION" | "float8" | "double precision"
        ),
        #[cfg(feature = "decimal")]
        ValueType::Decimal => matches!(actual, "NUMERIC" | "DECIMAL" | "numeric" | "decimal"),
        ValueType::Text => matches!(
            actual,
            "TEXT" | "VARCHAR" | "BPCHAR" | "NAME" | "text" | "varchar" | "bpchar" | "name"
        ),
        ValueType::Bytes => matches!(actual, "BYTEA" | "bytea"),
        #[cfg(feature = "chrono")]
        ValueType::Date => matches!(actual, "DATE" | "date"),
        #[cfg(feature = "chrono")]
        ValueType::Time => matches!(
            actual,
            "TIME" | "TIME WITHOUT TIME ZONE" | "time" | "time without time zone"
        ),
        #[cfg(feature = "chrono")]
        ValueType::DateTime => matches!(
            actual,
            "TIMESTAMP"
                | "TIMESTAMP WITHOUT TIME ZONE"
                | "timestamp"
                | "timestamp without time zone"
        ),
        #[cfg(feature = "chrono")]
        ValueType::Timestamp => matches!(
            actual,
            "TIMESTAMPTZ" | "TIMESTAMP WITH TIME ZONE" | "timestamptz" | "timestamp with time zone"
        ),
        #[cfg(feature = "uuid")]
        ValueType::Uuid => matches!(actual, "UUID" | "uuid"),
        #[cfg(feature = "json")]
        ValueType::Json => matches!(actual, "JSON" | "JSONB" | "json" | "jsonb"),
        #[cfg(feature = "postgres-types")]
        ValueType::ArrayI64 => matches!(actual, "_INT8" | "_int8"),
        #[cfg(feature = "postgres-types")]
        ValueType::ArrayText => matches!(actual, "_TEXT" | "_text"),
        #[cfg(feature = "postgres-types")]
        ValueType::Int8Range => matches!(actual, "INT8RANGE" | "int8range"),
        #[cfg(feature = "postgres-types")]
        ValueType::Network => matches!(actual, "INET" | "CIDR" | "inet" | "cidr"),
        _ => false,
    }
}
