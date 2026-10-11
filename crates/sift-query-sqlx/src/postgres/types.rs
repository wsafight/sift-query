use sift_query_core::{Int8Range, IntoScalarValue, ScalarValue};
use sqlx::encode::IsNull;
use sqlx::error::BoxDynError;
use sqlx::postgres::types::PgRange;
use sqlx::postgres::{PgArgumentBuffer, PgTypeInfo, PgValueFormat, PgValueRef};
use sqlx::{Decode, Encode, Postgres, Type};
use std::ops::Bound;

/// sqlx result wrapper retaining the distinction between empty and unbounded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PgInt8Range(pub Int8Range);
impl IntoScalarValue for PgInt8Range {
    fn into_scalar_value(self) -> ScalarValue {
        ScalarValue::Int8Range(Box::new(self.0))
    }
}
impl Type<Postgres> for PgInt8Range {
    fn type_info() -> PgTypeInfo {
        <PgRange<i64> as Type<Postgres>>::type_info()
    }
    fn compatible(ty: &PgTypeInfo) -> bool {
        <PgRange<i64> as Type<Postgres>>::compatible(ty)
    }
}
impl Encode<'_, Postgres> for PgInt8Range {
    fn encode_by_ref(&self, buffer: &mut PgArgumentBuffer) -> Result<IsNull, BoxDynError> {
        self.0.validate()?;
        match self.0 {
            Int8Range::Empty => {
                buffer.push(1);
                Ok(IsNull::No)
            }
            Int8Range::Bounds { lower, upper } => PgRange {
                start: lower.map_or(Bound::Unbounded, Bound::Included),
                end: upper.map_or(Bound::Unbounded, Bound::Excluded),
            }
            .encode_by_ref(buffer),
        }
    }
}
impl<'r> Decode<'r, Postgres> for PgInt8Range {
    fn decode(value: PgValueRef<'r>) -> Result<Self, BoxDynError> {
        if value.format() == PgValueFormat::Binary
            && value
                .as_bytes()?
                .first()
                .is_some_and(|flags| flags & 1 != 0)
        {
            return Ok(Self(Int8Range::Empty));
        }
        if value.format() == PgValueFormat::Text {
            return Ok(Self(value.as_str()?.parse()?));
        }
        let range = <PgRange<i64> as Decode<Postgres>>::decode(value)?;
        Ok(Self(Int8Range::new(range.start, range.end)?))
    }
}
#[cfg(feature = "serde")]
impl serde::Serialize for PgInt8Range {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0.to_string())
    }
}
