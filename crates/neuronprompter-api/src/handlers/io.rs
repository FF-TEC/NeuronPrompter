use std::path::PathBuf;
use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use neuronprompter_application::io_service::{self, ImportSummary};
use neuronprompter_core::paths;
use neuronprompter_db::ConnectionProvider;

use crate::error::ApiError;
use crate::handlers::common::validate_io_path;
use crate::middleware::session::AuthUser;
use crate::state::AppState;

#[derive(serde::Deserialize)]
pub struct ExportJsonPayload {
    pub user_id: i64,
    #[serde(default)]
    pub prompt_ids: Vec<i64>,
    pub path: PathBuf,
}

/// POST /api/v1/io/export/json
///
/// Exports prompts for the authenticated user as a JSON file to the specified
/// filesystem path. When `prompt_ids` is empty, all prompts for the user are
/// exported.
///
/// # Errors
///
/// Returns HTTP 403 if the authenticated user does not match the payload `user_id`.
/// Returns HTTP 400 if the export path fails validation.
/// Returns HTTP 500 if an internal database or filesystem error occurs.
pub async fn export_json(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(payload): Json<ExportJsonPayload>,
) -> Result<Json<()>, ApiError> {
    crate::middleware::auth::verify_user_id_param(auth.user_id, payload.user_id)?;
    // Path is validated against the user's home directory at the handler level.
    // Additional sanitization (symlink resolution, null-byte rejection) is
    // performed by the service layer.
    validate_io_path(&payload.path)?;
    let pool = state.pool.clone();
    crate::error::run_blocking(move || {
        let effective_ids = if payload.prompt_ids.is_empty() {
            let all = pool
                .with_connection(|conn| {
                    neuronprompter_db::repo::prompts::list_all_prompts(conn, payload.user_id)
                })
                .map_err(ApiError::from)?;
            all.iter().map(|p| p.id).collect()
        } else {
            payload.prompt_ids
        };
        io_service::export_json(&pool, payload.user_id, &effective_ids, &payload.path)
            .map_err(ApiError::from)
    })
    .await
    .map(Json)
}

#[derive(serde::Deserialize)]
pub struct ImportJsonPayload {
    pub user_id: i64,
    pub path: PathBuf,
}

/// POST /api/v1/io/import/json
///
/// Imports prompts from a JSON file at the specified filesystem path into the
/// authenticated user's account.
///
/// # Errors
///
/// Returns HTTP 403 if the authenticated user does not match the payload `user_id`.
/// Returns HTTP 400 if the import path fails validation.
/// Returns HTTP 500 if an internal database or filesystem error occurs.
pub async fn import_json(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(payload): Json<ImportJsonPayload>,
) -> Result<Json<ImportSummary>, ApiError> {
    crate::middleware::auth::verify_user_id_param(auth.user_id, payload.user_id)?;
    validate_io_path(&payload.path)?;
    let pool = state.pool.clone();
    crate::error::run_blocking(move || {
        io_service::import_json(&pool, payload.user_id, &payload.path).map_err(ApiError::from)
    })
    .await
    .map(Json)
}

#[derive(serde::Deserialize)]
pub struct ExportMarkdownPayload {
    pub user_id: i64,
    #[serde(default)]
    pub prompt_ids: Vec<i64>,
    pub dir_path: PathBuf,
}

/// POST /api/v1/io/export/markdown
///
/// Exports prompts for the authenticated user as individual Markdown files into
/// the specified directory. When `prompt_ids` is empty, all prompts for the user
/// are exported.
///
/// # Errors
///
/// Returns HTTP 403 if the authenticated user does not match the payload `user_id`.
/// Returns HTTP 400 if the directory path fails validation.
/// Returns HTTP 500 if an internal database or filesystem error occurs.
pub async fn export_markdown(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(payload): Json<ExportMarkdownPayload>,
) -> Result<Json<()>, ApiError> {
    crate::middleware::auth::verify_user_id_param(auth.user_id, payload.user_id)?;
    validate_io_path(&payload.dir_path)?;
    let pool = state.pool.clone();
    crate::error::run_blocking(move || {
        let effective_ids = if payload.prompt_ids.is_empty() {
            let all = pool
                .with_connection(|conn| {
                    neuronprompter_db::repo::prompts::list_all_prompts(conn, payload.user_id)
                })
                .map_err(ApiError::from)?;
            all.iter().map(|p| p.id).collect()
        } else {
            payload.prompt_ids
        };
        io_service::export_markdown(&pool, payload.user_id, &effective_ids, &payload.dir_path)
            .map_err(ApiError::from)
    })
    .await
    .map(Json)
}

#[derive(serde::Deserialize)]
pub struct ImportMarkdownPayload {
    pub user_id: i64,
    pub dir_path: PathBuf,
}

/// POST /api/v1/io/import/markdown
///
/// Imports prompts from Markdown files in the specified directory into the
/// authenticated user's account.
///
/// # Errors
///
/// Returns HTTP 403 if the authenticated user does not match the payload `user_id`.
/// Returns HTTP 400 if the directory path fails validation.
/// Returns HTTP 500 if an internal database or filesystem error occurs.
pub async fn import_markdown(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(payload): Json<ImportMarkdownPayload>,
) -> Result<Json<ImportSummary>, ApiError> {
    crate::middleware::auth::verify_user_id_param(auth.user_id, payload.user_id)?;
    validate_io_path(&payload.dir_path)?;
    let pool = state.pool.clone();
    crate::error::run_blocking(move || {
        io_service::import_markdown(&pool, payload.user_id, &payload.dir_path)
            .map_err(ApiError::from)
    })
    .await
    .map(Json)
}

#[derive(serde::Deserialize)]
pub struct BackupPayload {
    pub target_path: PathBuf,
}

/// POST /api/v1/io/backup
///
/// Creates a backup copy of the application database at the specified target
/// path. The target path must reside within the application backups directory.
///
/// # Errors
///
/// Returns HTTP 400 if the target path is outside the backups directory.
/// Returns HTTP 500 if the backups directory cannot be created or the backup
/// operation fails.
pub async fn backup_database(
    State(_state): State<Arc<AppState>>,
    _auth: AuthUser,
    Json(payload): Json<BackupPayload>,
) -> Result<Json<()>, ApiError> {
    // F4: Derive source database path internally -- never trust the client.
    let source = paths::db_path();

    // F4: Ensure backups directory exists before validating the target path.
    let backups = paths::backups_dir();
    std::fs::create_dir_all(&backups)
        .map_err(|e| ApiError::internal(format!("failed to create backups directory: {e}")))?;

    // Validate target_path is within the backups directory.
    paths::validate_path_within(&payload.target_path, &backups).map_err(ApiError::from)?;
    let target = payload.target_path;

    crate::error::run_blocking(move || {
        io_service::backup_database(&source, &target).map_err(ApiError::from)
    })
    .await
    .map(Json)
}
