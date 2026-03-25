// =============================================================================
// Session management API handlers.
//
// Provides endpoints for creating, switching, inspecting, and terminating
// user sessions. These replace the old `switch_user` / `active_user_id`
// pattern with proper per-client session tokens stored in cookies.
// =============================================================================

use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};

use neuronprompter_application::user_service;

use crate::error::{ApiError, run_blocking};
use crate::middleware::session::{AuthSession, extract_cookie_token, set_session_cookie};
use crate::state::AppState;

#[derive(Deserialize)]
pub struct CreateSessionPayload {
    pub user_id: i64,
}

/// POST /api/v1/sessions -- create a session for the given user.
///
/// # Errors
///
/// Returns HTTP 404 if the user does not exist.
/// Returns HTTP 500 if the database operation fails.
pub async fn create_session(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<CreateSessionPayload>,
) -> Result<Response, ApiError> {
    let pool = state.pool.clone();
    let user_id = payload.user_id;

    // Verify user exists.
    run_blocking(move || user_service::get_user(&pool, user_id).map_err(ApiError::from)).await?;

    // Also persist as last_user_id for auto-session resolution.
    let pool2 = state.pool.clone();
    run_blocking(move || user_service::switch_user(&pool2, user_id).map_err(ApiError::from))
        .await?;

    let ip = std::net::IpAddr::from([127, 0, 0, 1]);
    let token = state.sessions.create_session(ip, Some(user_id));

    let body = serde_json::json!({ "ok": true });
    let response = (StatusCode::CREATED, Json(body)).into_response();
    Ok(set_session_cookie(
        response,
        &token,
        state.sessions.is_localhost,
    ))
}

#[derive(Deserialize)]
pub struct SwitchSessionPayload {
    pub user_id: i64,
}

/// PUT /api/v1/sessions/switch -- switch the user in the current session.
///
/// # Errors
///
/// Returns HTTP 404 if the user does not exist.
/// Returns HTTP 401 if the session is not found.
/// Returns HTTP 500 if the database operation fails.
pub async fn switch_session(
    State(state): State<Arc<AppState>>,
    auth: AuthSession,
    Json(payload): Json<SwitchSessionPayload>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let pool = state.pool.clone();
    let user_id = payload.user_id;

    // Verify user exists.
    run_blocking(move || user_service::get_user(&pool, user_id).map_err(ApiError::from)).await?;

    // Update session.
    if !state
        .sessions
        .set_session_user(&auth.session_token, user_id)
    {
        return Err(ApiError::new(
            StatusCode::UNAUTHORIZED,
            "AUTH_REQUIRED",
            "session not found".to_owned(),
        ));
    }

    // Persist as last_user_id.
    let pool2 = state.pool.clone();
    run_blocking(move || user_service::switch_user(&pool2, user_id).map_err(ApiError::from))
        .await?;

    Ok(Json(serde_json::json!({ "ok": true })))
}

/// DELETE /api/v1/sessions -- logout, remove the session, clear cookie.
pub async fn logout(State(state): State<Arc<AppState>>, auth: AuthSession) -> Response {
    state.sessions.remove_session(&auth.session_token);

    let body = serde_json::json!({ "ok": true });
    let response = (StatusCode::OK, Json(body)).into_response();
    // Clear the cookie by setting Max-Age=0.
    let cookie_value = "np_session=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0";
    let mut response = response;
    if let Ok(val) = axum::http::HeaderValue::from_str(cookie_value) {
        response
            .headers_mut()
            .append(axum::http::header::SET_COOKIE, val);
    }
    response
}

/// Response body for the GET /api/v1/sessions/me endpoint.
/// The session token is deliberately excluded from this response to avoid
/// leaking it through the JSON body. The token is only transmitted via
/// the HttpOnly session cookie.
#[derive(Serialize)]
pub struct SessionMeResponse {
    pub user: Option<SessionMeUser>,
    pub remaining_ttl_secs: Option<u64>,
}

#[derive(Serialize)]
pub struct SessionMeUser {
    pub id: i64,
    pub username: String,
    pub display_name: String,
}

/// GET /api/v1/sessions/me -- returns current session info.
/// Does NOT require auth; returns null user if no session.
///
/// # Errors
///
/// Returns HTTP 500 if the database lookup for the session user fails.
pub async fn session_me(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> Result<Json<SessionMeResponse>, ApiError> {
    let token = extract_cookie_token(&headers);

    let Some(token) = token else {
        return Ok(Json(SessionMeResponse {
            user: None,
            remaining_ttl_secs: None,
        }));
    };

    let session_ref = state.sessions.get_session(&token);
    let Some(session_ref) = session_ref else {
        return Ok(Json(SessionMeResponse {
            user: None,
            remaining_ttl_secs: None,
        }));
    };

    let user_id = session_ref.user_id();
    let elapsed = session_ref.created_at.elapsed();
    let ttl = state.sessions.session_ttl();
    let remaining = ttl.saturating_sub(elapsed).as_secs();
    drop(session_ref);

    let user = if let Some(uid) = user_id {
        let pool = state.pool.clone();
        let result =
            run_blocking(move || user_service::get_user(&pool, uid).map_err(ApiError::from)).await;
        match result {
            Ok(u) => Some(SessionMeUser {
                id: u.id,
                username: u.username,
                display_name: u.display_name,
            }),
            Err(_) => None,
        }
    } else {
        None
    };

    Ok(Json(SessionMeResponse {
        user,
        remaining_ttl_secs: Some(remaining),
    }))
}
