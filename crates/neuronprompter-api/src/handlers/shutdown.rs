use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::state::AppState;

#[derive(Deserialize)]
pub struct ShutdownPayload {
    pub token: String,
}

#[derive(Serialize)]
pub struct ShutdownResponse {
    pub message: String,
}

/// POST /api/v1/shutdown
///
/// Initiates a graceful server shutdown. The request body must contain the
/// correct session token, which is compared in constant time to prevent
/// timing side-channel attacks.
///
/// # Errors
///
/// Returns HTTP 403 if the provided token does not match the server's session
/// token.
pub async fn shutdown(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<ShutdownPayload>,
) -> Result<Json<ShutdownResponse>, ApiError> {
    use subtle::ConstantTimeEq;
    // Constant-time comparison to prevent timing side-channel attacks on the
    // shutdown token. The session_token() accessor is used instead of direct
    // field access because the field is pub(crate).
    if bool::from(
        payload
            .token
            .as_bytes()
            .ct_eq(state.session_token().as_bytes()),
    ) {
        state.cancellation.cancel();
        Ok(Json(ShutdownResponse {
            message: "shutdown initiated".to_owned(),
        }))
    } else {
        Err(ApiError::new(
            axum::http::StatusCode::FORBIDDEN,
            "FORBIDDEN",
            "invalid shutdown token".to_owned(),
        ))
    }
}
