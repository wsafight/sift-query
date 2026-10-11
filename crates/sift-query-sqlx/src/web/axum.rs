use super::{QueryParams, SearchParamConfig, SearchParams, WebError, search};
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
            search::read_body(body.into_data_stream(), &config).await?
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
