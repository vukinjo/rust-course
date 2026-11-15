//! The shoe catalog service as a library: `main.rs` starts it, and the
//! tests in tests/ call the same `app`.
use axum::{Router, middleware};
use tower_http::trace::{DefaultMakeSpan, DefaultOnResponse, TraceLayer};
use tracing::Level;

pub mod auth;
pub mod error;
pub mod products;
pub mod reviews;
pub mod settings;

pub use error::CatalogError;
use products::ProductService;

/// The whole HTTP application. Each feature module contributes its routes;
/// there's one feature so far. Middleware for every route goes here:
///
/// - `route_layer` runs the token check only for requests that match a
///   route, so an unknown path is still 404, not 401;
/// - the trace layer, added last, is the outermost: a span per request
///   (method and URI) and an event when the response is sent (status and
///   latency), including the 401s. Events logged while handling a request
///   happen inside its span, so they carry its method and URI too.
pub fn app(products: ProductService) -> Router {
    products::router(products)
        .route_layer(middleware::from_fn(auth::require_bearer_token))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::new().level(Level::INFO))
                .on_response(DefaultOnResponse::new().level(Level::INFO)),
        )
}
