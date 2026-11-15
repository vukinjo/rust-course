//! Everything about products, from HTTP down to storage:
//!
//! - `routes`: the HTTP layer, which calls
//! - `service`: the use cases, which call
//! - `service::repo`: the storage (private to the service).
//!
//! `model` holds the API's request and response types, `entity` the stored
//! types, and `mapper` the conversions between the two. `validate` holds
//! the rules a product must follow.
pub mod entity;
mod mapper;
pub mod model;
mod routes;
mod service;
pub mod validate;

pub use routes::{MAX_BODY_BYTES, router};
pub use service::ProductService;
