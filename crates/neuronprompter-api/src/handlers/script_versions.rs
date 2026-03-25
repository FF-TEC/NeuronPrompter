use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use neuronprompter_application::{script_service, script_version_service};
use neuronprompter_core::domain::script::Script;
use neuronprompter_core::domain::script_version::ScriptVersion;

use crate::error::ApiError;
use crate::middleware::session::AuthUser;
use crate::state::AppState;

/// GET /api/v1/script-versions/script/{script_id}
///
/// Lists all version history entries for a script. Ownership of the parent
/// script is verified before returning results.
///
/// # Errors
///
/// Returns HTTP 404 if the script does not exist.
/// Returns HTTP 403 if the authenticated user does not own the script.
/// Returns HTTP 500 if an internal database error occurs.
pub async fn list_script_versions(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(script_id): Path<i64>,
) -> Result<Json<Vec<ScriptVersion>>, ApiError> {
    let uid = auth.user_id;
    let pool = state.pool.clone();
    crate::error::run_blocking(move || {
        // Verify ownership of the parent script.
        let swa = script_service::get_script(&pool, script_id).map_err(ApiError::from)?;
        crate::middleware::auth::check_ownership(uid, swa.script.user_id)?;
        script_version_service::list_versions(&pool, script_id).map_err(ApiError::from)
    })
    .await
    .map(Json)
}

/// GET /api/v1/script-versions/{version_id}
///
/// Retrieves a single script version by its ID. Ownership is verified through
/// the parent script.
///
/// # Errors
///
/// Returns HTTP 404 if the version or its parent script does not exist.
/// Returns HTTP 403 if the authenticated user does not own the parent script.
/// Returns HTTP 500 if an internal database error occurs.
pub async fn get_script_version(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(version_id): Path<i64>,
) -> Result<Json<ScriptVersion>, ApiError> {
    let uid = auth.user_id;
    let pool = state.pool.clone();
    crate::error::run_blocking(move || {
        let version =
            script_version_service::get_version(&pool, version_id).map_err(ApiError::from)?;
        // Verify ownership via the parent script.
        let swa = script_service::get_script(&pool, version.script_id).map_err(ApiError::from)?;
        crate::middleware::auth::check_ownership(uid, swa.script.user_id)?;
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

/// POST /api/v1/script-versions/script/{script_id}/restore
///
/// Restores a script to a previous version, identified by either
/// `version_number` or `version_id`. Exactly one of the two fields must be
/// provided.
///
/// # Errors
///
/// Returns HTTP 404 if the script or the specified version does not exist.
/// Returns HTTP 403 if the authenticated user does not own the script.
/// Returns HTTP 400 if neither or both of `version_number` and `version_id`
/// are provided, or if the version does not belong to the specified script.
/// Returns HTTP 500 if an internal database error occurs.
pub async fn restore_script_version(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(script_id): Path<i64>,
    Json(payload): Json<RestorePayload>,
) -> Result<Json<Script>, ApiError> {
    let session_user = auth.user_id;
    let pool = state.pool.clone();
    crate::error::run_blocking(move || {
        // Verify ownership of the parent script.
        let swa = script_service::get_script(&pool, script_id).map_err(ApiError::from)?;
        crate::middleware::auth::check_ownership(session_user, swa.script.user_id)?;

        let version_number = match (payload.version_number, payload.version_id) {
            (Some(vn), None) => vn,
            (None, Some(vid)) => {
                let v = script_version_service::get_version(&pool, vid).map_err(ApiError::from)?;
                if v.script_id != script_id {
                    return Err(ApiError::new(
                        axum::http::StatusCode::BAD_REQUEST,
                        "VALIDATION_ERROR",
                        format!("Version {vid} does not belong to script {script_id}"),
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

        script_version_service::restore_version(&pool, script_id, version_number)
            .map_err(ApiError::from)
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
    pub version_a: ScriptVersion,
    pub version_b: ScriptVersion,
}

/// GET /api/v1/script-versions/compare
///
/// Retrieves two script versions for side-by-side comparison. Both versions
/// must belong to the same script, and the authenticated user must own that
/// script.
///
/// # Errors
///
/// Returns HTTP 404 if either version or the parent script does not exist.
/// Returns HTTP 403 if the authenticated user does not own the parent script.
/// Returns HTTP 400 if the two versions belong to different scripts.
/// Returns HTTP 500 if an internal database error occurs.
pub async fn compare_script_versions(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    axum::extract::Query(params): axum::extract::Query<CompareQuery>,
) -> Result<Json<CompareResponse>, ApiError> {
    let pool = state.pool.clone();
    let uid = auth.user_id;
    crate::error::run_blocking(move || {
        let va =
            script_version_service::get_version(&pool, params.version_a).map_err(ApiError::from)?;
        let vb =
            script_version_service::get_version(&pool, params.version_b).map_err(ApiError::from)?;
        if va.script_id != vb.script_id {
            return Err(ApiError::new(
                axum::http::StatusCode::BAD_REQUEST,
                "VALIDATION_ERROR",
                "Both versions must belong to the same script".to_owned(),
            ));
        }
        let swa = script_service::get_script(&pool, va.script_id).map_err(ApiError::from)?;
        crate::middleware::auth::check_ownership(uid, swa.script.user_id)?;
        Ok(CompareResponse {
            version_a: va,
            version_b: vb,
        })
    })
    .await
    .map(Json)
}
