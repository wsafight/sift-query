//! Explicit PostgreSQL extension and text-configuration checks.
use super::operators::{TextConfig, TextQuery, full_text};
use sift_query_core::{ConfigError, CustomOperator, QueryPolicy};
use sqlx::PgPool;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Extension {
    #[cfg(feature = "pg-trgm")]
    Trgm,
    #[cfg(feature = "postgis")]
    Postgis,
    #[cfg(feature = "ltree")]
    Ltree,
}
impl Extension {
    fn name(self) -> &'static str {
        match self {
            #[cfg(feature = "pg-trgm")]
            Self::Trgm => "pg_trgm",
            #[cfg(feature = "postgis")]
            Self::Postgis => "postgis",
            #[cfg(feature = "ltree")]
            Self::Ltree => "ltree",
        }
    }
}

/// Initialization checks are explicit and never execute on the query path.
#[derive(Debug, Clone, Default)]
pub struct PgCapabilities {
    operators: Vec<CustomOperator>,
    extensions: Vec<Extension>,
    configs: Vec<TextConfig>,
}
impl PgCapabilities {
    #[must_use]
    pub fn with_operator(mut self, operator: CustomOperator) -> Self {
        self.operators.push(operator);
        self
    }
    #[must_use]
    pub fn require(mut self, extension: Extension) -> Self {
        if !self.extensions.contains(&extension) {
            self.extensions.push(extension);
        }
        self
    }
    #[must_use]
    pub fn with_full_text(mut self, config: TextConfig) -> Self {
        self.configs.push(config);
        for mode in [
            TextQuery::Plain,
            TextQuery::Phrase,
            TextQuery::Web,
            TextQuery::TsQuery,
        ] {
            self.operators.push(full_text(config, mode));
        }
        self
    }
    pub fn apply(&self, mut policy: QueryPolicy) -> Result<QueryPolicy, ConfigError> {
        for operator in &self.operators {
            let (_, input) = operator.types().ok_or_else(|| {
                ConfigError::Invalid("PostgreSQL capabilities require typed operators".to_owned())
            })?;
            policy = policy.with_custom_operator(*operator, input)?;
        }
        Ok(policy)
    }
    pub async fn check(&self, pool: &PgPool) -> Result<(), ConfigError> {
        let checks = async {
            for extension in &self.extensions {
                let available: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM pg_extension e JOIN pg_namespace n ON e.extnamespace=n.oid WHERE e.extname=$1 AND n.nspname = ANY(current_schemas(true)))").bind(extension.name()).fetch_one(pool).await.map_err(|_| ConfigError::Invalid("PostgreSQL capability check failed".to_owned()))?;
                if !available {
                    return Err(ConfigError::Invalid(format!(
                        "required PostgreSQL extension {} is missing or outside search_path",
                        extension.name()
                    )));
                }
            }
            for config in &self.configs {
                let available: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM pg_ts_config c JOIN pg_namespace n ON c.cfgnamespace=n.oid WHERE n.nspname || '.' || c.cfgname = $1)")
                    .bind(config.name())
                    .fetch_one(pool)
                    .await
                    .map_err(|_| {
                        ConfigError::Invalid("text configuration check failed".to_owned())
                    })?;
                if !available {
                    return Err(ConfigError::Invalid(
                        "text configuration is missing".to_owned(),
                    ));
                }
            }
            Ok(())
        };
        tokio::time::timeout(std::time::Duration::from_secs(5), checks)
            .await
            .map_err(|_| ConfigError::Invalid("PostgreSQL capability check timed out".to_owned()))?
    }
}
