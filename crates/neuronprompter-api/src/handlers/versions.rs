use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use neuronprompter_application::{prompt_service, version_service};
use neuronprompter_core::domain::prompt::Prompt;
use neuronprompter_core::domain::version::PromptVersion;

use crate::error::ApiError;
use crate::middleware::session::AuthUser;
use crate::state::AppState;

/// GET /api/v1/versions/prompt/{prompt_id}
///
/// Lists all version history entries for a prompt. Ownership of the parent
/// prompt is verified before returning results.
///
/// # Errors
///
/// Returns HTTP 404 if the prompt does not exist.
/// Returns HTTP 403 if the authenticated user does not own the prompt.
/// Returns HTTP 500 if an internal database error occurs.
pub async fn list_versions(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(prompt_id): Path<i64>,
) -> Result<Json<Vec<PromptVersion>>, ApiError> {
    let uid = auth.user_id;
    let pool = state.pool.clone();
    crate::error::run_blocking(move || {
        // Verify ownership of the parent prompt.
        let pwa = prompt_service::get_prompt(&pool, prompt_id).map_err(ApiError::from)?;
        crate::middleware::auth::check_ownership(uid, pwa.prompt.user_id)?;
        version_service::list_versions(&pool, prompt_id).map_err(ApiError::from)
    })
    .await
    .map(Json)
}

/// GET /api/v1/versions/{version_id}
///
/// Retrieves a single prompt version by its ID. Ownership is verified through
/// the parent prompt.
///
/// # Errors
///
/// Returns HTTP 404 if the version or its parent prompt does not exist.
/// Returns HTTP 403 if the authenticated user does not own the parent prompt.
/// Returns HTTP 500 if an internal database error occurs.
pub async fn get_version(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(version_id): Path<i64>,
) -> Result<Json<PromptVersion>, ApiError> {
    let uid = auth.user_id;
    let pool = state.pool.clone();
    crate::error::run_blocking(move || {
        let version = version_service::get_version(&pool, version_id).map_err(ApiError::from)?;
        // Verify ownership via the parent prompt.
        let pwa = prompt_service::get_prompt(&pool, version.prompt_id).map_err(ApiError::from)?;
        crate::middleware::auth::check_ownership(uid, pwa.prompt.user_id)?;
        Ok(version)
    })
    .await
    .map(Json)
}

#[derive(serde::Deserialize)]
pub struct RestorePayload {
    pub version_number: Option<i64>,
    pub version_id: Option<i64>,
}

/// POST /api/v1/versions/prompt/{prompt_id}/restore
///
/// Restores a prompt to a previous version, identified by either
/// `version_number` or `version_id`. Exactly one of the two fields must be
/// provided.
///
/// # Errors
///
/// Returns HTTP 404 if the prompt or the specified version does not exist.
/// Returns HTTP 403 if the authenticated user does not own the prompt.
/// Returns HTTP 400 if neither or both of `version_number` and `version_id`
/// are provided, or if the version does not belong to the specified prompt.
/// Returns HTTP 500 if an internal database error occurs.
pub async fn restore_version(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(prompt_id): Path<i64>,
    Json(payload): Json<RestorePayload>,
) -> Result<Json<Prompt>, ApiError> {
    let session_user = auth.user_id;
    let pool = state.pool.clone();
    crate::error::run_blocking(move || {
        // Verify ownership of the parent prompt.
        let pwa = prompt_service::get_prompt(&pool, prompt_id).map_err(ApiError::from)?;
        crate::middleware::auth::check_ownership(session_user, pwa.prompt.user_id)?;

        let version_number = match (payload.version_number, payload.version_id) {
            (Some(vn), None) => vn,
            (None, Some(vid)) => {
                let v = version_service::get_version(&pool, vid).map_err(ApiError::from)?;
                if v.prompt_id != prompt_id {
                    return Err(ApiError::new(
                        axum::http::StatusCode::BAD_REQUEST,
                        "VALIDATION_ERROR",
                        format!("Version {vid} does not belong to prompt {prompt_id}"),
                    ));
                }
                v.version_number
            }
            _ => {
                return Err(ApiError::new(
                    axum::http::StatusCode::BAD_REQUEST,
                    "VALIDATION_ERROR",
                    "Provide exactly one of version_number or version_id".to_owned(),
                ));
            }
        };

        version_service::restore_version(&pool, prompt_id, version_number).map_err(ApiError::from)
    })
    .await
    .map(Json)
}

#[derive(serde::Deserialize)]
pub struct CompareQuery {
    pub version_a: i64,
    pub version_b: i64,
}

#[derive(serde::Serialize)]
pub struct CompareResponse {
    pub version_a: PromptVersion,
    pub version_b: PromptVersion,
}

/// GET /api/v1/versions/compare
///
/// Retrieves two prompt versions for side-by-side comparison. Both versions
/// must belong to the same prompt, and the authenticated user must own that
/// prompt.
///
/// # Errors
///
/// Returns HTTP 404 if either version or the parent prompt does not exist.
/// Returns HTTP 403 if the authenticated user does not own the parent prompt.
/// Returns HTTP 400 if the two versions belong to different prompts.
/// Returns HTTP 500 if an internal database error occurs.
pub async fn compare_versions(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    axum::extract::Query(params): axum::extract::Query<CompareQuery>,
) -> Result<Json<CompareResponse>, ApiError> {
    let pool = state.pool.clone();
    let uid = auth.user_id;
    crate::error::run_blocking(move || {
        let va = version_service::get_version(&pool, params.version_a).map_err(ApiError::from)?;
        let vb = version_service::get_version(&pool, params.version_b).map_err(ApiError::from)?;
        // Verify both versions belong to the same prompt.
        if va.prompt_id != vb.prompt_id {
            return Err(ApiError::new(
                axum::http::StatusCode::BAD_REQUEST,
                "VALIDATION_ERROR",
                "Both versions must belong to the same prompt".to_owned(),
            ));
        }
        // Verify ownership via the parent prompt.
        let pwa = prompt_service::get_prompt(&pool, va.prompt_id).map_err(ApiError::from)?;
        crate::middleware::auth::check_ownership(uid, pwa.prompt.user_id)?;
        Ok(CompareResponse {
            version_a: va,
            version_b: vb,
        })
    })
    .await
    .map(Json)
}
