use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use serde::Serialize;

use crate::middleware::session::AuthUser;
use crate::state::AppState;

/// Response body for GET /api/v1/health.
/// Reports application status and version. Does not expose internal paths.
#[derive(Serialize)]
pub struct HealthResponse {
    /// Fixed "ok" string indicating the server is running.
    pub status: String,
    /// Application version from workspace Cargo.toml.
    pub version: String,
}

/// Handler for GET /api/v1/health.
pub async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_owned(),
        version: env!("CARGO_PKG_VERSION").to_owned(),
    })
}

/// Response body for GET /api/v1/settings/db-path.
#[derive(Serialize)]
pub struct DbPathResponse {
    /// Absolute path to the SQLite database file.
    pub db_path: String,
}

/// Handler for GET /api/v1/settings/db-path.
/// Returns the database file path. Requires authentication to prevent
/// leaking internal filesystem paths to unauthenticated clients.
pub async fn get_db_path(
    State(_state): State<Arc<AppState>>,
    _auth: AuthUser,
) -> Json<DbPathResponse> {
    Json(DbPathResponse {
        db_path: neuronprompter_core::paths::db_path().display().to_string(),
    })
}
