//! PostgreSQL web adapter primitives shared by axum, actix-web, and poem.
//!
//! Request parsing is kept aligned with the portable common adapter, while
//! this module maps PostgreSQL's richer internal error enum to public HTTP
//! errors.

use sift_query_core::{ParamError, QueryLimits, RawParams};

use crate::PgSearchError;

mod search;
pub use search::{SearchParamConfig, SearchParams};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryParams(pub RawParams);

impl QueryParams {
    pub fn parse(query: Option<&str>) -> Result<Self, ParamError> {
        RawParams::parse(query.unwrap_or_default()).map(Self)
    }

    pub fn parse_with_limits(
        query: Option<&str>,
        limits: &QueryLimits,
    ) -> Result<Self, ParamError> {
        RawParams::parse_with_limits(query.unwrap_or_default(), limits).map(Self)
    }

    #[must_use]
    pub fn raw(&self) -> &RawParams {
        &self.0
    }
}

/// Safe default HTTP error adapter. Applications can override the mapping
/// using `public_error` without publishing the underlying database error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WebError(pub PublicError);

impl std::fmt::Display for WebError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.0.code)
    }
}
impl std::error::Error for WebError {}
impl From<PgSearchError> for WebError {
    fn from(error: PgSearchError) -> Self {
        Self(public_error(&error))
    }
}
impl From<ParamError> for WebError {
    fn from(error: ParamError) -> Self {
        if error == ParamError::QueryTooLong {
            return search::error(413, "query_too_long");
        }
        Self(PublicError {
            status: 400,
            code: "invalid_query",
        })
    }
}
impl PublicError {
    #[must_use]
    pub fn body(self) -> String {
        format!("{{\"error\":{{\"code\":\"{}\"}}}}", self.code)
    }
}

#[cfg(feature = "actix-web")]
mod actix;
#[cfg(feature = "axum")]
mod axum;
#[cfg(feature = "poem")]
mod poem;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PublicError {
    pub status: u16,
    pub code: &'static str,
}

#[must_use]
pub fn public_error(error: &PgSearchError) -> PublicError {
    match error {
        PgSearchError::Param(ParamError::QueryTooLong)
        | PgSearchError::Plan(sift_query_core::PlanError::Param(ParamError::QueryTooLong)) => {
            WebError::from(ParamError::QueryTooLong).0
        }
        PgSearchError::Param(_) | PgSearchError::Plan(sift_query_core::PlanError::Param(_)) => {
            PublicError {
                status: 400,
                code: "invalid_query",
            }
        }
        PgSearchError::Timeout(_)
        | PgSearchError::Extension(crate::ExtensionError::LabelTimeout) => PublicError {
            status: 504,
            code: "query_timeout",
        },
        PgSearchError::ScanLimitExceeded { .. } => PublicError {
            status: 413,
            code: "scan_limit_exceeded",
        },
        _ => PublicError {
            status: 500,
            code: "query_failed",
        },
    }
}
