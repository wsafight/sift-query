use super::{QueryParams, SearchParamConfig, SearchParams, WebError, search};
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
            search::read_body(body.take()?.into_bytes_stream(), &config).await?
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
            Self::parse(request.uri().query())
                .map_err(|error| poem::Error::from_response(WebError::from(error).into_response())),
        )
    }
}
impl IntoResponse for WebError {
    fn into_response(self) -> Response {
        Response::builder()
            .status(
                StatusCode::from_u16(self.0.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
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
