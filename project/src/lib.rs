//! The shoe catalog service as a library: `main.rs` starts it, and the
//! tests in tests/ call the same `app`.
use axum::extract::Request;
use axum::{Router, middleware};
use tower_http::trace::{DefaultOnResponse, TraceLayer};
use tracing::{Level, Span, field};

pub mod auth;
pub mod error;
pub mod products;
pub mod reviews;
pub mod settings;

pub use error::CatalogError;
use products::ProductService;

/// The whole HTTP application. Each feature module contributes its routes:
/// the products, and `GET /me` from `auth`. Middleware for every route goes
/// here:
///
/// - `route_layer` runs the token check only for requests that match a
///   route, so an unknown path is still 404, not 401;
/// - the trace layer, added last, is the outermost: a span per request
///   (method and URI) and an event when the response is sent (status and
///   latency), including the 401s. Events logged while handling a request
///   happen inside its span, so they carry its method and URI too, and,
///   once the token check has filled it in, the user.
pub fn app(products: ProductService) -> Router {
    products::router(products)
        .merge(auth::router())
        .route_layer(middleware::from_fn(auth::require_bearer_token))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(request_span)
                .on_response(DefaultOnResponse::new().level(Level::INFO)),
        )
}

/// The span for one request: what `DefaultMakeSpan` records, plus a `user`
/// field. A span's fields are fixed when it's created, so `user` starts
/// empty and the token check fills it in; an empty field isn't printed.
fn request_span(request: &Request) -> Span {
    tracing::info_span!(
        "request",
        method = %request.method(),
        uri = %request.uri(),
        version = ?request.version(),
        user = field::Empty,
    )
}
