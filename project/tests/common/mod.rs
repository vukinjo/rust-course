//! Helpers shared by the test files. A file in a subdirectory of tests/ is
//! not compiled as a test of its own; each test file includes it with
//! `mod common;`.
#![allow(dead_code)] // each test file uses only some of the helpers

use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use serde_json::Value;
use shoes::products::ProductService;
use shoes::reviews::ReviewsClient;
use sqlx::PgPool;
use tower::ServiceExt;

/// A token the service accepts: any 10 characters.
pub const TOKEN: &str = "test-token";

pub const PNG: &[u8] = b"\x89PNG\r\n\x1a\n not really the rest of a PNG";

/// The app on the test's own database. Its reviews client points at
/// `reviews_url`.
pub fn app_with_reviews(pool: PgPool, reviews_url: &str) -> Router {
    let reviews = ReviewsClient::new(reviews_url, Duration::from_secs(5)).unwrap();
    shoes::app(ProductService::new(pool, reviews))
}

/// An app whose reviews service is never reached: nothing listens on port 9.
pub fn app(pool: PgPool) -> Router {
    app_with_reviews(pool, "http://127.0.0.1:9")
}

/// Sends one request and returns the status, the Content-Type and the body.
pub async fn send(
    app: &Router,
    method: &str,
    uri: &str,
    body: Body,
) -> (StatusCode, String, Vec<u8>) {
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
        .body(body)
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .map(|v| v.to_str().unwrap().to_string())
        .unwrap_or_default();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, content_type, bytes.to_vec())
}

/// The same for JSON requests and responses.
pub async fn call(
    app: &Router,
    method: &str,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let body = body.map_or(Body::empty(), |b| Body::from(b.to_string()));
    let (status, _, bytes) = send(app, method, uri, body).await;
    let json = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes)
            .unwrap_or(Value::String(String::from_utf8_lossy(&bytes).into()))
    };
    (status, json)
}
