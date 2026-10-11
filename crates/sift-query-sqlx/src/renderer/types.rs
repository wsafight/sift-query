use sift_query_core::{AggregateFn, ValueType};

pub(super) fn bind_type(value_type: ValueType) -> Result<&'static str, crate::RenderError> {
    Ok(match value_type {
        ValueType::Bool => "boolean",
        ValueType::I32 => "integer",
        ValueType::I64 => "bigint",
        ValueType::F32 => "real",
        ValueType::F64 => "double precision",
        ValueType::Text => "text",
        ValueType::Bytes => "bytea",
        #[cfg(feature = "decimal")]
        ValueType::Decimal => "numeric",
        #[cfg(feature = "chrono")]
        ValueType::Date => "date",
        #[cfg(feature = "chrono")]
        ValueType::Time => "time",
        #[cfg(feature = "chrono")]
        ValueType::DateTime => "timestamp",
        #[cfg(feature = "chrono")]
        ValueType::Timestamp => "timestamptz",
        #[cfg(feature = "uuid")]
        ValueType::Uuid => "uuid",
        #[cfg(feature = "json")]
        ValueType::Json => "jsonb",
        #[cfg(feature = "postgres-types")]
        ValueType::ArrayI64 => "bigint[]",
        #[cfg(feature = "postgres-types")]
        ValueType::ArrayText => "text[]",
        #[cfg(feature = "postgres-types")]
        ValueType::Int8Range => "int8range",
        #[cfg(feature = "postgres-types")]
        ValueType::Network => "inet",
        _ => {
            return Err(crate::RenderError::UnsupportedBindType(format!(
                "{value_type:?}"
            )));
        }
    })
}

pub(super) fn supports_array(value_type: ValueType) -> bool {
    match value_type {
        ValueType::Bool
        | ValueType::I32
        | ValueType::I64
        | ValueType::F32
        | ValueType::F64
        | ValueType::Text => true,
        #[cfg(feature = "decimal")]
        ValueType::Decimal => true,
        #[cfg(feature = "chrono")]
        ValueType::Date | ValueType::Time | ValueType::DateTime | ValueType::Timestamp => true,
        #[cfg(feature = "uuid")]
        ValueType::Uuid => true,
        _ => false,
    }
}

pub(super) fn supports_aggregate(function: AggregateFn, value_type: ValueType) -> bool {
    match function {
        AggregateFn::Sum => match value_type {
            ValueType::I32 | ValueType::F32 | ValueType::F64 => true,
            #[cfg(feature = "decimal")]
            ValueType::I64 | ValueType::Decimal => true,
            _ => false,
        },
        AggregateFn::Avg => match value_type {
            ValueType::F32 | ValueType::F64 => true,
            #[cfg(feature = "decimal")]
            ValueType::I32 | ValueType::I64 | ValueType::Decimal => true,
            _ => false,
        },
        AggregateFn::Min | AggregateFn::Max => match value_type {
            ValueType::I32 | ValueType::I64 | ValueType::F32 | ValueType::F64 | ValueType::Text => {
                true
            }
            #[cfg(feature = "decimal")]
            ValueType::Decimal => true,
            #[cfg(feature = "chrono")]
            ValueType::Date | ValueType::Time | ValueType::DateTime | ValueType::Timestamp => true,
            #[cfg(feature = "uuid")]
            ValueType::Uuid => true,
            _ => false,
        },
        AggregateFn::Custom(_) => true,
    }
}

pub(super) fn render_aggregate(function: AggregateFn, field: &str) -> String {
    match function {
        AggregateFn::Sum => format!("sum({field})"),
        AggregateFn::Avg => format!("avg({field})"),
        AggregateFn::Min => format!("min({field})"),
        AggregateFn::Max => format!("max({field})"),
        AggregateFn::Custom(aggregate) => aggregate.template().replace("{field}", field),
    }
}
#[cfg(test)]
mod tests;
