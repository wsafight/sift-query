use std::marker::PhantomData;

use crate::{FieldId, FieldRef, IntoScalarValue, ParamError, ScalarValue, SearchEntity};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TimeBucketUnit {
    Hour,
    Day,
    Week,
    Month,
}

impl TimeBucketUnit {
    #[must_use]
    pub const fn sql_name(self) -> &'static str {
        match self {
            Self::Hour => "hour",
            Self::Day => "day",
            Self::Week => "week",
            Self::Month => "month",
        }
    }
}

#[derive(Debug, Clone)]
pub struct TimeBucketRequest<E: SearchEntity> {
    field: FieldId,
    schema_fingerprint: u64,
    unit: TimeBucketUnit,
    timezone: &'static str,
    _entity: PhantomData<fn() -> E>,
}

impl<E: SearchEntity> TimeBucketRequest<E> {
    pub fn new<T, K>(
        field: FieldRef<E, T, K>,
        unit: TimeBucketUnit,
        timezone: &'static str,
    ) -> Result<Self, ParamError> {
        if E::META.field(field.id()).is_none() {
            return Err(ParamError::UnknownField(format!("#{}", field.id().0)));
        }
        if timezone.is_empty()
            || timezone.contains('\'')
            || timezone.contains(';')
            || timezone.contains("--")
            || timezone.contains("/*")
        {
            return Err(ParamError::InvalidValue {
                field: "timezone".to_owned(),
                reason: "timezone contains an unsafe token".to_owned(),
            });
        }
        Ok(Self {
            field: field.id(),
            schema_fingerprint: E::META.schema_fingerprint,
            unit,
            timezone,
            _entity: PhantomData,
        })
    }

    #[must_use]
    pub const fn field_id(&self) -> FieldId {
        self.field
    }
    #[must_use]
    pub const fn schema_fingerprint(&self) -> u64 {
        self.schema_fingerprint
    }
    #[must_use]
    pub const fn unit(&self) -> TimeBucketUnit {
        self.unit
    }
    #[must_use]
    pub const fn timezone(&self) -> &'static str {
        self.timezone
    }
}

#[derive(Debug, Clone)]
pub struct NumericBucketRequest<E: SearchEntity> {
    field: FieldId,
    schema_fingerprint: u64,
    boundaries: Vec<ScalarValue>,
    _entity: PhantomData<fn() -> E>,
}

impl<E: SearchEntity> NumericBucketRequest<E> {
    pub fn new<T, K, V, I>(field: FieldRef<E, T, K>, boundaries: I) -> Result<Self, ParamError>
    where
        V: IntoScalarValue,
        I: IntoIterator<Item = V>,
    {
        let boundaries = boundaries
            .into_iter()
            .map(IntoScalarValue::into_scalar_value)
            .collect::<Vec<_>>();
        if boundaries.is_empty() || boundaries.len() > 1_000 {
            return Err(ParamError::InvalidValue {
                field: "bucket.boundaries".to_owned(),
                reason: "boundaries must contain between 1 and 1000 values".to_owned(),
            });
        }
        let value_type = boundaries[0].value_type();
        let Some(field_meta) = E::META.field(field.id()) else {
            return Err(ParamError::UnknownField(format!("#{}", field.id().0)));
        };
        if !matches!(
            value_type,
            crate::ValueType::I32
                | crate::ValueType::I64
                | crate::ValueType::F32
                | crate::ValueType::F64
        ) || field_meta.value_type != value_type
            || boundaries
                .iter()
                .any(|value| value.value_type() != value_type)
        {
            return Err(ParamError::InvalidValue {
                field: "bucket.boundaries".to_owned(),
                reason: "numeric boundaries must use one numeric value type".to_owned(),
            });
        }
        if boundaries
            .windows(2)
            .any(|pair| !scalar_lt(&pair[0], &pair[1]))
        {
            return Err(ParamError::InvalidValue {
                field: "bucket.boundaries".to_owned(),
                reason: "boundaries must be strictly increasing".to_owned(),
            });
        }
        Ok(Self {
            field: field.id(),
            schema_fingerprint: E::META.schema_fingerprint,
            boundaries,
            _entity: PhantomData,
        })
    }

    #[must_use]
    pub const fn field_id(&self) -> FieldId {
        self.field
    }
    #[must_use]
    pub const fn schema_fingerprint(&self) -> u64 {
        self.schema_fingerprint
    }
    #[must_use]
    pub fn boundaries(&self) -> &[ScalarValue] {
        &self.boundaries
    }
}

#[derive(Debug, Clone)]
pub enum BucketRequest<E: SearchEntity> {
    Time(TimeBucketRequest<E>),
    Numeric(NumericBucketRequest<E>),
}

impl<E: SearchEntity> BucketRequest<E> {
    #[must_use]
    pub const fn field_id(&self) -> FieldId {
        match self {
            Self::Time(request) => request.field_id(),
            Self::Numeric(request) => request.field_id(),
        }
    }

    #[must_use]
    pub const fn schema_fingerprint(&self) -> u64 {
        match self {
            Self::Time(request) => request.schema_fingerprint(),
            Self::Numeric(request) => request.schema_fingerprint(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct BucketEntry {
    pub label: String,
    pub count: i64,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct BucketResult {
    pub field: &'static str,
    pub buckets: Vec<BucketEntry>,
}

fn scalar_lt(left: &ScalarValue, right: &ScalarValue) -> bool {
    match (left, right) {
        (ScalarValue::I32(left), ScalarValue::I32(right)) => left < right,
        (ScalarValue::I64(left), ScalarValue::I64(right)) => left < right,
        (ScalarValue::F32(left), ScalarValue::F32(right)) => left < right,
        (ScalarValue::F64(left), ScalarValue::F64(right)) => left < right,
        _ => false,
    }
}

#[doc(hidden)]
pub fn numeric_label(boundaries: &[ScalarValue], index: usize) -> String {
    if index == 0 {
        format!("<{}", boundaries[0].to_external_text())
    } else if index < boundaries.len() {
        format!(
            "[{}, {})",
            boundaries[index - 1].to_external_text(),
            boundaries[index].to_external_text()
        )
    } else {
        format!(">={}", boundaries[boundaries.len() - 1].to_external_text())
    }
}
