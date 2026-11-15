//! The errors the catalog reports to its callers, and how each one becomes
//! an HTTP response.
use std::error::Error;
use std::fmt;

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;
use tracing::{debug, error};

use crate::products::validate::ValidationError;

/// Everything that can go wrong in the service. `Invalid` and `Reviews`
/// wrap the cause, and `source()` exposes it, so a caller can print the
/// whole chain. (`reqwest::Error` can't be compared, so neither can this.)
#[derive(Debug)]
pub enum CatalogError {
    Invalid(ValidationError),
    NotFound(i64),
    VariantNotFound { id: i64, sku: String },
    DuplicateName(String),
    DuplicateSku(String),
    NoImage,
    NotAnImage,
    Reviews(reqwest::Error),
    Database(sqlx::Error),
}

impl fmt::Display for CatalogError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CatalogError::Invalid(_) => write!(f, "invalid product"),
            CatalogError::NotFound(id) => write!(f, "product {id} not found"),
            CatalogError::VariantNotFound { id, sku } => {
                write!(f, "product {id} has no variant {sku}")
            }
            CatalogError::DuplicateName(name) => write!(f, "a product named {name:?} exists"),
            CatalogError::DuplicateSku(sku) => {
                write!(f, "SKU {sku} belongs to another product")
            }
            CatalogError::NoImage => write!(f, "no image has been uploaded"),
            CatalogError::NotAnImage => write!(f, "the body is not a JPEG, PNG or WebP image"),
            CatalogError::Reviews(_) => write!(f, "the reviews service failed"),
            CatalogError::Database(_) => write!(f, "the database failed"),
        }
    }
}

impl Error for CatalogError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            CatalogError::Invalid(reason) => Some(reason),
            CatalogError::Reviews(cause) => Some(cause),
            CatalogError::Database(cause) => Some(cause),
            _ => None,
        }
    }
}

/// Lets `?` turn a ValidationError into a CatalogError.
impl From<ValidationError> for CatalogError {
    fn from(reason: ValidationError) -> Self {
        CatalogError::Invalid(reason)
    }
}

/// Lets `?` turn a failed call to the reviews service into a CatalogError.
impl From<reqwest::Error> for CatalogError {
    fn from(cause: reqwest::Error) -> Self {
        CatalogError::Reviews(cause)
    }
}

/// Lets `?` turn any other database error into a CatalogError.
impl From<sqlx::Error> for CatalogError {
    fn from(cause: sqlx::Error) -> Self {
        CatalogError::Database(cause)
    }
}

/// A handler can return `Result<_, CatalogError>`: axum calls this for the
/// error case. The status says what kind of failure it was, and the body
/// says what exactly: `{"error": "invalid product: the name is empty"}`.
impl IntoResponse for CatalogError {
    fn into_response(self) -> Response {
        let status = match &self {
            CatalogError::Invalid(_) => StatusCode::UNPROCESSABLE_ENTITY,
            CatalogError::NotFound(_)
            | CatalogError::VariantNotFound { .. }
            | CatalogError::NoImage => StatusCode::NOT_FOUND,
            CatalogError::DuplicateName(_) | CatalogError::DuplicateSku(_) => StatusCode::CONFLICT,
            CatalogError::NotAnImage => StatusCode::UNSUPPORTED_MEDIA_TYPE,
            // Not our client's fault, and not ours: the server we depend on failed
            CatalogError::Reviews(_) => StatusCode::BAD_GATEWAY,
            CatalogError::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };
        // The server's log gets the whole chain of causes; the client doesn't
        let mut chain = self.to_string();
        let mut cause = self.source();
        while let Some(c) = cause {
            chain = format!("{chain}: {c}");
            cause = c.source();
        }
        if status.is_server_error() {
            error!(%status, error = %chain, "request failed");
        } else {
            debug!(%status, error = %chain, "request rejected");
        }
        let message = match self {
            // The causes name internal hosts and tables; the client only
            // needs to know what failed
            CatalogError::Reviews(_) | CatalogError::Database(_) => self.to_string(),
            _ => chain,
        };
        (status, Json(json!({ "error": message }))).into_response()
    }
}
