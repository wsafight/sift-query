//! PostgreSQL web adapter primitives shared by axum, actix-web, and poem.
//!
//! Request extraction and transport rules are re-exported directly from
//! `sift-query-sqlx-common`, while this module maps PostgreSQL's richer
//! internal error enum to public HTTP errors.

use sift_query_core::ParamError;
pub use sift_query_sqlx_common::web::{
    PublicError, QueryParams, SearchParamConfig, SearchParams, WebError,
};

use crate::PgSearchError;

impl From<PgSearchError> for WebError {
    fn from(error: PgSearchError) -> Self {
        Self(public_error(&error))
    }
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
