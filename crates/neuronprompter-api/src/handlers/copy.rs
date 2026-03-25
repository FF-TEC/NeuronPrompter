// =============================================================================
// Cross-user copy API handlers.
//
// Provides endpoints for copying prompts, scripts, and chains between users,
// as well as bulk-copying all content from one user to another.
// =============================================================================

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use serde::Deserialize;

use neuronprompter_application::copy_service::{self, CopySummary};
use neuronprompter_application::{chain_service, prompt_service, script_service};

use crate::error::ApiError;
use crate::middleware::auth::check_ownership;
use crate::middleware::session::AuthUser;
use crate::state::AppState;

/// Request body for single-item copy operations.
#[derive(Deserialize)]
pub struct CopyToUserRequest {
    pub target_user_id: i64,
}

/// Request body for bulk copy operations.
#[derive(Deserialize)]
pub struct BulkCopyRequest {
    pub source_user_id: i64,
    pub target_user_id: i64,
}

/// POST /api/v1/prompts/{prompt_id}/copy-to-user
///
/// Copies a prompt and its version history to another user.
///
/// # Errors
///
/// Returns HTTP 404 if the prompt does not exist or is not owned by the caller.
/// Returns HTTP 500 if the copy operation fails.
pub async fn copy_prompt_to_user(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(prompt_id): Path<i64>,
    Json(body): Json<CopyToUserRequest>,
) -> Result<Json<CopySummary>, ApiError> {
    let uid = auth.user_id;
    let pool = state.pool.clone();
    let target_user_id = body.target_user_id;
    tracing::info!(prompt_id, target_user_id, "copying prompt to user");
    crate::error::run_blocking(move || {
        // Verify the caller owns the source prompt.
        let source = prompt_service::get_prompt(&pool, prompt_id).map_err(ApiError::from)?;
        check_ownership(uid, source.prompt.user_id)?;
        copy_service::copy_prompt_to_user(&pool, prompt_id, target_user_id).map_err(ApiError::from)
    })
    .await
    .map(Json)
}

/// POST /api/v1/scripts/{script_id}/copy-to-user
///
/// Copies a script and its version history to another user.
///
/// # Errors
///
/// Returns HTTP 404 if the script does not exist or is not owned by the caller.
/// Returns HTTP 500 if the copy operation fails.
pub async fn copy_script_to_user(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(script_id): Path<i64>,
    Json(body): Json<CopyToUserRequest>,
) -> Result<Json<CopySummary>, ApiError> {
    let uid = auth.user_id;
    let pool = state.pool.clone();
    let target_user_id = body.target_user_id;
    tracing::info!(script_id, target_user_id, "copying script to user");
    crate::error::run_blocking(move || {
        // Verify the caller owns the source script.
        let source = script_service::get_script(&pool, script_id).map_err(ApiError::from)?;
        check_ownership(uid, source.script.user_id)?;
        copy_service::copy_script_to_user(&pool, script_id, target_user_id).map_err(ApiError::from)
    })
    .await
    .map(Json)
}

/// POST /api/v1/chains/{chain_id}/copy-to-user
///
/// Deep-copies a chain, its steps, and referenced prompts/scripts to another user.
///
/// # Errors
///
/// Returns HTTP 404 if the chain does not exist or is not owned by the caller.
/// Returns HTTP 500 if the copy operation fails.
pub async fn copy_chain_to_user(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(chain_id): Path<i64>,
    Json(body): Json<CopyToUserRequest>,
) -> Result<Json<CopySummary>, ApiError> {
    let uid = auth.user_id;
    let pool = state.pool.clone();
    let target_user_id = body.target_user_id;
    tracing::info!(
        chain_id,
        target_user_id,
        "copying chain to user (deep copy)"
    );
    crate::error::run_blocking(move || {
        // Verify the caller owns the source chain.
        let source = chain_service::get_chain(&pool, chain_id).map_err(ApiError::from)?;
        check_ownership(uid, source.chain.user_id)?;
        copy_service::copy_chain_to_user(&pool, chain_id, target_user_id).map_err(ApiError::from)
    })
    .await
    .map(Json)
}

/// POST /api/v1/users/bulk-copy
///
/// Copies all prompts, scripts, and chains from one user to another.
///
/// # Errors
///
/// Returns HTTP 403 if the caller is not the source user.
/// Returns HTTP 500 if the copy operation fails.
pub async fn bulk_copy_all(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(body): Json<BulkCopyRequest>,
) -> Result<Json<CopySummary>, ApiError> {
    // Verify the caller is the source user.
    crate::middleware::auth::verify_user_id_param(auth.user_id, body.source_user_id)?;
    let pool = state.pool.clone();
    let source_user_id = body.source_user_id;
    let target_user_id = body.target_user_id;
    tracing::info!(
        source_user_id,
        target_user_id,
        "bulk copying all content between users"
    );
    crate::error::run_blocking(move || {
        copy_service::bulk_copy_all(&pool, source_user_id, target_user_id).map_err(ApiError::from)
    })
    .await
    .map(Json)
}
