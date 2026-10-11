use super::predicate::CustomOperator;
use crate::{ConfigError, ValueType};

/// A server-owned SQL extension declaration. The SQL template remains in the
/// binary and only bound values can come from a request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServerSqlExtension {
    pub operator: CustomOperator,
    pub input_type: ValueType,
    pub fields: &'static [&'static str],
}

/// Bounded registry used by an HTTP/service boundary. Registration is a
/// startup operation; lookup accepts only a name that was registered by the
/// server and never parses SQL supplied by a caller.
#[derive(Debug, Clone)]
pub struct ServerSqlRegistry {
    extensions: Vec<ServerSqlExtension>,
    max_extensions: usize,
}

impl Default for ServerSqlRegistry {
    fn default() -> Self {
        Self {
            extensions: Vec::new(),
            max_extensions: 64,
        }
    }
}

impl ServerSqlRegistry {
    pub fn with_limit(max_extensions: usize) -> Result<Self, ConfigError> {
        if max_extensions == 0 || max_extensions > 256 {
            return Err(ConfigError::Invalid(
                "server SQL registry limit must be between 1 and 256".to_owned(),
            ));
        }
        Ok(Self {
            extensions: Vec::new(),
            max_extensions,
        })
    }

    pub fn register(&mut self, extension: ServerSqlExtension) -> Result<(), ConfigError> {
        extension.validate()?;
        if self.extensions.len() >= self.max_extensions {
            return Err(ConfigError::Invalid(
                "server SQL extension registry limit exceeded".to_owned(),
            ));
        }
        if self
            .extensions
            .iter()
            .any(|item| item.operator.name() == extension.operator.name())
        {
            return Err(ConfigError::Invalid(
                "duplicate server SQL extension name".to_owned(),
            ));
        }
        self.extensions.push(extension);
        Ok(())
    }

    #[must_use]
    pub fn get(&self, name: &str) -> Option<ServerSqlExtension> {
        self.extensions
            .iter()
            .find(|item| item.operator.name() == name)
            .copied()
    }

    pub fn iter(&self) -> impl Iterator<Item = &ServerSqlExtension> {
        self.extensions.iter()
    }
}

impl ServerSqlExtension {
    pub fn new(
        operator: CustomOperator,
        input_type: ValueType,
        fields: &'static [&'static str],
    ) -> Result<Self, ConfigError> {
        let extension = Self {
            operator,
            input_type,
            fields,
        };
        extension.validate()?;
        Ok(extension)
    }

    fn validate(&self) -> Result<(), ConfigError> {
        if self.fields.is_empty() || self.fields.len() > 256 {
            return Err(ConfigError::Invalid(
                "server SQL extension requires 1..=256 fields".to_owned(),
            ));
        }
        if self
            .operator
            .types()
            .is_some_and(|(_, input)| input != self.input_type)
        {
            return Err(ConfigError::Invalid(
                "server SQL extension input type differs from operator".to_owned(),
            ));
        }
        if self.fields.windows(2).any(|pair| pair[0] >= pair[1]) {
            return Err(ConfigError::Invalid(
                "server SQL extension fields must be sorted and unique".to_owned(),
            ));
        }
        if self
            .fields
            .iter()
            .any(|field| !crate::policy::names::is_name(field))
        {
            return Err(ConfigError::Invalid(
                "server SQL extension fields must be valid public names".to_owned(),
            ));
        }
        Ok(())
    }
}
