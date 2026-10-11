use super::typed;
use sift_query_core::{CustomOperator, ValueType};

use sift_query_core::ConfigError;

#[cfg(feature = "postgis")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpatialQuery {
    Intersects,
    Contains,
    Within,
}
/// Server-declared geometry expression/SRID; bound EWKT carries the input SRID.
#[cfg(feature = "postgis")]
pub fn spatial(mode: SpatialQuery) -> CustomOperator {
    match mode {
        SpatialQuery::Intersects => typed(
            "geo_intersects",
            "ST_Intersects({field}, ST_GeomFromEWKT({0}))",
            ValueType::Text,
            ValueType::Text,
        ),
        SpatialQuery::Contains => typed(
            "geo_contains",
            "ST_Contains({field}, ST_GeomFromEWKT({0}))",
            ValueType::Text,
            ValueType::Text,
        ),
        SpatialQuery::Within => typed(
            "geo_within",
            "ST_Within({field}, ST_GeomFromEWKT({0}))",
            ValueType::Text,
            ValueType::Text,
        ),
    }
}
/// Named input codec for a filter-only ltree field.
#[cfg(feature = "postgis")]
#[derive(Debug, Clone, Copy)]
/// Bound EWKT input with a server-declared SRID; no client SQL or casts.
pub struct EwktCodec {
    field: &'static str,
    srid: u32,
}
#[cfg(feature = "postgis")]
impl EwktCodec {
    pub fn new(field: &'static str, srid: u32) -> Result<Self, ConfigError> {
        if srid == 0 || srid > 999_999 {
            return Err(ConfigError::Invalid("invalid spatial SRID".to_owned()));
        }
        Ok(Self { field, srid })
    }
}
#[cfg(feature = "postgis")]
impl sift_query_core::Codec for EwktCodec {
    fn name(&self) -> &'static str {
        "ewkt"
    }
    fn supports(&self, field: &sift_query_core::FieldMeta) -> bool {
        field.value_type == ValueType::Text && field.public_name == self.field
    }
    fn parse(
        &self,
        _field: &sift_query_core::FieldMeta,
        input: &str,
    ) -> Result<sift_query_core::ScalarValue, String> {
        let (prefix, wkt) = input.split_once(';').ok_or("expected SRID=n;WKT")?;
        if prefix
            .strip_prefix("SRID=")
            .and_then(|value| value.parse::<u32>().ok())
            != Some(self.srid)
        {
            return Err("input SRID differs from server configuration".to_owned());
        }
        wkt.parse::<wkt::Wkt<f64>>()
            .map_err(|_| "invalid WKT geometry")?;
        for token in wkt.split(|ch: char| ch.is_whitespace() || matches!(ch, '(' | ')' | ',')) {
            if token.parse::<f64>().is_ok_and(|number| !number.is_finite()) {
                return Err("geometry coordinates must be finite".to_owned());
            }
        }
        Ok(sift_query_core::ScalarValue::Text(input.to_owned()))
    }
    fn fingerprint(&self) -> u64 {
        let mut hash = sift_query_algorithm::StableHasher64::default();
        hash.write_delimited(self.field);
        hash.write(self.srid.to_le_bytes());
        hash.finish()
    }
}
