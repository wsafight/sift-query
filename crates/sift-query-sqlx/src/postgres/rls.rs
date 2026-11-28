//! Trusted transaction-local RLS settings with pool-owned cancellation cleanup.
use crate::{Page, PgSearchError, PgSiftQuery, RowMap, TimeoutStage};
use sift_query_core::{ConfigError, QueryInput, SearchEntity, ServerScope};
use sqlx::pool::PoolConnection;
use sqlx::postgres::{PgConnection, PgRow};
use sqlx::{FromRow, Postgres};
use std::fmt;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::{Instant, timeout_at};

#[derive(Debug, Clone)]
pub struct RlsConfig {
    keys: Arc<[&'static str]>,
    roles: Arc<[&'static str]>,
}
impl RlsConfig {
    pub fn new(keys: &[&'static str], roles: &[&'static str]) -> Result<Self, ConfigError> {
        let identifier = |name: &str| {
            !name.is_empty()
                && name.len() <= 63
                && name
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
        };
        let mut seen = std::collections::HashSet::new();
        if keys.len() > 16
            || keys.iter().any(|key| {
                !seen.insert(*key)
                    || key.split_once('.').is_none_or(|(namespace, name)| {
                        !identifier(namespace)
                            || !identifier(name)
                            || namespace.starts_with("pg_")
                            || name.contains('.')
                    })
            })
            || roles.iter().any(|role| !identifier(role))
        {
            return Err(ConfigError::Invalid(
                "RLS requires unique custom GUC keys and server-declared role identifiers"
                    .to_owned(),
            ));
        }
        Ok(Self {
            keys: keys.into(),
            roles: roles.into(),
        })
    }
    /// All configured claims are required. Values come from trusted auth context.
    pub fn context(
        &self,
        values: &[(&str, &str)],
        role: Option<&str>,
    ) -> Result<RlsContext, ConfigError> {
        let mut seen = std::collections::HashSet::new();
        if values.len() != self.keys.len()
            || values.iter().any(|(key, value)| {
                !self.keys.contains(key)
                    || !seen.insert(*key)
                    || value.len() > 4096
                    || value.contains('\0')
            })
            || role.is_some_and(|role| !self.roles.contains(&role))
        {
            return Err(ConfigError::Invalid(
                "RLS context has missing, duplicate, oversized or unauthorized settings".to_owned(),
            ));
        }
        Ok(RlsContext {
            values: values
                .iter()
                .map(|(key, value)| (key.to_string(), value.to_string()))
                .collect(),
            role: role.map(str::to_owned),
        })
    }
}
#[derive(Clone)]
pub struct RlsContext {
    values: Vec<(String, String)>,
    role: Option<String>,
}
impl fmt::Debug for RlsContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RlsContext")
            .field("claims", &self.values.len())
            .field("role_configured", &self.role.is_some())
            .finish()
    }
}
impl RlsContext {
    /// For an external transaction its owner retains cleanup responsibility.
    pub async fn apply_to(
        &self,
        transaction: &mut sqlx::Transaction<'_, Postgres>,
    ) -> Result<(), PgSearchError> {
        self.apply(&mut *transaction).await
    }
    async fn apply(&self, connection: &mut PgConnection) -> Result<(), PgSearchError> {
        self.apply_with_timeout(connection, None).await
    }

    async fn apply_with_timeout(
        &self,
        connection: &mut PgConnection,
        timeout_ms: Option<&str>,
    ) -> Result<(), PgSearchError> {
        let total =
            usize::from(self.role.is_some()) + self.values.len() + usize::from(timeout_ms.is_some());
        if total == 0 {
            return Ok(());
        }
        let mut keys = Vec::with_capacity(total);
        let mut vals = Vec::with_capacity(total);
        if let Some(role) = &self.role {
            keys.push("role");
            vals.push(role.as_str());
        }
        for (key, value) in &self.values {
            keys.push(key.as_str());
            vals.push(value.as_str());
        }
        if let Some(ms) = timeout_ms {
            keys.push("statement_timeout");
            vals.push(ms);
        }
        sqlx::query(
            "SELECT set_config(k, v, true) FROM unnest($1::text[], $2::text[]) AS t(k, v)",
        )
        .bind(&keys)
        .bind(&vals)
        .execute(&mut *connection)
        .await
        .map_err(|_| context_error())?;
        Ok(())
    }
}
fn context_error() -> PgSearchError {
    sift_query_core::ParamError::InvalidValue {
        field: "rls".to_owned(),
        reason: "RLS context initialization failed".to_owned(),
    }
    .into()
}

/// Drop, query error or timeout closes the owned connection. Successful finish
/// rolls back local settings before returning a clean connection to the pool.
pub struct PgRlsTransaction {
    connection: Option<PoolConnection<Postgres>>,
    executor: PgSiftQuery,
    deadline: Instant,
    poisoned: bool,
}
impl fmt::Debug for PgRlsTransaction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PgRlsTransaction")
            .field("poisoned", &self.poisoned)
            .finish_non_exhaustive()
    }
}
impl PgSiftQuery {
    /// Validates all query parameters and scopes before checking out a connection.
    pub async fn search_rls<E>(
        &self,
        context: &RlsContext,
        query: impl QueryInput<E>,
        scope: &ServerScope<E>,
    ) -> Result<Page<E>, PgSearchError>
    where
        E: SearchEntity + Unpin + for<'row> FromRow<'row, PgRow>,
    {
        let query = query.into_user_query(&self.policy)?;
        sift_query_core::QueryPlan::new(&query, scope, sift_query_core::QueryTarget::TypedPage)?;
        let mut transaction = self.begin_rls(context, self.execution.timeout).await?;
        let page = transaction.search(&query, scope).await?;
        transaction.finish().await?;
        Ok(page)
    }

    pub fn scan_map_stream_rls<E: SearchEntity + Unpin>(
        &self,
        context: &RlsContext,
        query: impl QueryInput<E>,
        scope: &ServerScope<E>,
        policy: crate::ScanPolicy,
    ) -> Result<crate::SearchStream<RowMap>, PgSearchError> {
        let policy = policy.validate()?;
        let plan = crate::execution::stream::scan_plan(
            self.resolve_plan(
                query,
                scope,
                sift_query_core::QueryTarget::MapPage,
                "rls_scan",
            )?,
            policy,
        )?;
        if plan.force_empty || plan.paging.size == 0 {
            return Ok(Box::pin(futures_util::stream::empty()));
        }
        let partition = self.extensions.resolve_partition(&plan, "rls_scan")?;
        let prepared = crate::execution::io::prepare(self.render(
            &plan,
            "rls_scan",
            "list",
            partition.as_deref(),
            crate::cache::RenderRequest::List,
        )?)?;
        let executor = self.clone();
        let context = context.clone();
        let stream = Box::pin(async_stream::try_stream! {
            let mut transaction=executor.begin_rls_inner(&context,policy.timeout).await?;
            transaction.poisoned=true;
            let deadline=transaction.deadline;
            let connection=transaction.connection.as_mut().expect("active transaction");
            let mut rows=sqlx::query_with(prepared.sql(),prepared.arguments).fetch(&mut **connection);
            while let Some(row)=timeout_at(deadline,futures_util::TryStreamExt::try_next(&mut rows)).await.map_err(|_|PgSearchError::Timeout(TimeoutStage::Scan))?? {
                let mut row=crate::postgres::decode::decode_map::<E>(&row,&plan)?;
                executor.format_map_rows::<E>(std::slice::from_mut(&mut row))?;
                yield row;
            }
            drop(rows);
            transaction.poisoned=false;
            transaction.finish().await?;
        });
        Ok(self.bound_scan(stream, policy))
    }
    pub async fn begin_rls(
        &self,
        context: &RlsContext,
        timeout: Duration,
    ) -> Result<PgRlsTransaction, PgSearchError> {
        self.validate_external_strategy()?;
        if timeout.is_zero() || timeout > Duration::from_secs(3600) {
            return Err(context_error());
        }
        self.begin_rls_inner(context, timeout.min(self.execution.timeout))
            .await
    }

    async fn begin_rls_inner(
        &self,
        context: &RlsContext,
        timeout: Duration,
    ) -> Result<PgRlsTransaction, PgSearchError> {
        if timeout.is_zero() || timeout > Duration::from_secs(3600) {
            return Err(context_error());
        }
        let deadline = Instant::now() + timeout;
        let mut connection = self.acquire(deadline).await?;
        connection.close_on_drop();
        timeout_at(deadline, async {
            sqlx::query("BEGIN READ ONLY")
                .execute(&mut *connection)
                .await?;
            let milliseconds = deadline
                .saturating_duration_since(Instant::now())
                .as_millis()
                .max(1)
                .to_string();
            context
                .apply_with_timeout(&mut connection, Some(&milliseconds))
                .await?;
            Ok::<_, PgSearchError>(())
        })
        .await
        .map_err(|_| PgSearchError::Timeout(TimeoutStage::PoolAcquire))??;
        Ok(PgRlsTransaction {
            connection: Some(connection),
            executor: self.clone(),
            deadline,
            poisoned: false,
        })
    }
}
impl PgRlsTransaction {
    fn usable(&self) -> Result<(), PgSearchError> {
        if self.poisoned || self.connection.is_none() {
            Err(context_error())
        } else {
            Ok(())
        }
    }
    pub async fn search<E>(
        &mut self,
        query: impl QueryInput<E>,
        scope: &ServerScope<E>,
    ) -> Result<Page<E>, PgSearchError>
    where
        E: SearchEntity + Unpin + for<'row> FromRow<'row, PgRow>,
    {
        self.usable()?;
        self.poisoned = true;
        let result = timeout_at(
            self.deadline,
            self.executor.search_on(
                self.connection.as_mut().expect("active transaction"),
                query,
                scope,
            ),
        )
        .await
        .map_err(|_| PgSearchError::Timeout(TimeoutStage::ListQuery))
        .and_then(|result| result);
        self.poisoned = result.is_err();
        result
    }
    pub async fn search_map<E: SearchEntity>(
        &mut self,
        query: impl QueryInput<E>,
        scope: &ServerScope<E>,
    ) -> Result<Page<RowMap>, PgSearchError> {
        self.usable()?;
        self.poisoned = true;
        let result = timeout_at(
            self.deadline,
            self.executor.search_map_on(
                self.connection.as_mut().expect("active transaction"),
                query,
                scope,
            ),
        )
        .await
        .map_err(|_| PgSearchError::Timeout(TimeoutStage::ListQuery))
        .and_then(|result| result);
        self.poisoned = result.is_err();
        result
    }
    /// Explicit cleanup; failed transactions are discarded.
    pub async fn finish(mut self) -> Result<(), PgSearchError> {
        self.usable()?;
        let connection = self.connection.as_mut().expect("active transaction");
        timeout_at(
            self.deadline,
            sqlx::query("ROLLBACK").execute(&mut **connection),
        )
        .await
        .map_err(|_| PgSearchError::Timeout(TimeoutStage::ListQuery))??;
        timeout_at(self.deadline, connection.return_to_pool())
            .await
            .map_err(|_| PgSearchError::Timeout(TimeoutStage::PoolAcquire))?;
        Ok(())
    }
}
