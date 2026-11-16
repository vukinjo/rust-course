//! The reviews endpoint, tested against a stand-in for jsonplaceholder: a
//! small axum app on a free local port. The tests need no internet access,
//! and they can make the other service fail on purpose. Each test gets a
//! fresh database from `#[sqlx::test]`.
mod common;

use axum::extract::Query;
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use common::{app_with_reviews, call};
use serde::Deserialize;
use serde_json::{Value, json};
use shoes::reviews::post_for;
use sqlx::PgPool;
use tokio::net::TcpListener;
use uuid::Uuid;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CommentsQuery {
    post_id: u64,
}

/// Answers like jsonplaceholder: two comments for each post. The second one
/// names the post, so a test can see which post the client asked for.
async fn comments(Query(query): Query<CommentsQuery>) -> Json<Value> {
    let post = query.post_id;
    Json(json!([
        {"postId": post, "id": 1, "name": "Great grip", "email": "ana@example.com", "body": "Wet rocks, no problem."},
        {"postId": post, "id": 2, "name": "Runs small", "email": "ben@example.com", "body": format!("Order a size up. (post {post})")}
    ]))
}

/// Starts `stub` on port 0 (the OS picks a free port) and returns its URL.
async fn serve(stub: Router) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, stub).await.unwrap() });
    url
}

/// The app with one product, and the URI of its reviews.
async fn app_with_one_product(pool: PgPool, reviews_url: &str) -> (Router, Uuid, String) {
    let app = app_with_reviews(pool, reviews_url);
    let product = json!({"name": "Trail Runner X", "brand": "Stride"});
    let (status, created) = call(&app, "POST", "/products", Some(product)).await;
    assert_eq!(status, StatusCode::CREATED);
    let id: Uuid = created["id"].as_str().unwrap().parse().unwrap();
    (app, id, format!("/products/{id}/reviews"))
}

#[sqlx::test]
async fn reviews_are_fetched_and_renamed(pool: PgPool) {
    let url = serve(Router::new().route("/comments", get(comments))).await;
    let (app, id, uri) = app_with_one_product(pool, &url).await;
    let (status, body) = call(&app, "GET", &uri, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body[0],
        json!({"author": "ana@example.com", "title": "Great grip", "body": "Wet rocks, no problem."})
    );
    assert_eq!(body.as_array().unwrap().len(), 2);
    let post = post_for(id);
    assert_eq!(body[1]["body"], format!("Order a size up. (post {post})"));
}

#[sqlx::test]
async fn an_unknown_product_is_404_without_calling_the_service(pool: PgPool) {
    let app = app_with_reviews(pool, "http://127.0.0.1:9"); // would fail if it were called
    let uri = format!("/products/{}/reviews", Uuid::now_v7());
    let (status, _) = call(&app, "GET", &uri, None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn a_failing_service_is_a_bad_gateway(pool: PgPool) {
    let broken = Router::new().route(
        "/comments",
        get(|| async { StatusCode::INTERNAL_SERVER_ERROR }),
    );
    let url = serve(broken).await;
    let (app, _, uri) = app_with_one_product(pool, &url).await;
    let (status, body) = call(&app, "GET", &uri, None).await;
    assert_eq!(status, StatusCode::BAD_GATEWAY);
    assert_eq!(body, json!({"error": "the reviews service failed"}));
}

#[sqlx::test]
async fn an_unexpected_answer_is_a_bad_gateway(pool: PgPool) {
    let wrong = Router::new().route(
        "/comments",
        get(|| async { Json(json!({"not": "a list"})) }),
    );
    let url = serve(wrong).await;
    let (app, _, uri) = app_with_one_product(pool, &url).await;
    let (status, _) = call(&app, "GET", &uri, None).await;
    assert_eq!(status, StatusCode::BAD_GATEWAY);
}

#[sqlx::test]
async fn an_unreachable_service_is_a_bad_gateway(pool: PgPool) {
    let (app, _, uri) = app_with_one_product(pool, "http://127.0.0.1:9").await;
    let (status, _) = call(&app, "GET", &uri, None).await;
    assert_eq!(status, StatusCode::BAD_GATEWAY);
}
