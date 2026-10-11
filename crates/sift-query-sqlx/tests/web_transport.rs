#[cfg(all(feature = "axum", feature = "actix-web", feature = "poem"))]
use sift_query_sqlx::SearchParamConfig;
use sift_query_sqlx::SearchParams;

const FORM: &str = "id=2&id=1&id-op=il&name=%E4%B8%AD+%2B%26%3D";
const MEDIA: &str = "application/x-www-form-urlencoded;charset=UTF-8";

#[test]
fn transport_rules_and_query_budgets_are_shared() {
    let limits = sift_query_sqlx::core::QueryLimits::default();
    let get = SearchParams::parse_request("GET", Some(FORM), None, &[], &limits).unwrap();
    let post =
        SearchParams::parse_request("POST", None, Some(MEDIA), FORM.as_bytes(), &limits).unwrap();
    assert_eq!(get, post);
    assert_eq!(post.raw().entries()[3].1, "中 +&=");
    for (method, query, media, body, status, code) in [
        (
            "POST",
            Some("id=1"),
            Some(MEDIA),
            "id=2",
            400,
            "conflicting_query_body",
        ),
        (
            "POST",
            None,
            Some("application/json"),
            "{}",
            415,
            "unsupported_query_media_type",
        ),
        ("PUT", None, None, "", 405, "query_method_not_allowed"),
        ("POST", None, Some(MEDIA), "id=%GG", 400, "invalid_query"),
    ] {
        let error = SearchParams::parse_request(method, query, media, body.as_bytes(), &limits)
            .unwrap_err();
        assert_eq!((error.0.status, error.0.code), (status, code));
    }
    let limits = sift_query_sqlx::core::QueryLimits {
        max_raw_query_bytes: 3,
        ..limits
    };
    assert_eq!(
        SearchParams::parse_request("GET", Some("id=1"), None, &[], &limits)
            .unwrap_err()
            .0
            .code,
        "query_too_long"
    );
    assert_eq!(
        SearchParams::parse_request("POST", None, Some(MEDIA), b"id=1", &limits)
            .unwrap_err()
            .0
            .code,
        "query_body_too_large"
    );
}

#[cfg(all(feature = "axum", feature = "actix-web", feature = "poem"))]
#[actix_web::test]
async fn actual_extractors_accept_get_post_and_enforce_configured_body_limits() {
    use axum::extract::FromRequest;
    for method in ["GET", "POST"] {
        let uri = if method == "GET" {
            format!("/users?{FORM}")
        } else {
            "/users".to_owned()
        };
        let body = if method == "POST" { FORM } else { "" };
        let axum_request = axum::http::Request::builder()
            .method(method)
            .uri(&uri)
            .header("content-type", MEDIA)
            .body(axum::body::Body::from(body))
            .unwrap();
        let axum_params = SearchParams::from_request(axum_request, &()).await.unwrap();
        let (actix_request, mut payload) = actix_web::test::TestRequest::with_uri(&uri)
            .method(method.parse().unwrap())
            .insert_header(("content-type", MEDIA))
            .set_payload(body)
            .to_http_parts();
        let actix_params =
            <SearchParams as actix_web::FromRequest>::from_request(&actix_request, &mut payload)
                .await
                .unwrap();
        let (poem_request, mut body) = poem::Request::builder()
            .method(method.parse().unwrap())
            .uri(uri.parse().unwrap())
            .content_type(MEDIA)
            .body(body)
            .split();
        let poem_params =
            <SearchParams as poem::FromRequest>::from_request(&poem_request, &mut body)
                .await
                .unwrap();
        assert_eq!(axum_params, actix_params);
        assert_eq!(axum_params, poem_params);
    }
    let config = SearchParamConfig {
        limits: sift_query_sqlx::core::QueryLimits {
            max_raw_query_bytes: 3,
            ..Default::default()
        },
        ..Default::default()
    };
    let request = axum::http::Request::builder()
        .method("POST")
        .uri("/users")
        .header("content-type", MEDIA)
        .extension(config.clone())
        .body(axum::body::Body::from("id=1"))
        .unwrap();
    assert_eq!(
        SearchParams::from_request(request, &())
            .await
            .unwrap_err()
            .0
            .status,
        413
    );
    let (request, mut payload) = actix_web::test::TestRequest::post()
        .uri("/users")
        .insert_header(("content-type", MEDIA))
        .app_data(config.clone())
        .set_payload("id=1")
        .to_http_parts();
    assert_eq!(
        <SearchParams as actix_web::FromRequest>::from_request(&request, &mut payload)
            .await
            .unwrap_err()
            .0
            .status,
        413
    );
    let (request, mut body) = poem::Request::builder()
        .method(poem::http::Method::POST)
        .uri_str("/users")
        .extension(config)
        .content_type(MEDIA)
        .body("id=1")
        .split();
    assert_eq!(
        <SearchParams as poem::FromRequest>::from_request(&request, &mut body)
            .await
            .unwrap_err()
            .status()
            .as_u16(),
        413
    );
}
