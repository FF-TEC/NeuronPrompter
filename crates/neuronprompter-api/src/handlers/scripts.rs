use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use neuronprompter_application::script_service;
use neuronprompter_application::sync_service::{self, SyncReport};
use neuronprompter_core::domain::PaginatedList;
use neuronprompter_core::domain::script::{
    NewScript, Script, ScriptFilter, ScriptWithAssociations, UpdateScript,
};

use super::TogglePayload;
use crate::error::ApiError;
use crate::middleware::session::AuthUser;
use crate::state::AppState;

/// GET /api/v1/scripts/count
///
/// Returns the total number of scripts owned by the authenticated user.
///
/// # Errors
///
/// Returns HTTP 500 if the database query fails.
pub async fn count_scripts(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> Result<Json<serde_json::Value>, ApiError> {
    let uid = auth.user_id;
    let pool = state.pool.clone();
    crate::error::run_blocking(move || {
        script_service::count_scripts(&pool, uid).map_err(ApiError::from)
    })
    .await
    .map(|count| Json(serde_json::json!({ "count": count })))
}

/// POST /api/v1/scripts/search
///
/// Lists scripts for the authenticated user, filtered and paginated according
/// to the provided `ScriptFilter` body.
///
/// # Errors
///
/// Returns HTTP 500 if the database query fails.
pub async fn list_scripts(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(mut filter): Json<ScriptFilter>,
) -> Result<Json<PaginatedList<Script>>, ApiError> {
    filter.user_id = Some(auth.user_id);
    let pool = state.pool.clone();
    crate::error::run_blocking(move || {
        script_service::list_scripts(&pool, &filter).map_err(ApiError::from)
    })
    .await
    .map(Json)
}

/// GET /api/v1/scripts/{script_id}
///
/// Retrieves a single script with its associated tags, categories, and
/// collections.
///
/// # Errors
///
/// Returns HTTP 404 if the script does not exist.
/// Returns HTTP 403 if the script is not owned by the authenticated user.
/// Returns HTTP 500 if the database query fails.
pub async fn get_script(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(script_id): Path<i64>,
) -> Result<Json<ScriptWithAssociations>, ApiError> {
    let uid = auth.user_id;
    let pool = state.pool.clone();
    crate::error::run_blocking(move || {
        let swa = script_service::get_script(&pool, script_id).map_err(ApiError::from)?;
        crate::middleware::auth::check_ownership(uid, swa.script.user_id)?;
        Ok(swa)
    })
    .await
    .map(Json)
}

/// POST /api/v1/scripts
///
/// Creates a script for the authenticated user. The `user_id` field in the
/// payload is overwritten with the session user ID.
///
/// # Errors
///
/// Returns HTTP 400 if the request body fails validation.
/// Returns HTTP 500 if the database insert fails.
pub async fn create_script(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    crate::error::ValidatedJson(mut payload): crate::error::ValidatedJson<NewScript>,
) -> Result<(axum::http::StatusCode, Json<Script>), ApiError> {
    payload.user_id = auth.user_id;
    let pool = state.pool.clone();
    let script = crate::error::run_blocking(move || {
        script_service::create_script(&pool, &payload).map_err(ApiError::from)
    })
    .await?;
    Ok((axum::http::StatusCode::CREATED, Json(script)))
}

/// PUT /api/v1/scripts/{script_id}
///
/// Replaces an existing script. The path `script_id` must match the
/// `script_id` in the JSON body.
///
/// # Errors
///
/// Returns HTTP 400 if the path ID does not match the payload ID.
/// Returns HTTP 404 if the script does not exist.
/// Returns HTTP 403 if the script is not owned by the authenticated user.
/// Returns HTTP 500 if the database update fails.
pub async fn update_script(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(script_id): Path<i64>,
    Json(payload): Json<UpdateScript>,
) -> Result<Json<Script>, ApiError> {
    // Validate path param matches payload ID.
    if payload.script_id != script_id {
        return Err(ApiError::new(
            axum::http::StatusCode::BAD_REQUEST,
            "ID_MISMATCH",
            format!(
                "path script_id ({script_id}) does not match payload script_id ({})",
                payload.script_id
            ),
        ));
    }
    let uid = auth.user_id;
    let pool = state.pool.clone();
    crate::error::run_blocking(move || {
        let existing = script_service::get_script(&pool, script_id).map_err(ApiError::from)?;
        crate::middleware::auth::check_ownership(uid, existing.script.user_id)?;
        script_service::update_script(&pool, &payload).map_err(ApiError::from)
    })
    .await
    .map(Json)
}

/// DELETE /api/v1/scripts/{script_id}
///
/// Deletes a script owned by the authenticated user.
///
/// # Errors
///
/// Returns HTTP 404 if the script does not exist.
/// Returns HTTP 403 if the script is not owned by the authenticated user.
/// Returns HTTP 500 if the database deletion fails.
pub async fn delete_script(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(script_id): Path<i64>,
) -> Result<axum::http::StatusCode, ApiError> {
    let uid = auth.user_id;
    let pool = state.pool.clone();
    crate::error::run_blocking(move || {
        let existing = script_service::get_script(&pool, script_id).map_err(ApiError::from)?;
        crate::middleware::auth::check_ownership(uid, existing.script.user_id)?;
        script_service::delete_script(&pool, script_id).map_err(ApiError::from)
    })
    .await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

/// POST /api/v1/scripts/{script_id}/duplicate
///
/// Creates a copy of an existing script, including its associations.
///
/// # Errors
///
/// Returns HTTP 404 if the script does not exist.
/// Returns HTTP 403 if the script is not owned by the authenticated user.
/// Returns HTTP 500 if the database operation fails.
pub async fn duplicate_script(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(script_id): Path<i64>,
) -> Result<Json<Script>, ApiError> {
    let uid = auth.user_id;
    let pool = state.pool.clone();
    crate::error::run_blocking(move || {
        let existing = script_service::get_script(&pool, script_id).map_err(ApiError::from)?;
        crate::middleware::auth::check_ownership(uid, existing.script.user_id)?;
        script_service::duplicate_script(&pool, script_id).map_err(ApiError::from)
    })
    .await
    .map(Json)
}

/// PATCH /api/v1/scripts/{script_id}/favorite
///
/// Toggles or explicitly sets the favorite flag on a script. When the request
/// body contains a `value` field, that value is used; otherwise the current
/// flag is inverted.
///
/// # Errors
///
/// Returns HTTP 404 if the script does not exist.
/// Returns HTTP 403 if the script is not owned by the authenticated user.
/// Returns HTTP 500 if the database update fails.
pub async fn toggle_favorite(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(script_id): Path<i64>,
    body: Option<Json<TogglePayload>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let payload = body.map(|j| j.0).unwrap_or_default();
    let uid = auth.user_id;
    let pool = state.pool.clone();
    crate::error::run_blocking(move || {
        let existing = script_service::get_script(&pool, script_id).map_err(ApiError::from)?;
        crate::middleware::auth::check_ownership(uid, existing.script.user_id)?;
        let new_value = payload.value.unwrap_or(!existing.script.is_favorite);
        script_service::toggle_favorite(&pool, script_id, new_value).map_err(ApiError::from)?;
        Ok(serde_json::json!({ "is_favorite": new_value }))
    })
    .await
    .map(Json)
}

/// PATCH /api/v1/scripts/{script_id}/archive
///
/// Toggles or explicitly sets the archived flag on a script. When the request
/// body contains a `value` field, that value is used; otherwise the current
/// flag is inverted.
///
/// # Errors
///
/// Returns HTTP 404 if the script does not exist.
/// Returns HTTP 403 if the script is not owned by the authenticated user.
/// Returns HTTP 500 if the database update fails.
pub async fn toggle_archive(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(script_id): Path<i64>,
    body: Option<Json<TogglePayload>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let payload = body.map(|j| j.0).unwrap_or_default();
    let uid = auth.user_id;
    let pool = state.pool.clone();
    crate::error::run_blocking(move || {
        let existing = script_service::get_script(&pool, script_id).map_err(ApiError::from)?;
        crate::middleware::auth::check_ownership(uid, existing.script.user_id)?;
        let new_value = payload.value.unwrap_or(!existing.script.is_archived);
        script_service::toggle_archive(&pool, script_id, new_value).map_err(ApiError::from)?;
        Ok(serde_json::json!({ "is_archived": new_value }))
    })
    .await
    .map(Json)
}

#[derive(serde::Deserialize)]
pub struct SyncPayload {
    pub user_id: i64,
}

/// POST /api/v1/scripts/sync
///
/// Synchronizes on-disk script files with the database for the specified user.
/// Detects additions, modifications, and deletions on the filesystem and
/// reconciles them with stored records.
///
/// # Errors
///
/// Returns HTTP 403 if the payload `user_id` does not match the authenticated
/// user.
/// Returns HTTP 500 if the sync operation fails.
pub async fn sync_scripts(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(payload): Json<SyncPayload>,
) -> Result<Json<SyncReport>, ApiError> {
    crate::middleware::auth::verify_user_id_param(auth.user_id, payload.user_id)?;
    let pool = state.pool.clone();
    crate::error::run_blocking(move || {
        sync_service::sync_scripts(&pool, payload.user_id).map_err(ApiError::from)
    })
    .await
    .map(Json)
}

#[derive(serde::Deserialize)]
pub struct ImportFilePayload {
    pub user_id: i64,
    pub path: String,
    pub is_synced: bool,
    pub script_language: Option<String>,
}

/// POST /api/v1/scripts/import-file
///
/// Imports a single file from the local filesystem as a script record for the
/// specified user.
///
/// # Errors
///
/// Returns HTTP 403 if the payload `user_id` does not match the authenticated
/// user.
/// Returns HTTP 400 if the file path fails validation.
/// Returns HTTP 500 if the import operation fails.
pub async fn import_file(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(payload): Json<ImportFilePayload>,
) -> Result<Json<Script>, ApiError> {
    crate::middleware::auth::verify_user_id_param(auth.user_id, payload.user_id)?;
    crate::handlers::common::validate_io_path(std::path::Path::new(&payload.path))?;
    let pool = state.pool.clone();
    crate::error::run_blocking(move || {
        sync_service::import_file(
            &pool,
            payload.user_id,
            &payload.path,
            payload.is_synced,
            payload.script_language.as_deref(),
        )
        .map_err(ApiError::from)
    })
    .await
    .map(Json)
}

/// GET /api/v1/scripts/languages
///
/// Returns the distinct set of languages used across all scripts owned by the
/// authenticated user.
///
/// # Errors
///
/// Returns HTTP 500 if the database query fails.
pub async fn list_languages(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> Result<Json<Vec<String>>, ApiError> {
    let uid = auth.user_id;
    let pool = state.pool.clone();
    crate::error::run_blocking(move || {
        script_service::list_script_languages(&pool, uid).map_err(ApiError::from)
    })
    .await
    .map(Json)
}

/// POST /api/v1/scripts/bulk-update
///
/// Applies bulk changes (favorite, archived, tag/category/collection
/// additions and removals) to multiple scripts at once.
///
/// # Errors
///
/// Returns HTTP 400 if the `ids` array is empty or exceeds the maximum
/// allowed count.
/// Returns HTTP 500 if the database operation fails.
pub async fn bulk_update(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(payload): Json<super::BulkUpdatePayload>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if payload.ids.is_empty() {
        return Err(ApiError::new(
            axum::http::StatusCode::BAD_REQUEST,
            "VALIDATION_ERROR",
            "ids must not be empty".to_owned(),
        ));
    }
    if payload.ids.len() > super::MAX_BULK_IDS {
        return Err(ApiError::new(
            axum::http::StatusCode::BAD_REQUEST,
            "VALIDATION_ERROR",
            format!("too many IDs, maximum is {}", super::MAX_BULK_IDS),
        ));
    }
    let active_uid = auth.user_id;
    let pool = state.pool.clone();
    let input = neuronprompter_application::association_sync::BulkUpdateInput {
        ids: payload.ids,
        set_favorite: payload.set_favorite,
        set_archived: payload.set_archived,
        add_tag_ids: payload.add_tag_ids,
        remove_tag_ids: payload.remove_tag_ids,
        add_category_ids: payload.add_category_ids,
        remove_category_ids: payload.remove_category_ids,
        add_collection_ids: payload.add_collection_ids,
        remove_collection_ids: payload.remove_collection_ids,
    };
    crate::error::run_blocking(move || {
        script_service::bulk_update(&pool, active_uid, &input).map_err(ApiError::from)
    })
    .await
    .map(|count| Json(serde_json::json!({"updated": count})))
}
