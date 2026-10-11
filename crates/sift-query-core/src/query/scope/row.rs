use super::{ScopeLimits, validate_conditions, validate_predicates};
use crate::{
    Condition, CustomOperator, FieldId, FieldKind, FieldRef, IntoScalarValue, Operator, ParamError,
    PredicateArena, PredicatePhase, RowPhaseKind, ScalarValue, SearchEntity, ServerScope,
};
use std::marker::PhantomData;

pub struct ServerScopeBuilder<E: SearchEntity> {
    conditions: Vec<Condition>,
    limits: ScopeLimits,
    error: Option<ParamError>,
    _entity: PhantomData<fn() -> E>,
}

impl<E: SearchEntity> ServerScope<E> {
    #[must_use]
    pub fn new() -> ServerScopeBuilder<E> {
        Self::builder(ScopeLimits::default())
    }

    #[must_use]
    pub fn builder(limits: ScopeLimits) -> ServerScopeBuilder<E> {
        ServerScopeBuilder {
            conditions: Vec::new(),
            limits,
            error: None,
            _entity: PhantomData,
        }
    }

    pub fn all(scopes: impl IntoIterator<Item = Self>) -> Result<Self, ParamError> {
        Self::all_with_limits(scopes, ScopeLimits::default())
    }

    pub fn any(scopes: impl IntoIterator<Item = Self>) -> Result<Self, ParamError> {
        Self::any_with_limits(scopes, ScopeLimits::default())
    }

    pub fn all_with_limits(
        scopes: impl IntoIterator<Item = Self>,
        limits: ScopeLimits,
    ) -> Result<Self, ParamError> {
        Self::combine(scopes, true, limits)
    }

    pub fn any_with_limits(
        scopes: impl IntoIterator<Item = Self>,
        limits: ScopeLimits,
    ) -> Result<Self, ParamError> {
        Self::combine(scopes, false, limits)
    }

    fn combine(
        scopes: impl IntoIterator<Item = Self>,
        and: bool,
        limits: ScopeLimits,
    ) -> Result<Self, ParamError> {
        let scopes = scopes.into_iter().collect::<Vec<_>>();
        if scopes.is_empty() {
            return Err(ParamError::InvalidValue {
                field: "server scope".to_owned(),
                reason: "scope composition must not be empty".to_owned(),
            });
        }
        if scopes
            .iter()
            .any(|scope| scope.schema_fingerprint != E::META.schema_fingerprint)
        {
            return Err(ParamError::UnknownField(
                "entity schema mismatch".to_owned(),
            ));
        }
        validate_predicates(scopes.iter().map(|scope| &scope.predicate), limits)?;
        let field_access = scopes
            .iter()
            .skip(1)
            .fold(scopes[0].field_access.clone(), |access, scope| {
                access.intersect(&scope.field_access)
            });
        let predicates = scopes.into_iter().map(|scope| scope.predicate);
        Ok(Self {
            predicate: if and {
                PredicateArena::combine_and(predicates)
            } else {
                PredicateArena::combine_or(predicates)
            },
            schema_fingerprint: E::META.schema_fingerprint,
            field_access,
            _entity: PhantomData,
        })
    }
}

impl<E: SearchEntity> ServerScopeBuilder<E> {
    #[must_use]
    pub fn eq<T: IntoScalarValue, K: RowPhaseKind>(
        self,
        field: FieldRef<E, T, K>,
        value: T,
    ) -> Self {
        self.push_condition(
            field.id(),
            Operator::Eq,
            vec![value.into_scalar_value()],
            false,
        )
    }

    #[must_use]
    pub fn ne<T: IntoScalarValue, K: RowPhaseKind>(
        self,
        field: FieldRef<E, T, K>,
        value: T,
    ) -> Self {
        self.push_condition(
            field.id(),
            Operator::Ne,
            vec![value.into_scalar_value()],
            false,
        )
    }

    #[must_use]
    pub fn gt<T: IntoScalarValue, K: RowPhaseKind>(
        self,
        field: FieldRef<E, T, K>,
        value: T,
    ) -> Self {
        self.push_condition(
            field.id(),
            Operator::Gt,
            vec![value.into_scalar_value()],
            false,
        )
    }

    #[must_use]
    pub fn ge<T: IntoScalarValue, K: RowPhaseKind>(
        self,
        field: FieldRef<E, T, K>,
        value: T,
    ) -> Self {
        self.push_condition(
            field.id(),
            Operator::Ge,
            vec![value.into_scalar_value()],
            false,
        )
    }

    #[must_use]
    pub fn lt<T: IntoScalarValue, K: RowPhaseKind>(
        self,
        field: FieldRef<E, T, K>,
        value: T,
    ) -> Self {
        self.push_condition(
            field.id(),
            Operator::Lt,
            vec![value.into_scalar_value()],
            false,
        )
    }

    #[must_use]
    pub fn le<T: IntoScalarValue, K: RowPhaseKind>(
        self,
        field: FieldRef<E, T, K>,
        value: T,
    ) -> Self {
        self.push_condition(
            field.id(),
            Operator::Le,
            vec![value.into_scalar_value()],
            false,
        )
    }

    #[must_use]
    pub fn between<T: IntoScalarValue, K: RowPhaseKind>(
        self,
        field: FieldRef<E, T, K>,
        lower: T,
        upper: T,
    ) -> Self {
        self.push_condition(
            field.id(),
            Operator::Between,
            vec![lower.into_scalar_value(), upper.into_scalar_value()],
            false,
        )
    }

    #[must_use]
    pub fn in_list<T, K, I>(self, field: FieldRef<E, T, K>, values: I) -> Self
    where
        T: IntoScalarValue,
        K: RowPhaseKind,
        I: IntoIterator<Item = T>,
    {
        self.push_condition(
            field.id(),
            Operator::InList,
            values
                .into_iter()
                .map(IntoScalarValue::into_scalar_value)
                .collect(),
            false,
        )
    }

    #[must_use]
    pub fn is_null<T, K: RowPhaseKind>(self, field: FieldRef<E, T, K>) -> Self {
        self.push_condition(field.id(), Operator::IsNull, Vec::new(), false)
    }

    #[must_use]
    pub fn is_not_null<T, K: RowPhaseKind>(self, field: FieldRef<E, T, K>) -> Self {
        self.push_condition(field.id(), Operator::NotNull, Vec::new(), false)
    }

    #[must_use]
    pub fn starts_with<K: RowPhaseKind>(
        self,
        field: FieldRef<E, String, K>,
        value: impl Into<String>,
    ) -> Self {
        self.push_condition(
            field.id(),
            Operator::StartWith,
            vec![ScalarValue::Text(value.into())],
            false,
        )
    }

    #[must_use]
    pub fn contains<K: RowPhaseKind>(
        self,
        field: FieldRef<E, String, K>,
        value: impl Into<String>,
    ) -> Self {
        self.push_condition(
            field.id(),
            Operator::Contain,
            vec![ScalarValue::Text(value.into())],
            false,
        )
    }

    #[must_use]
    pub fn contains_ignore_case<K: RowPhaseKind>(
        self,
        field: FieldRef<E, String, K>,
        value: impl Into<String>,
    ) -> Self {
        self.push_condition(
            field.id(),
            Operator::Contain,
            vec![ScalarValue::Text(value.into())],
            true,
        )
    }

    #[must_use]
    pub fn custom<T, K, I>(
        self,
        field: FieldRef<E, T, K>,
        operator: CustomOperator,
        values: I,
    ) -> Self
    where
        T: IntoScalarValue,
        K: RowPhaseKind,
        I: IntoIterator<Item = T>,
    {
        self.custom_values(field, operator, values)
    }

    /// Applies a server-owned operator whose input type may differ from the field.
    #[must_use]
    pub fn custom_values<T, K: RowPhaseKind, V: IntoScalarValue>(
        self,
        field: FieldRef<E, T, K>,
        operator: CustomOperator,
        values: impl IntoIterator<Item = V>,
    ) -> Self {
        self.push_condition(
            field.id(),
            Operator::Custom(operator),
            values
                .into_iter()
                .map(IntoScalarValue::into_scalar_value)
                .collect(),
            false,
        )
    }

    /// Adds a trusted correlated `EXISTS` relation filter. The operator must
    /// have been created with [`CustomOperator::exists`].
    #[must_use]
    pub fn exists<T, K: RowPhaseKind, V: IntoScalarValue>(
        self,
        field: FieldRef<E, T, K>,
        operator: CustomOperator,
        values: impl IntoIterator<Item = V>,
    ) -> Self {
        if !operator.is_exists() {
            return self.push_error(ParamError::InvalidValue {
                field: "exists".to_owned(),
                reason: "operator must be declared with CustomOperator::exists".to_owned(),
            });
        }
        self.custom_values(field, operator, values)
    }

    /// Adds a trusted correlated `NOT EXISTS` relation filter. The operator
    /// must have been created with [`CustomOperator::not_exists`].
    #[must_use]
    pub fn not_exists<T, K: RowPhaseKind, V: IntoScalarValue>(
        self,
        field: FieldRef<E, T, K>,
        operator: CustomOperator,
        values: impl IntoIterator<Item = V>,
    ) -> Self {
        if !operator.is_not_exists() {
            return self.push_error(ParamError::InvalidValue {
                field: "not_exists".to_owned(),
                reason: "operator must be declared with CustomOperator::not_exists".to_owned(),
            });
        }
        self.custom_values(field, operator, values)
    }

    fn push_error(mut self, error: ParamError) -> Self {
        if self.error.is_none() {
            self.error = Some(error);
        }
        self
    }

    fn push_condition(
        mut self,
        field: FieldId,
        operator: Operator,
        values: Vec<ScalarValue>,
        ignore_case: bool,
    ) -> Self {
        if self.error.is_some() {
            return self;
        }
        let Some(meta) = E::META.field(field) else {
            self.error = Some(ParamError::UnknownField(format!("#{}", field.0)));
            return self;
        };
        if meta.kind == FieldKind::Aggregate
            || !operator.accepts_field_type(meta.value_type)
            || values
                .iter()
                .any(|value| value.value_type() != operator.input_type(meta.value_type))
        {
            self.error = Some(ParamError::InvalidValue {
                field: meta.public_name.to_owned(),
                reason: "scope field type or phase mismatch".to_owned(),
            });
        } else if !operator.accepts_arity(values.len()) {
            self.error = Some(ParamError::WrongArity {
                field: meta.public_name.to_owned(),
                operator: operator.short_name(),
                expected: operator.expected_arity(),
                actual: values.len(),
            });
        } else if ignore_case
            && (meta.value_type != crate::ValueType::Text || !operator.supports_ignore_case())
        {
            self.error = Some(ParamError::IgnoreCaseNotAllowed(
                meta.public_name.to_owned(),
            ));
        } else {
            if let Some(error) = values.iter().find_map(|value| value.validate().err()) {
                self.error = Some(error);
                return self;
            }
            self.conditions.push(Condition {
                field,
                operator,
                values,
                ignore_case,
                phase: PredicatePhase::Row,
            });
        }
        self
    }

    pub fn build(self) -> Result<ServerScope<E>, ParamError> {
        if let Some(error) = self.error {
            return Err(error);
        }
        if self.conditions.is_empty() {
            return Err(ParamError::InvalidValue {
                field: "server scope".to_owned(),
                reason: "empty scope must use ServerScope::unrestricted() explicitly".to_owned(),
            });
        }
        validate_conditions(self.conditions.iter(), self.limits)?;
        Ok(ServerScope {
            predicate: PredicateArena::combine_and(
                self.conditions
                    .into_iter()
                    .map(PredicateArena::from_condition),
            ),
            schema_fingerprint: E::META.schema_fingerprint,
            field_access: crate::FieldAccessPolicy::unrestricted(),
            _entity: PhantomData,
        })
    }
}
