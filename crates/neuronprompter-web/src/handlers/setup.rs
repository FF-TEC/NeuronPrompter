// =============================================================================
// First-run setup status and completion handlers.
//
// The frontend calls these endpoints on startup to decide whether to show the
// welcome dialog. A `.setup_complete` marker file tracks whether the first-run
// flow has been completed. As a safety net the handler also checks whether at
// least one user exists in the database -- even if the marker file is present
// a missing user should trigger the welcome flow.
//
// Endpoints:
// - GET  /api/v1/web/setup/status   -- check first-run state
// - POST /api/v1/web/setup/complete -- mark setup as done
// =============================================================================

use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use serde::Serialize;
use tracing::error;

use crate::WebState;

/// Helper to build a consistent JSON error response.
fn error_json(code: &str, message: &str) -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({ "code": code, "message": message })),
    )
}

/// Response for `GET /api/v1/web/setup/status`.
#[derive(Debug, Serialize)]
pub struct SetupStatus {
    /// `true` when the welcome dialog should be shown.
    pub is_first_run: bool,
    /// `true` when at least one user exists in the database.
    pub has_users: bool,
    /// Absolute path to the data directory. Only populated during first-run
    /// to avoid exposing internal paths to authenticated non-setup requests.
    pub data_dir: Option<String>,
}

/// GET /api/v1/web/setup/status
///
/// Returns whether this is a first-run scenario. The check combines two
/// signals: the presence of the `.setup_complete` marker file **and** the
/// existence of at least one user row in the database.
pub async fn get_setup_status(State(web_state): State<Arc<WebState>>) -> Json<SetupStatus> {
    let marker_exists = neuronprompter_core::paths::setup_complete_path().exists();

    let has_users = {
        let pool = web_state.app_state.pool.clone();
        match pool.get() {
            Ok(conn) => match neuronprompter_db::repo::users::list_users(&conn) {
                Ok(users) => !users.is_empty(),
                Err(_) => false,
            },
            Err(_) => false,
        }
    };

    let is_first_run = !marker_exists || !has_users;
    let data_dir = if is_first_run {
        Some(
            neuronprompter_core::paths::base_dir()
                .to_string_lossy()
                .into_owned(),
        )
    } else {
        None
    };

    Json(SetupStatus {
        is_first_run,
        has_users,
        data_dir,
    })
}

/// POST /api/v1/web/setup/complete
///
/// Creates the `.setup_complete` marker file so the welcome dialog is not
/// shown on subsequent launches.
///
/// # Errors
///
/// Returns a 400 JSON error if no users exist in the database (setup
/// prerequisite not met). Returns a 500 JSON error if the marker file
/// directory cannot be created or the marker file cannot be written.
pub async fn mark_setup_complete(
    State(web_state): State<Arc<WebState>>,
) -> Result<StatusCode, impl IntoResponse> {
    // Reject if no users exist to prevent network-adjacent attackers from
    // bypassing the first-run flow by calling this endpoint directly.
    let has_users = {
        let pool = web_state.app_state.pool.clone();
        match pool.get() {
            Ok(conn) => match neuronprompter_db::repo::users::list_users(&conn) {
                Ok(users) => !users.is_empty(),
                Err(_) => false,
            },
            Err(_) => false,
        }
    };
    if !has_users {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "code": "SETUP_INCOMPLETE",
                "message": "at least one user must exist before completing setup"
            })),
        ));
    }

    let path = neuronprompter_core::paths::setup_complete_path();

    if let Some(parent) = path.parent()
        && let Err(e) = std::fs::create_dir_all(parent)
    {
        error!("failed to create setup marker directory: {e}");
        return Err(error_json(
            "SETUP_DIR_FAILED",
            &format!("failed to create directory: {e}"),
        ));
    }

    match std::fs::write(&path, b"") {
        Ok(()) => Ok(StatusCode::OK),
        Err(e) => {
            error!("failed to write setup marker: {e}");
            Err(error_json(
                "SETUP_WRITE_FAILED",
                &format!("failed to write marker: {e}"),
            ))
        }
    }
}
