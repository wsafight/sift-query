use crate::{CustomOperator, FieldRef, IntoScalarValue, Operator, ScalarValue, SearchEntity};

use super::QueryBuilder;

impl<E: SearchEntity> QueryBuilder<'_, E> {
    #[must_use]
    pub fn eq<T: IntoScalarValue, K>(self, field: FieldRef<E, T, K>, value: T) -> Self {
        self.push_condition(
            field.id(),
            Operator::Eq,
            vec![value.into_scalar_value()],
            false,
        )
    }

    #[must_use]
    pub fn ne<T: IntoScalarValue, K>(self, field: FieldRef<E, T, K>, value: T) -> Self {
        self.push_condition(
            field.id(),
            Operator::Ne,
            vec![value.into_scalar_value()],
            false,
        )
    }

    #[must_use]
    pub fn gt<T: IntoScalarValue, K>(self, field: FieldRef<E, T, K>, value: T) -> Self {
        self.push_condition(
            field.id(),
            Operator::Gt,
            vec![value.into_scalar_value()],
            false,
        )
    }

    #[must_use]
    pub fn ge<T: IntoScalarValue, K>(self, field: FieldRef<E, T, K>, value: T) -> Self {
        self.push_condition(
            field.id(),
            Operator::Ge,
            vec![value.into_scalar_value()],
            false,
        )
    }

    #[must_use]
    pub fn lt<T: IntoScalarValue, K>(self, field: FieldRef<E, T, K>, value: T) -> Self {
        self.push_condition(
            field.id(),
            Operator::Lt,
            vec![value.into_scalar_value()],
            false,
        )
    }

    #[must_use]
    pub fn le<T: IntoScalarValue, K>(self, field: FieldRef<E, T, K>, value: T) -> Self {
        self.push_condition(
            field.id(),
            Operator::Le,
            vec![value.into_scalar_value()],
            false,
        )
    }

    #[must_use]
    pub fn between<T: IntoScalarValue, K>(
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
    pub fn not_between<T: IntoScalarValue, K>(
        self,
        field: FieldRef<E, T, K>,
        lower: T,
        upper: T,
    ) -> Self {
        self.push_condition(
            field.id(),
            Operator::NotBetween,
            vec![lower.into_scalar_value(), upper.into_scalar_value()],
            false,
        )
    }

    #[must_use]
    pub fn in_list<T, K, I>(self, field: FieldRef<E, T, K>, values: I) -> Self
    where
        T: IntoScalarValue,
        I: IntoIterator<Item = T>,
    {
        self.list(field, Operator::InList, values)
    }

    #[must_use]
    pub fn not_in<T, K, I>(self, field: FieldRef<E, T, K>, values: I) -> Self
    where
        T: IntoScalarValue,
        I: IntoIterator<Item = T>,
    {
        self.list(field, Operator::NotIn, values)
    }

    #[must_use]
    pub fn is_null<T, K>(self, field: FieldRef<E, T, K>) -> Self {
        self.push_condition(field.id(), Operator::IsNull, Vec::new(), false)
    }

    #[must_use]
    pub fn is_not_null<T, K>(self, field: FieldRef<E, T, K>) -> Self {
        self.push_condition(field.id(), Operator::NotNull, Vec::new(), false)
    }

    #[must_use]
    pub fn empty<K>(self, field: FieldRef<E, String, K>) -> Self {
        self.push_condition(field.id(), Operator::Empty, Vec::new(), false)
    }

    #[must_use]
    pub fn not_empty<K>(self, field: FieldRef<E, String, K>) -> Self {
        self.push_condition(field.id(), Operator::NotEmpty, Vec::new(), false)
    }

    #[must_use]
    pub fn always_true<T, K>(self, field: FieldRef<E, T, K>) -> Self {
        self.push_condition(field.id(), Operator::AlwaysTrue, Vec::new(), false)
    }

    #[must_use]
    pub fn always_false<T, K>(self, field: FieldRef<E, T, K>) -> Self {
        self.push_condition(field.id(), Operator::AlwaysFalse, Vec::new(), false)
    }

    #[must_use]
    pub fn eq_ignore_case<K>(
        self,
        field: FieldRef<E, String, K>,
        value: impl Into<String>,
    ) -> Self {
        self.text(field, Operator::Eq, value, true)
    }

    #[must_use]
    pub fn ne_ignore_case<K>(
        self,
        field: FieldRef<E, String, K>,
        value: impl Into<String>,
    ) -> Self {
        self.text(field, Operator::Ne, value, true)
    }

    #[must_use]
    pub fn starts_with<K>(self, field: FieldRef<E, String, K>, value: impl Into<String>) -> Self {
        self.text(field, Operator::StartWith, value, false)
    }

    #[must_use]
    pub fn starts_with_ignore_case<K>(
        self,
        field: FieldRef<E, String, K>,
        value: impl Into<String>,
    ) -> Self {
        self.text(field, Operator::StartWith, value, true)
    }

    #[must_use]
    pub fn ends_with<K>(self, field: FieldRef<E, String, K>, value: impl Into<String>) -> Self {
        self.text(field, Operator::EndWith, value, false)
    }

    #[must_use]
    pub fn contains<K>(self, field: FieldRef<E, String, K>, value: impl Into<String>) -> Self {
        self.text(field, Operator::Contain, value, false)
    }

    #[must_use]
    pub fn contains_ignore_case<K>(
        self,
        field: FieldRef<E, String, K>,
        value: impl Into<String>,
    ) -> Self {
        self.text(field, Operator::Contain, value, true)
    }

    #[must_use]
    pub fn not_like<K>(self, field: FieldRef<E, String, K>, pattern: impl Into<String>) -> Self {
        self.text(field, Operator::NotLike, pattern, false)
    }

    #[must_use]
    pub fn or_like<K, I, S>(self, field: FieldRef<E, String, K>, patterns: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.push_condition(
            field.id(),
            Operator::OrLike,
            patterns
                .into_iter()
                .map(|value| ScalarValue::Text(value.into()))
                .collect(),
            false,
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
        I: IntoIterator<Item = T>,
    {
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

    /// Typed custom input may differ from the field's returned Rust type.
    #[must_use]
    pub fn custom_values<T, K, V: IntoScalarValue>(
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

    fn list<T, K, I>(self, field: FieldRef<E, T, K>, operator: Operator, values: I) -> Self
    where
        T: IntoScalarValue,
        I: IntoIterator<Item = T>,
    {
        self.push_condition(
            field.id(),
            operator,
            values
                .into_iter()
                .map(IntoScalarValue::into_scalar_value)
                .collect(),
            false,
        )
    }

    fn text<K>(
        self,
        field: FieldRef<E, String, K>,
        operator: Operator,
        value: impl Into<String>,
        ignore_case: bool,
    ) -> Self {
        self.push_condition(
            field.id(),
            operator,
            vec![ScalarValue::Text(value.into())],
            ignore_case,
        )
    }
}
