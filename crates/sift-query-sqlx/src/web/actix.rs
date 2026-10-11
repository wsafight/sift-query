use super::{QueryParams, SearchParamConfig, SearchParams, WebError, search};
use actix_web::{
    FromRequest, HttpRequest, HttpResponse, ResponseError, dev::Payload, http::StatusCode,
};
impl FromRequest for SearchParams {
    type Error = WebError;
    type Future = std::pin::Pin<Box<dyn std::future::Future<Output = Result<Self, Self::Error>>>>;
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
                search::read_body(payload, &config).await?
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
