use std::marker::PhantomData;

use crate::{
    Condition, CustomOperator, FieldKind, FieldRef, IntoScalarValue, Operator, ParamError,
    PredicateArena, PredicatePhase, ScalarValue, SearchEntity, ValueType,
};

/// Entity-bound predicate expression for programmatic queries.
#[derive(Debug)]
pub struct FilterExpr<E: SearchEntity> {
    pub(crate) arena: PredicateArena,
    pub(crate) condition_count: usize,
    pub(crate) logical_values: usize,
    schema_fingerprint: u64,
    _entity: PhantomData<fn() -> E>,
}

impl<E: SearchEntity> FilterExpr<E> {
    pub fn eq<T: IntoScalarValue, K>(
        field: FieldRef<E, T, K>,
        value: T,
    ) -> Result<Self, ParamError> {
        Self::condition(field, Operator::Eq, vec![value.into_scalar_value()], false)
    }

    pub fn ne<T: IntoScalarValue, K>(
        field: FieldRef<E, T, K>,
        value: T,
    ) -> Result<Self, ParamError> {
        Self::condition(field, Operator::Ne, vec![value.into_scalar_value()], false)
    }

    pub fn gt<T: IntoScalarValue, K>(
        field: FieldRef<E, T, K>,
        value: T,
    ) -> Result<Self, ParamError> {
        Self::condition(field, Operator::Gt, vec![value.into_scalar_value()], false)
    }

    pub fn ge<T: IntoScalarValue, K>(
        field: FieldRef<E, T, K>,
        value: T,
    ) -> Result<Self, ParamError> {
        Self::condition(field, Operator::Ge, vec![value.into_scalar_value()], false)
    }

    pub fn lt<T: IntoScalarValue, K>(
        field: FieldRef<E, T, K>,
        value: T,
    ) -> Result<Self, ParamError> {
        Self::condition(field, Operator::Lt, vec![value.into_scalar_value()], false)
    }

    pub fn le<T: IntoScalarValue, K>(
        field: FieldRef<E, T, K>,
        value: T,
    ) -> Result<Self, ParamError> {
        Self::condition(field, Operator::Le, vec![value.into_scalar_value()], false)
    }

    pub fn between<T: IntoScalarValue, K>(
        field: FieldRef<E, T, K>,
        lower: T,
        upper: T,
    ) -> Result<Self, ParamError> {
        Self::condition(
            field,
            Operator::Between,
            vec![lower.into_scalar_value(), upper.into_scalar_value()],
            false,
        )
    }

    pub fn in_list<T, K, I>(field: FieldRef<E, T, K>, values: I) -> Result<Self, ParamError>
    where
        T: IntoScalarValue,
        I: IntoIterator<Item = T>,
    {
        Self::condition(
            field,
            Operator::InList,
            values
                .into_iter()
                .map(IntoScalarValue::into_scalar_value)
                .collect(),
            false,
        )
    }

    pub fn is_null<T, K>(field: FieldRef<E, T, K>) -> Result<Self, ParamError> {
        Self::condition(field, Operator::IsNull, Vec::new(), false)
    }

    pub fn is_not_null<T, K>(field: FieldRef<E, T, K>) -> Result<Self, ParamError> {
        Self::condition(field, Operator::NotNull, Vec::new(), false)
    }

    pub fn starts_with<K>(
        field: FieldRef<E, String, K>,
        value: impl Into<String>,
    ) -> Result<Self, ParamError> {
        Self::condition(
            field,
            Operator::StartWith,
            vec![ScalarValue::Text(value.into())],
            false,
        )
    }

    pub fn eq_ignore_case<K>(
        field: FieldRef<E, String, K>,
        value: impl Into<String>,
    ) -> Result<Self, ParamError> {
        Self::condition(
            field,
            Operator::Eq,
            vec![ScalarValue::Text(value.into())],
            true,
        )
    }

    pub fn ne_ignore_case<K>(
        field: FieldRef<E, String, K>,
        value: impl Into<String>,
    ) -> Result<Self, ParamError> {
        Self::condition(
            field,
            Operator::Ne,
            vec![ScalarValue::Text(value.into())],
            true,
        )
    }

    pub fn starts_with_ignore_case<K>(
        field: FieldRef<E, String, K>,
        value: impl Into<String>,
    ) -> Result<Self, ParamError> {
        Self::condition(
            field,
            Operator::StartWith,
            vec![ScalarValue::Text(value.into())],
            true,
        )
    }

    pub fn contains<K>(
        field: FieldRef<E, String, K>,
        value: impl Into<String>,
    ) -> Result<Self, ParamError> {
        Self::condition(
            field,
            Operator::Contain,
            vec![ScalarValue::Text(value.into())],
            false,
        )
    }

    pub fn contains_ignore_case<K>(
        field: FieldRef<E, String, K>,
        value: impl Into<String>,
    ) -> Result<Self, ParamError> {
        Self::condition(
            field,
            Operator::Contain,
            vec![ScalarValue::Text(value.into())],
            true,
        )
    }

    pub fn custom<T, K, I>(
        field: FieldRef<E, T, K>,
        operator: CustomOperator,
        values: I,
    ) -> Result<Self, ParamError>
    where
        T: IntoScalarValue,
        I: IntoIterator<Item = T>,
    {
        Self::condition(
            field,
            Operator::Custom(operator),
            values
                .into_iter()
                .map(IntoScalarValue::into_scalar_value)
                .collect(),
            false,
        )
    }

    pub fn custom_values<T, K, V: IntoScalarValue>(
        field: FieldRef<E, T, K>,
        operator: CustomOperator,
        values: impl IntoIterator<Item = V>,
    ) -> Result<Self, ParamError> {
        Self::condition(
            field,
            Operator::Custom(operator),
            values
                .into_iter()
                .map(IntoScalarValue::into_scalar_value)
                .collect(),
            false,
        )
    }

    pub fn all(expressions: impl IntoIterator<Item = Self>) -> Result<Self, ParamError> {
        Self::combine(expressions, true)
    }

    pub fn any(expressions: impl IntoIterator<Item = Self>) -> Result<Self, ParamError> {
        Self::combine(expressions, false)
    }

    fn condition<T, K>(
        field: FieldRef<E, T, K>,
        operator: Operator,
        values: Vec<ScalarValue>,
        ignore_case: bool,
    ) -> Result<Self, ParamError> {
        let meta = E::META
            .field(field.id())
            .ok_or_else(|| ParamError::UnknownField(format!("#{}", field.id().0)))?;
        if !meta.filterable {
            return Err(ParamError::NotFilterable(meta.public_name.to_owned()));
        }
        if !matches!(operator, Operator::Custom(_)) && !meta.operators.contains(operator) {
            return Err(ParamError::OperatorNotAllowed {
                field: meta.public_name.to_owned(),
                operator: operator.short_name(),
            });
        }
        if !operator.accepts_arity(values.len()) {
            return Err(ParamError::WrongArity {
                field: meta.public_name.to_owned(),
                operator: operator.short_name(),
                expected: operator.expected_arity(),
                actual: values.len(),
            });
        }
        if !operator.accepts_field_type(meta.value_type)
            || values
                .iter()
                .any(|value| value.value_type() != operator.input_type(meta.value_type))
        {
            return Err(ParamError::InvalidValue {
                field: meta.public_name.to_owned(),
                reason: "programmatic value type does not match field".to_owned(),
            });
        }
        if ignore_case && (meta.value_type != ValueType::Text || !operator.supports_ignore_case()) {
            return Err(ParamError::IgnoreCaseNotAllowed(
                meta.public_name.to_owned(),
            ));
        }
        for value in &values {
            value.validate()?;
        }
        let logical_values = values.iter().map(ScalarValue::logical_weight).sum();
        let condition = Condition {
            field: field.id(),
            operator,
            values,
            ignore_case,
            phase: if meta.kind == FieldKind::Aggregate {
                PredicatePhase::Group
            } else {
                PredicatePhase::Row
            },
        };
        Ok(Self {
            logical_values,
            arena: PredicateArena::from_condition(condition),
            condition_count: 1,
            schema_fingerprint: E::META.schema_fingerprint,
            _entity: PhantomData,
        })
    }

    fn combine(expressions: impl IntoIterator<Item = Self>, and: bool) -> Result<Self, ParamError> {
        let expressions = expressions.into_iter().collect::<Vec<_>>();
        if expressions.is_empty() {
            return Err(ParamError::InvalidValue {
                field: "predicate".to_owned(),
                reason: "predicate composition must not be empty".to_owned(),
            });
        }
        if expressions
            .iter()
            .any(|expr| expr.schema_fingerprint != E::META.schema_fingerprint)
        {
            return Err(ParamError::UnknownField(
                "entity schema mismatch".to_owned(),
            ));
        }
        let condition_count = expressions.iter().map(|expr| expr.condition_count).sum();
        let logical_values = expressions.iter().map(|expr| expr.logical_values).sum();
        let arenas = expressions.into_iter().map(|expr| expr.arena);
        let arena = if and {
            PredicateArena::all(arenas)
        } else {
            PredicateArena::any(arenas)
        };
        arena.partition()?;
        Ok(Self {
            arena,
            condition_count,
            logical_values,
            schema_fingerprint: E::META.schema_fingerprint,
            _entity: PhantomData,
        })
    }
}
