pub trait PgSummaryValue {}
impl PgSummaryValue for i64 {}
impl PgSummaryValue for f32 {}
impl PgSummaryValue for f64 {}
#[cfg(feature = "decimal")]
impl PgSummaryValue for rust_decimal::Decimal {}

pub trait PgSumType {
    type Output: PgSummaryValue;
}

impl PgSumType for i32 {
    type Output = i64;
}

#[cfg(feature = "decimal")]
impl PgSumType for i64 {
    type Output = rust_decimal::Decimal;
}

#[cfg(feature = "decimal")]
impl PgSumType for rust_decimal::Decimal {
    type Output = rust_decimal::Decimal;
}

impl PgSumType for f32 {
    type Output = f32;
}

impl PgSumType for f64 {
    type Output = f64;
}
