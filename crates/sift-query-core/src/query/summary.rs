use std::marker::PhantomData;

use crate::{ConfigError, FieldId, FieldRef, ParamError, SearchEntity, SummableKind, ValueType};

/// Trusted server-side aggregate expression containing a `{field}` placeholder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct CustomAggregate {
    name: &'static str,
    template: &'static str,
    output_type: ValueType,
}

impl CustomAggregate {
    pub fn new(
        name: &'static str,
        template: &'static str,
        output_type: ValueType,
    ) -> Result<Self, ConfigError> {
        if name.is_empty()
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        {
            return Err(ConfigError::Invalid(
                "custom aggregate name must be ASCII alphanumeric or underscore".to_owned(),
            ));
        }
        let remainder = template.replace("{field}", "");
        if !template.contains("{field}")
            || remainder.contains(['{', '}'])
            || template.contains(';')
            || template.contains("--")
            || template.contains("/*")
            || template.contains('$')
        {
            return Err(ConfigError::Invalid(
                "custom aggregate template contains a forbidden token".to_owned(),
            ));
        }
        Ok(Self {
            name,
            template,
            output_type,
        })
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        self.name
    }

    #[must_use]
    pub const fn template(self) -> &'static str {
        self.template
    }

    #[must_use]
    pub const fn output_type(self) -> ValueType {
        self.output_type
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum AggregateFn {
    Sum,
    Avg,
    Min,
    Max,
    Custom(CustomAggregate),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SummaryRequest {
    pub field: FieldId,
    pub function: AggregateFn,
    pub input_type: ValueType,
}

#[derive(Debug)]
pub struct SearchOptions<E: SearchEntity> {
    requests: Vec<SummaryRequest>,
    error: Option<ParamError>,
    _entity: PhantomData<fn() -> E>,
}

impl<E: SearchEntity> SearchOptions<E> {
    #[must_use]
    pub fn builder() -> Self {
        Self {
            requests: Vec::new(),
            error: None,
            _entity: PhantomData,
        }
    }

    #[must_use]
    pub fn sum<T, K: SummableKind>(self, field: FieldRef<E, T, K>) -> Self {
        self.push(field.id(), AggregateFn::Sum)
    }

    #[must_use]
    pub fn avg<T, K: SummableKind>(self, field: FieldRef<E, T, K>) -> Self {
        self.push(field.id(), AggregateFn::Avg)
    }

    #[must_use]
    pub fn min<T, K: SummableKind>(self, field: FieldRef<E, T, K>) -> Self {
        self.push(field.id(), AggregateFn::Min)
    }

    #[must_use]
    pub fn max<T, K: SummableKind>(self, field: FieldRef<E, T, K>) -> Self {
        self.push(field.id(), AggregateFn::Max)
    }

    #[must_use]
    pub fn custom<T, K: SummableKind>(
        self,
        field: FieldRef<E, T, K>,
        aggregate: CustomAggregate,
    ) -> Self {
        self.push(field.id(), AggregateFn::Custom(aggregate))
    }

    fn push(mut self, field_id: FieldId, function: AggregateFn) -> Self {
        if self.requests.len() >= 8 {
            self.error = Some(ParamError::TooComplex("summary count"));
        } else if self
            .requests
            .iter()
            .any(|request| request.field == field_id && request.function == function)
        {
            self.error = Some(ParamError::DuplicateParameter("summary".to_owned()));
        } else if let Some(meta) = E::META.field(field_id) {
            self.requests.push(SummaryRequest {
                field: field_id,
                function,
                input_type: meta.value_type,
            });
        } else {
            self.error = Some(ParamError::UnknownField(format!("#{}", field_id.0)));
        }
        self
    }

    pub fn build(self) -> Result<Self, ParamError> {
        if let Some(error) = self.error {
            Err(error)
        } else {
            Ok(self)
        }
    }

    #[must_use]
    pub fn requests(&self) -> &[SummaryRequest] {
        &self.requests
    }
}
