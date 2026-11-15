//! Authentication: every request to a route must carry a bearer token,
//! `Authorization: Bearer <token>`. For now the only rule is that the token
//! is 10 characters long. A real service would verify the token here instead:
//! check a JWT's signature and expiry, or ask the identity provider about it.
use std::fmt;

use axum::Json;
use axum::extract::Request;
use axum::http::{HeaderValue, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use serde_json::json;
use tracing::debug;

/// The length every token must have.
pub const TOKEN_LENGTH: usize = 10;

/// Why a request was not authenticated.
#[derive(Debug, PartialEq)]
pub enum AuthError {
    MissingHeader,
    NotBearer,
    WrongLength(usize),
}

impl fmt::Display for AuthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AuthError::MissingHeader => write!(f, "the Authorization header is missing"),
            AuthError::NotBearer => {
                write!(f, "the Authorization header must be \"Bearer <token>\"")
            }
            AuthError::WrongLength(n) => {
                write!(
                    f,
                    "the token must be {TOKEN_LENGTH} characters long, got {n}"
                )
            }
        }
    }
}

/// 401 Unauthorized, with the `WWW-Authenticate` header that says which
/// scheme the server expects, and the usual JSON error body.
impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        debug!(error = %self, "request not authenticated");
        let challenge = [(header::WWW_AUTHENTICATE, "Bearer")];
        let body = Json(json!({ "error": self.to_string() }));
        (StatusCode::UNAUTHORIZED, challenge, body).into_response()
    }
}

/// The token in an `Authorization` header value, if the value is a valid
/// bearer credential. The scheme name is case-insensitive (RFC 9110).
pub fn bearer_token(value: Option<&HeaderValue>) -> Result<&str, AuthError> {
    let value = value.ok_or(AuthError::MissingHeader)?;
    // to_str accepts only visible ASCII, so below, bytes and characters are the same
    let value = value.to_str().map_err(|_| AuthError::NotBearer)?;
    let (scheme, token) = value.split_once(' ').ok_or(AuthError::NotBearer)?;
    if !scheme.eq_ignore_ascii_case("Bearer") {
        return Err(AuthError::NotBearer);
    }
    if token.len() != TOKEN_LENGTH {
        return Err(AuthError::WrongLength(token.len()));
    }
    Ok(token)
}

/// Middleware for `axum::middleware::from_fn`: lets the request through to
/// the handler only if it carries a valid token, and answers 401 otherwise.
pub async fn require_bearer_token(request: Request, next: Next) -> Result<Response, AuthError> {
    bearer_token(request.headers().get(header::AUTHORIZATION))?;
    Ok(next.run(request).await)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The token as a String: the &str borrows from a HeaderValue that is
    /// dropped when this function returns.
    fn check(value: &str) -> Result<String, AuthError> {
        let value = HeaderValue::from_str(value).unwrap();
        bearer_token(Some(&value)).map(str::to_string)
    }

    #[test]
    fn a_ten_character_bearer_token_is_accepted() {
        assert_eq!(check("Bearer abcdefghij"), Ok("abcdefghij".to_string()));
        assert_eq!(check("bearer 0123456789"), Ok("0123456789".to_string()));
    }

    #[test]
    fn anything_else_is_rejected() {
        assert_eq!(bearer_token(None), Err(AuthError::MissingHeader));
        assert_eq!(check("abcdefghij"), Err(AuthError::NotBearer));
        assert_eq!(check("Basic abcdefghij"), Err(AuthError::NotBearer));
        assert_eq!(check("Bearer abcdefghi"), Err(AuthError::WrongLength(9)));
        assert_eq!(check("Bearer abcdefghijk"), Err(AuthError::WrongLength(11)));
        assert_eq!(check("Bearer  abcdefghij"), Err(AuthError::WrongLength(11)));
    }
}
