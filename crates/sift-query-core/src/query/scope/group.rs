use super::{ScopeLimits, validate_conditions, validate_predicates};
use crate::{
    Condition, FieldId, FieldKind, FieldRef, GroupScope, IntoScalarValue, Operator, ParamError,
    PredicateArena, PredicatePhase, ScalarValue, SearchEntity,
};
use std::marker::PhantomData;

/// Builder for service-owned aggregate predicates evaluated in HAVING.
pub struct GroupScopeBuilder<E: SearchEntity> {
    conditions: Vec<Condition>,
    limits: ScopeLimits,
    error: Option<ParamError>,
    _entity: PhantomData<fn() -> E>,
}

impl<E: SearchEntity> GroupScope<E> {
    /// Combines service-owned HAVING scopes with OR.
    pub fn any(scopes: impl IntoIterator<Item = Self>) -> Result<Self, ParamError> {
        Self::any_with_limits(scopes, ScopeLimits::default())
    }
    #[must_use]
    pub fn new() -> GroupScopeBuilder<E> {
        Self::builder(ScopeLimits::default())
    }

    #[must_use]
    pub fn builder(limits: ScopeLimits) -> GroupScopeBuilder<E> {
        GroupScopeBuilder {
            conditions: Vec::new(),
            limits,
            error: None,
            _entity: PhantomData,
        }
    }

    pub fn all(scopes: impl IntoIterator<Item = Self>) -> Result<Self, ParamError> {
        Self::all_with_limits(scopes, ScopeLimits::default())
    }

    pub fn all_with_limits(
        scopes: impl IntoIterator<Item = Self>,
        limits: ScopeLimits,
    ) -> Result<Self, ParamError> {
        let scopes = scopes.into_iter().collect::<Vec<_>>();
        if scopes.is_empty() {
            return Err(ParamError::InvalidValue {
                field: "group scope".to_owned(),
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
        Ok(Self {
            predicate: PredicateArena::combine_and(scopes.into_iter().map(|scope| scope.predicate)),
            schema_fingerprint: E::META.schema_fingerprint,
            _entity: PhantomData,
        })
    }

    pub fn any_with_limits(
        scopes: impl IntoIterator<Item = Self>,
        limits: ScopeLimits,
    ) -> Result<Self, ParamError> {
        let scopes = scopes.into_iter().collect::<Vec<_>>();
        if scopes.is_empty() {
            return Err(ParamError::InvalidValue {
                field: "group scope".to_owned(),
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
        Ok(Self {
            predicate: PredicateArena::combine_or(scopes.into_iter().map(|scope| scope.predicate)),
            schema_fingerprint: E::META.schema_fingerprint,
            _entity: PhantomData,
        })
    }
}

impl<E: SearchEntity> GroupScopeBuilder<E> {
    #[must_use]
    pub fn eq<T: IntoScalarValue>(
        self,
        field: FieldRef<E, T, crate::AggregateKind>,
        value: T,
    ) -> Self {
        self.push_condition(field.id(), Operator::Eq, vec![value.into_scalar_value()])
    }

    #[must_use]
    pub fn ne<T: IntoScalarValue>(
        self,
        field: FieldRef<E, T, crate::AggregateKind>,
        value: T,
    ) -> Self {
        self.push_condition(field.id(), Operator::Ne, vec![value.into_scalar_value()])
    }

    #[must_use]
    pub fn gt<T: IntoScalarValue>(
        self,
        field: FieldRef<E, T, crate::AggregateKind>,
        value: T,
    ) -> Self {
        self.push_condition(field.id(), Operator::Gt, vec![value.into_scalar_value()])
    }

    #[must_use]
    pub fn ge<T: IntoScalarValue>(
        self,
        field: FieldRef<E, T, crate::AggregateKind>,
        value: T,
    ) -> Self {
        self.push_condition(field.id(), Operator::Ge, vec![value.into_scalar_value()])
    }

    #[must_use]
    pub fn lt<T: IntoScalarValue>(
        self,
        field: FieldRef<E, T, crate::AggregateKind>,
        value: T,
    ) -> Self {
        self.push_condition(field.id(), Operator::Lt, vec![value.into_scalar_value()])
    }

    #[must_use]
    pub fn le<T: IntoScalarValue>(
        self,
        field: FieldRef<E, T, crate::AggregateKind>,
        value: T,
    ) -> Self {
        self.push_condition(field.id(), Operator::Le, vec![value.into_scalar_value()])
    }

    #[must_use]
    pub fn between<T: IntoScalarValue>(
        self,
        field: FieldRef<E, T, crate::AggregateKind>,
        lower: T,
        upper: T,
    ) -> Self {
        self.push_condition(
            field.id(),
            Operator::Between,
            vec![lower.into_scalar_value(), upper.into_scalar_value()],
        )
    }

    #[must_use]
    pub fn in_list<T, I>(self, field: FieldRef<E, T, crate::AggregateKind>, values: I) -> Self
    where
        T: IntoScalarValue,
        I: IntoIterator<Item = T>,
    {
        self.push_condition(
            field.id(),
            Operator::InList,
            values
                .into_iter()
                .map(IntoScalarValue::into_scalar_value)
                .collect(),
        )
    }

    fn push_condition(
        mut self,
        field: FieldId,
        operator: Operator,
        values: Vec<ScalarValue>,
    ) -> Self {
        if self.error.is_some() {
            return self;
        }
        let Some(meta) = E::META.field(field) else {
            self.error = Some(ParamError::UnknownField(format!("#{}", field.0)));
            return self;
        };
        if meta.kind != FieldKind::Aggregate
            || !operator.accepts_field_type(meta.value_type)
            || values
                .iter()
                .any(|value| value.value_type() != operator.input_type(meta.value_type))
        {
            self.error = Some(ParamError::InvalidValue {
                field: meta.public_name.to_owned(),
                reason: "group scope requires an aggregate field with a matching type".to_owned(),
            });
        } else if !meta.operators.contains(operator) {
            self.error = Some(ParamError::OperatorNotAllowed {
                field: meta.public_name.to_owned(),
                operator: operator.short_name(),
            });
        } else if !operator.accepts_arity(values.len()) {
            self.error = Some(ParamError::WrongArity {
                field: meta.public_name.to_owned(),
                operator: operator.short_name(),
                expected: operator.expected_arity(),
                actual: values.len(),
            });
        } else {
            if let Some(error) = values.iter().find_map(|value| value.validate().err()) {
                self.error = Some(error);
                return self;
            }
            self.conditions.push(Condition {
                field,
                operator,
                values,
                ignore_case: false,
                phase: PredicatePhase::Group,
            });
        }
        self
    }

    pub fn build(self) -> Result<GroupScope<E>, ParamError> {
        if let Some(error) = self.error {
            return Err(error);
        }
        if self.conditions.is_empty() {
            return Err(ParamError::InvalidValue {
                field: "group scope".to_owned(),
                reason: "empty scope must use GroupScope::unrestricted() explicitly".to_owned(),
            });
        }
        validate_conditions(self.conditions.iter(), self.limits)?;
        Ok(GroupScope {
            predicate: PredicateArena::combine_and(
                self.conditions
                    .into_iter()
                    .map(PredicateArena::from_condition),
            ),
            schema_fingerprint: E::META.schema_fingerprint,
            _entity: PhantomData,
        })
    }
}
