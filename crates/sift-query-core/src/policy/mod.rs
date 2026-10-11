//! Immutable policy configuration, limits and entity validation.
use crate::{
    Codec, CodecRegistry, ConfigError, CursorCodec, ParamError, RawParamFilter, RawParams,
};
use sift_query_algorithm::BoundedLruMap;
use std::any::TypeId;
use std::borrow::Cow;
use std::sync::{Arc, Mutex};

pub(crate) mod budget;
mod fingerprint;
mod limits;
pub(crate) mod names;
mod validation;
use fingerprint::{compatibility_fingerprint, policy_fingerprint};
use limits::ensure_within_hard_limits;
pub use limits::{QueryHardLimits, QueryLimits};
pub use names::ParamNames;
use names::is_name;

/// Immutable parser and execution policy.
#[derive(Debug, Clone)]
pub struct QueryPolicy {
    names: ParamNames,
    limits: QueryLimits,
    hard_limits: QueryHardLimits,
    compatibility: CompatibilityPolicy,
    cursor_codec: Option<CursorCodec>,
    filters: Vec<Arc<dyn RawParamFilter>>,
    codecs: CodecRegistry,
    operators: Vec<crate::RegisteredOperator>,
    fingerprint: u64,
    validated_names: Arc<Mutex<BoundedLruMap<TypeId, Result<(), ParamError>>>>,
    validated_codecs: Arc<Mutex<BoundedLruMap<TypeId, Result<(), ParamError>>>>,
    codec_bindings: Arc<Mutex<BoundedLruMap<TypeId, Result<Arc<[CodecBinding]>, ParamError>>>>,
    validated_operators: Arc<Mutex<BoundedLruMap<TypeId, Result<(), ParamError>>>>,
}

#[derive(Debug, Clone)]
struct CodecBinding {
    field: &'static crate::FieldMeta,
    codec: Arc<dyn Codec>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum CompatibilityPolicy {
    #[default]
    Strict,
    BeanSearcher4812,
}

impl Default for QueryPolicy {
    fn default() -> Self {
        let names = ParamNames::default();
        let limits = QueryLimits::default();
        Self {
            fingerprint: policy_fingerprint(&names, &limits),
            names,
            hard_limits: QueryHardLimits::default(),
            limits,
            compatibility: CompatibilityPolicy::Strict,
            cursor_codec: None,
            filters: Vec::new(),
            codecs: CodecRegistry::default(),
            operators: Vec::new(),
            validated_names: name_cache(),
            validated_codecs: validation_cache(),
            codec_bindings: codec_binding_cache(),
            validated_operators: validation_cache(),
        }
    }
}

impl QueryPolicy {
    pub fn new(names: ParamNames, limits: QueryLimits) -> Result<Self, ConfigError> {
        Self::with_hard_limits(names, limits, QueryHardLimits::default())
    }

    /// Builds a policy with explicitly elevated hard limits.
    pub fn with_hard_limits(
        names: ParamNames,
        limits: QueryLimits,
        hard_limits: QueryHardLimits,
    ) -> Result<Self, ConfigError> {
        names.validate()?;
        ensure_within_hard_limits(&limits, &hard_limits.0)?;
        if limits.timeout_ms == 0 {
            return Err(ConfigError::Invalid(
                "query timeout must be greater than zero".to_owned(),
            ));
        }
        Ok(Self {
            fingerprint: policy_fingerprint(&names, &limits),
            names,
            limits,
            hard_limits,
            compatibility: CompatibilityPolicy::Strict,
            cursor_codec: None,
            filters: Vec::new(),
            codecs: CodecRegistry::default(),
            operators: Vec::new(),
            validated_names: name_cache(),
            validated_codecs: validation_cache(),
            codec_bindings: codec_binding_cache(),
            validated_operators: validation_cache(),
        })
    }

    #[must_use]
    pub fn bean_searcher_4812() -> Self {
        let mut policy = Self::default();
        policy.compatibility = CompatibilityPolicy::BeanSearcher4812;
        policy.fingerprint = policy_fingerprint(&policy.names, &policy.limits)
            ^ compatibility_fingerprint(policy.compatibility);
        policy
    }

    #[must_use]
    pub fn with_cursor_codec(mut self, codec: CursorCodec) -> Self {
        self.cursor_codec = Some(codec);
        self.recompute_fingerprint();
        self
    }

    /// Appends a server-owned parameter normalization step.
    #[must_use]
    pub fn with_raw_param_filter<F: RawParamFilter + 'static>(mut self, filter: F) -> Self {
        self.filters.push(Arc::new(filter));
        self.recompute_fingerprint();
        self
    }

    /// Appends a named input/result codec.
    #[must_use]
    pub fn with_codec<C: Codec + 'static>(mut self, codec: C) -> Self {
        self.codecs.push(codec);
        self.validated_codecs = validation_cache();
        self.codec_bindings = codec_binding_cache();
        self.recompute_fingerprint();
        self
    }

    /// Registers one typed HTTP operator. Aliases cannot shadow builtins.
    pub fn with_custom_operator(
        mut self,
        operator: crate::CustomOperator,
        input_type: crate::ValueType,
    ) -> Result<Self, ConfigError> {
        if !is_name(operator.name())
            || operator.name().len() > 64
            || crate::Operator::parse(operator.name()).is_some()
            || self
                .operators
                .iter()
                .any(|item| item.operator.name() == operator.name())
            || self.operators.len() >= 64
        {
            return Err(ConfigError::Invalid(
                "invalid, duplicate or reserved custom operator registration".to_owned(),
            ));
        }
        if operator.types().is_some_and(|types| types.1 != input_type) {
            return Err(ConfigError::Invalid(
                "operator input type differs from registration".to_owned(),
            ));
        }
        self.operators.push(crate::RegisteredOperator {
            operator,
            field_type: operator.types().map_or(input_type, |types| types.0),
            input_type,
        });
        self.validated_operators = validation_cache();
        self.recompute_fingerprint();
        Ok(self)
    }

    /// Registers a server-owned SQL extension and verifies its generated field
    /// whitelist against an entity before it can be exposed to HTTP.
    pub fn with_server_sql_extension<E: crate::SearchEntity>(
        self,
        extension: crate::ServerSqlExtension,
    ) -> Result<Self, ConfigError> {
        for field_name in extension.fields {
            let field = E::META.resolve_field(field_name).ok_or_else(|| {
                ConfigError::Invalid(format!(
                    "server SQL extension references unknown field {field_name}"
                ))
            })?;
            if !E::custom_operator_names(field.id).contains(&extension.operator.name()) {
                return Err(ConfigError::Invalid(format!(
                    "server SQL extension {} is not whitelisted on field {}",
                    extension.operator.name(),
                    field_name
                )));
            }
        }
        self.with_custom_operator(extension.operator, extension.input_type)
    }

    /// Registers every bounded, server-owned SQL extension in a registry for
    /// one entity. The registry itself contains no request data and is
    /// expected to be constructed during application startup.
    pub fn with_server_sql_registry<E: crate::SearchEntity>(
        mut self,
        registry: &crate::ServerSqlRegistry,
    ) -> Result<Self, ConfigError> {
        for extension in registry.iter().copied() {
            self = self.with_server_sql_extension::<E>(extension)?;
        }
        Ok(self)
    }

    #[must_use]
    pub fn registered_operator(&self, name: &str) -> Option<&crate::RegisteredOperator> {
        self.operators
            .iter()
            .find(|item| item.operator.name() == name)
    }

    #[must_use]
    pub fn names(&self) -> &ParamNames {
        &self.names
    }

    /// Copies the policy for a remote endpoint with different control names.
    pub fn with_param_names(mut self, names: ParamNames) -> Result<Self, ConfigError> {
        names.validate()?;
        self.names = names;
        self.validated_names = name_cache();
        self.recompute_fingerprint();
        Ok(self)
    }

    #[must_use]
    pub fn limits(&self) -> &QueryLimits {
        &self.limits
    }

    #[must_use]
    pub fn hard_limits(&self) -> &QueryHardLimits {
        &self.hard_limits
    }

    #[must_use]
    pub const fn fingerprint(&self) -> u64 {
        self.fingerprint
    }

    #[must_use]
    pub const fn compatibility(&self) -> CompatibilityPolicy {
        self.compatibility
    }

    #[must_use]
    pub fn cursor_codec(&self) -> Option<&CursorCodec> {
        self.cursor_codec.as_ref()
    }

    pub(crate) fn apply_raw_filters<'raw, E: crate::SearchEntity>(
        &self,
        raw: &'raw RawParams,
    ) -> Result<Cow<'raw, RawParams>, ParamError> {
        if self.filters.is_empty() {
            return Ok(Cow::Borrowed(raw));
        }
        if self.filters.len() > self.limits.max_filter_steps {
            return Err(ParamError::TooComplex("parameter filter chain"));
        }
        let mut current = raw.clone();
        let started = std::time::Instant::now();
        for filter in &self.filters {
            current = filter.filter_with_limits(E::META, &current, &self.limits)?;
            if current.len() > self.limits.max_filter_items
                || current.decoded_bytes() > self.limits.max_filter_bytes
            {
                return Err(ParamError::TooComplex("filtered parameter budget"));
            }
            if started.elapsed().as_micros() > u128::from(self.limits.max_filter_micros) {
                return Err(ParamError::TooComplex("parameter filter time budget"));
            }
        }
        Ok(Cow::Owned(current))
    }
}

fn name_cache() -> Arc<Mutex<BoundedLruMap<TypeId, Result<(), ParamError>>>> {
    validation_cache()
}

fn validation_cache() -> Arc<Mutex<BoundedLruMap<TypeId, Result<(), ParamError>>>> {
    Arc::new(Mutex::new(
        BoundedLruMap::new(256).expect("fixed nonzero metadata cache capacity"),
    ))
}

fn codec_binding_cache()
-> Arc<Mutex<BoundedLruMap<TypeId, Result<Arc<[CodecBinding]>, ParamError>>>> {
    Arc::new(Mutex::new(
        BoundedLruMap::new(256).expect("fixed nonzero metadata cache capacity"),
    ))
}
