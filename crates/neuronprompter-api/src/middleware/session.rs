// =============================================================================
// Session middleware and authentication extractors.
//
// The session middleware runs on every request, extracting the `np_session`
// cookie and populating request extensions with the authenticated context.
// The AuthUser and AuthSession extractors provide type-safe access to the
// session context in handler functions.
// =============================================================================

use std::sync::Arc;

use axum::extract::{FromRequestParts, Request};
use axum::http::{StatusCode, header, request::Parts};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use crate::state::AppState;

/// Session context injected into request extensions by the session middleware.
#[derive(Clone, Debug)]
pub struct AuthContext {
    /// The user ID from the session, if a user has been selected.
    pub user_id: Option<i64>,
    /// The hex-encoded session token.
    pub session_token: String,
}

/// Extractor that requires a valid session with a selected user.
/// Returns 401 UNAUTHORIZED if no session exists or no user is selected.
#[derive(Clone, Debug)]
pub struct AuthUser {
    /// The authenticated user ID (guaranteed non-None).
    pub user_id: i64,
    /// The hex-encoded session token.
    pub session_token: String,
}

/// Extractor that requires a valid session but does NOT require a user selection.
/// Returns 401 only if no session exists at all.
#[derive(Clone, Debug)]
pub struct AuthSession {
    /// The user ID from the session, or None if no user selected yet.
    pub user_id: Option<i64>,
    /// The hex-encoded session token.
    pub session_token: String,
}

impl<S> FromRequestParts<S> for AuthUser
where
    S: Send + Sync,
{
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let ctx = parts
            .extensions
            .get::<AuthContext>()
            .ok_or_else(|| unauthorized_json("AUTH_REQUIRED", "a valid session is required"))?;

        let user_id = ctx.user_id.ok_or_else(|| {
            unauthorized_json(
                "USER_REQUIRED",
                "a user must be selected to perform this action",
            )
        })?;

        Ok(Self {
            user_id,
            session_token: ctx.session_token.clone(),
        })
    }
}

impl<S> FromRequestParts<S> for AuthSession
where
    S: Send + Sync,
{
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let ctx = parts
            .extensions
            .get::<AuthContext>()
            .ok_or_else(|| unauthorized_json("AUTH_REQUIRED", "a valid session is required"))?;

        Ok(Self {
            user_id: ctx.user_id,
            session_token: ctx.session_token.clone(),
        })
    }
}

/// Axum middleware that extracts the session cookie and populates request
/// extensions with the `AuthContext`.
pub async fn session_middleware(
    state: axum::extract::State<Arc<AppState>>,
    req: Request,
    next: Next,
) -> Response {
    let cookie_token = extract_cookie_token(req.headers());

    match cookie_token {
        Some(ref token) => {
            // Validate existing session.
            if let Some(session_ref) = state.sessions.get_session(token) {
                let user_id = session_ref.user_id();
                drop(session_ref);
                let ctx = AuthContext {
                    user_id,
                    session_token: token.clone(),
                };
                let mut req = req;
                req.extensions_mut().insert(ctx);
                next.run(req).await
            } else {
                // Invalid/expired session -- clear the cookie and proceed without auth.
                let response = next.run(req).await;
                clear_session_cookie(response)
            }
        }
        None => {
            // No cookie present -- proceed without auth context.
            // Protected endpoints return 401 via AuthUser/AuthSession extractors.
            next.run(req).await
        }
    }
}

/// Extracts the `np_session` token value from the Cookie header.
pub fn extract_cookie_token(headers: &axum::http::HeaderMap) -> Option<String> {
    let cookie_header = headers.get(header::COOKIE)?.to_str().ok()?;
    for part in cookie_header.split(';') {
        let trimmed = part.trim();
        if let Some(value) = trimmed.strip_prefix("np_session=") {
            let value = value.trim();
            if !value.is_empty() {
                return Some(value.to_owned());
            }
        }
    }
    None
}

/// Adds a `Set-Cookie` header to set the session cookie.
pub fn set_session_cookie(response: Response, token: &str, is_localhost: bool) -> Response {
    let secure = if is_localhost { "" } else { "; Secure" };
    let cookie_value =
        format!("np_session={token}; HttpOnly; SameSite=Strict; Path=/; Max-Age=86400{secure}");
    let mut response = response;
    if let Ok(val) = axum::http::HeaderValue::from_str(&cookie_value) {
        response.headers_mut().append(header::SET_COOKIE, val);
    }
    response
}

/// Adds a `Set-Cookie` header to clear the session cookie.
fn clear_session_cookie(response: Response) -> Response {
    let cookie_value = "np_session=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0";
    let mut response = response;
    if let Ok(val) = axum::http::HeaderValue::from_str(cookie_value) {
        response.headers_mut().append(header::SET_COOKIE, val);
    }
    response
}

/// Returns a 401 JSON response with a structured error body.
fn unauthorized_json(code: &str, message: &str) -> Response {
    let body = serde_json::json!({
        "code": code,
        "message": message,
    });
    (
        StatusCode::UNAUTHORIZED,
        [(header::CONTENT_TYPE, "application/json")],
        body.to_string(),
    )
        .into_response()
}
