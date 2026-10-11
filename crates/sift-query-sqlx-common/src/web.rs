//! Framework-neutral HTTP parameter validation and optional framework extractors.

use std::time::Duration;

use sift_query_core::{ParamError, QueryLimits, RawParams};

use crate::SearchError;

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchParams(pub RawParams);

#[derive(Debug, Clone)]
pub struct SearchParamConfig {
    pub limits: QueryLimits,
    pub body_timeout: Duration,
}

impl Default for SearchParamConfig {
    fn default() -> Self {
        Self {
            limits: QueryLimits::default(),
            body_timeout: Duration::from_secs(5),
        }
    }
}

impl SearchParams {
    #[must_use]
    pub fn raw(&self) -> &RawParams {
        &self.0
    }

    pub fn validate_request(
        method: &str,
        query: Option<&str>,
        content_type: Option<&str>,
    ) -> Result<(), WebError> {
        match method {
            "GET" => Ok(()),
            "POST" => {
                if query.is_some_and(|query| !query.is_empty()) {
                    return Err(error(400, "conflicting_query_body"));
                }
                let mut parts = content_type.unwrap_or_default().split(';');
                if !parts
                    .next()
                    .unwrap_or_default()
                    .trim()
                    .eq_ignore_ascii_case("application/x-www-form-urlencoded")
                    || parts.any(|part| {
                        let Some((name, value)) = part.trim().split_once('=') else {
                            return true;
                        };
                        !name.trim().eq_ignore_ascii_case("charset")
                            || !value.trim().trim_matches('"').eq_ignore_ascii_case("utf-8")
                    })
                {
                    return Err(error(415, "unsupported_query_media_type"));
                }
                Ok(())
            }
            _ => Err(error(405, "query_method_not_allowed")),
        }
    }

    pub fn parse_request(
        method: &str,
        query: Option<&str>,
        content_type: Option<&str>,
        body: &[u8],
        limits: &QueryLimits,
    ) -> Result<Self, WebError> {
        Self::validate_request(method, query, content_type)?;
        let input = if method == "GET" {
            query.unwrap_or_default()
        } else {
            if body.len() > limits.max_raw_query_bytes {
                return Err(error(413, "query_body_too_large"));
            }
            std::str::from_utf8(body).map_err(|_| WebError::from(ParamError::InvalidEncoding))?
        };
        RawParams::parse_with_limits(input, limits)
            .map(Self)
            .map_err(WebError::from)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PublicError {
    pub status: u16,
    pub code: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WebError(pub PublicError);

impl std::fmt::Display for WebError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.0.code)
    }
}
impl std::error::Error for WebError {}

impl From<SearchError> for WebError {
    fn from(error: SearchError) -> Self {
        Self(public_error(&error))
    }
}

impl From<ParamError> for WebError {
    fn from(param_error: ParamError) -> Self {
        if param_error == ParamError::QueryTooLong {
            return error(413, "query_too_long");
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

#[must_use]
pub fn public_error(error: &SearchError) -> PublicError {
    match error {
        SearchError::Param(ParamError::QueryTooLong)
        | SearchError::Plan(sift_query_core::PlanError::Param(ParamError::QueryTooLong)) => {
            PublicError {
                status: 413,
                code: "query_too_long",
            }
        }
        SearchError::Param(_) | SearchError::Plan(sift_query_core::PlanError::Param(_)) => {
            PublicError {
                status: 400,
                code: "invalid_query",
            }
        }
        SearchError::Timeout(_) => PublicError {
            status: 504,
            code: "query_timeout",
        },
        SearchError::ScanLimitExceeded { .. } => PublicError {
            status: 413,
            code: "scan_limit_exceeded",
        },
        _ => PublicError {
            status: 500,
            code: "query_failed",
        },
    }
}

const fn error(status: u16, code: &'static str) -> WebError {
    WebError(PublicError { status, code })
}

#[cfg(any(feature = "axum", feature = "actix-web", feature = "poem", test))]
#[allow(dead_code)]
async fn read_body<S, B, E>(stream: S, config: &SearchParamConfig) -> Result<Vec<u8>, WebError>
where
    S: futures_core::Stream<Item = Result<B, E>>,
    B: AsRef<[u8]>,
{
    use futures_util::StreamExt;
    tokio::time::timeout(config.body_timeout, async {
        futures_util::pin_mut!(stream);
        let mut bytes = Vec::new();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|_| error(400, "invalid_query_body"))?;
            let chunk = chunk.as_ref();
            if chunk.len()
                > config
                    .limits
                    .max_raw_query_bytes
                    .saturating_sub(bytes.len())
            {
                return Err(error(413, "query_body_too_large"));
            }
            bytes.extend_from_slice(chunk);
        }
        Ok(bytes)
    })
    .await
    .map_err(|_| error(408, "query_body_timeout"))?
}

#[cfg(feature = "axum")]
mod axum_impl {
    use super::{QueryParams, SearchParamConfig, SearchParams, WebError};
    use axum::{
        extract::{FromRequest, FromRequestParts, Request},
        http::{StatusCode, request::Parts},
        response::{IntoResponse, Response},
    };

    impl<S: Send + Sync> FromRequest<S> for SearchParams {
        type Rejection = WebError;
        async fn from_request(request: Request, _: &S) -> Result<Self, Self::Rejection> {
            let (parts, body) = request.into_parts();
            let config = parts
                .extensions
                .get::<SearchParamConfig>()
                .cloned()
                .unwrap_or_default();
            let method = parts.method.as_str();
            let query = parts.uri.query();
            let content_type = parts
                .headers
                .get("content-type")
                .and_then(|value| value.to_str().ok());
            Self::validate_request(method, query, content_type)?;
            let body = if method == "POST" {
                super::read_body(body.into_data_stream(), &config).await?
            } else {
                Vec::new()
            };
            Self::parse_request(method, query, content_type, &body, &config.limits)
        }
    }

    impl<S: Send + Sync> FromRequestParts<S> for QueryParams {
        type Rejection = WebError;
        fn from_request_parts(
            parts: &mut Parts,
            _: &S,
        ) -> impl std::future::Future<Output = Result<Self, Self::Rejection>> + Send {
            std::future::ready(Self::parse(parts.uri.query()).map_err(WebError::from))
        }
    }

    impl IntoResponse for WebError {
        fn into_response(self) -> Response {
            (
                StatusCode::from_u16(self.0.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
                [("content-type", "application/json")],
                self.0.body(),
            )
                .into_response()
        }
    }
}

#[cfg(feature = "actix-web")]
mod actix_impl {
    use super::{QueryParams, SearchParamConfig, SearchParams, WebError};
    use actix_web::{
        FromRequest, HttpRequest, HttpResponse, ResponseError, dev::Payload, http::StatusCode,
    };

    impl FromRequest for SearchParams {
        type Error = WebError;
        type Future =
            std::pin::Pin<Box<dyn std::future::Future<Output = Result<Self, Self::Error>>>>;
        fn from_request(request: &HttpRequest, payload: &mut Payload) -> Self::Future {
            let config = request
                .app_data::<SearchParamConfig>()
                .cloned()
                .unwrap_or_default();
            let request = request.clone();
            let payload = payload.take();
            Box::pin(async move {
                let method = request.method().as_str();
                let query = request.uri().query();
                let content_type = request
                    .headers()
                    .get("content-type")
                    .and_then(|value| value.to_str().ok());
                Self::validate_request(method, query, content_type)?;
                let body = if method == "POST" {
                    super::read_body(payload, &config).await?
                } else {
                    Vec::new()
                };
                Self::parse_request(method, query, content_type, &body, &config.limits)
            })
        }
    }
    impl FromRequest for QueryParams {
        type Error = WebError;
        type Future = std::future::Ready<Result<Self, Self::Error>>;
        fn from_request(request: &HttpRequest, _: &mut Payload) -> Self::Future {
            std::future::ready(Self::parse(Some(request.query_string())).map_err(WebError::from))
        }
    }
    impl ResponseError for WebError {
        fn status_code(&self) -> StatusCode {
            StatusCode::from_u16(self.0.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR)
        }
        fn error_response(&self) -> HttpResponse {
            HttpResponse::build(self.status_code())
                .content_type("application/json")
                .body(self.0.body())
        }
    }
}

#[cfg(feature = "poem")]
mod poem_impl {
    use super::{QueryParams, SearchParamConfig, SearchParams, WebError};
    use poem::{FromRequest, IntoResponse, Request, RequestBody, Response, http::StatusCode};

    impl<'a> FromRequest<'a> for SearchParams {
        async fn from_request(request: &'a Request, body: &mut RequestBody) -> poem::Result<Self> {
            let config = request
                .data::<SearchParamConfig>()
                .cloned()
                .unwrap_or_default();
            let method = request.method().as_str();
            let query = request.uri().query();
            let content_type = request.content_type();
            Self::validate_request(method, query, content_type)?;
            let body = if method == "POST" {
                super::read_body(body.take()?.into_bytes_stream(), &config).await?
            } else {
                Vec::new()
            };
            Ok(Self::parse_request(
                method,
                query,
                content_type,
                &body,
                &config.limits,
            )?)
        }
    }
    impl<'a> FromRequest<'a> for QueryParams {
        fn from_request(
            request: &'a Request,
            _: &mut RequestBody,
        ) -> impl std::future::Future<Output = poem::Result<Self>> + Send {
            std::future::ready(
                Self::parse(request.uri().query()).map_err(|error| {
                    poem::Error::from_response(WebError::from(error).into_response())
                }),
            )
        }
    }
    impl IntoResponse for WebError {
        fn into_response(self) -> Response {
            Response::builder()
                .status(
                    StatusCode::from_u16(self.0.status)
                        .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
                )
                .content_type("application/json")
                .body(self.0.body())
        }
    }
    impl From<WebError> for poem::Error {
        fn from(error: WebError) -> Self {
            Self::from_response(error.into_response())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_transport_rules_preserve_ordered_parameters() {
        let limits = QueryLimits::default();
        let get =
            SearchParams::parse_request("GET", Some("id=1&id=2"), None, &[], &limits).unwrap();
        assert_eq!(get.raw().entries().len(), 2);
        let post = SearchParams::parse_request(
            "POST",
            None,
            Some("application/x-www-form-urlencoded; charset=utf-8"),
            b"id=3",
            &limits,
        )
        .unwrap();
        assert_eq!(post.raw().entries().len(), 1);
        assert_eq!(
            SearchParams::parse_request(
                "POST",
                Some("id=1"),
                Some("application/x-www-form-urlencoded"),
                b"",
                &limits
            )
            .unwrap_err()
            .0
            .code,
            "conflicting_query_body"
        );
    }

    #[test]
    fn public_errors_are_low_sensitivity() {
        let error = SearchError::ScanLimitExceeded { max_rows: 4 };
        assert_eq!(public_error(&error).code, "scan_limit_exceeded");
        assert!(!WebError::from(error).0.body().contains('4'));
    }
}
