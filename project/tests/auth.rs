//! Authentication through the whole app. `common::send` always sends a valid
//! token, so these tests build their requests by hand.
mod common;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use common::{TOKEN, app};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;

/// Sends `GET uri` with the given Authorization header (or none) and returns
/// the status, the WWW-Authenticate header and the JSON body.
async fn get(app: &Router, uri: &str, authorization: Option<&str>) -> (StatusCode, String, Value) {
    let mut request = Request::builder().uri(uri);
    if let Some(value) = authorization {
        request = request.header(header::AUTHORIZATION, value);
    }
    let response = app
        .clone()
        .oneshot(request.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let challenge = response
        .headers()
        .get(header::WWW_AUTHENTICATE)
        .map(|v| v.to_str().unwrap().to_string())
        .unwrap_or_default();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, challenge, body)
}

#[sqlx::test]
async fn requests_without_a_valid_token_get_401(pool: PgPool) {
    let app = app(pool);
    let cases = [
        (None, "the Authorization header is missing"),
        (
            Some("Basic dXNlcjpwYXNz"),
            "the Authorization header must be \"Bearer <token>\"",
        ),
        (
            Some("Bearer short"),
            "the token must be 10 characters long, got 5",
        ),
    ];
    for (authorization, message) in cases {
        let (status, challenge, body) = get(&app, "/products", authorization).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{authorization:?}");
        assert_eq!(challenge, "Bearer");
        assert_eq!(body, json!({ "error": message }));
    }
}

#[sqlx::test]
async fn a_valid_token_gets_through(pool: PgPool) {
    let app = app(pool);
    let (status, _, body) = get(&app, "/products", Some(&format!("Bearer {TOKEN}"))).await;
    assert_eq!((status, body), (StatusCode::OK, json!([])));
}

#[sqlx::test]
async fn an_unknown_path_is_404_even_without_a_token(pool: PgPool) {
    let (status, challenge, _) = get(&app(pool), "/nothing-here", None).await;
    assert_eq!((status, challenge.as_str()), (StatusCode::NOT_FOUND, ""));
}
