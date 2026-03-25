use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use neuronprompter_application::prompt_service;
use neuronprompter_core::domain::PaginatedList;
use neuronprompter_core::domain::prompt::{
    NewPrompt, Prompt, PromptFilter, PromptWithAssociations, UpdatePrompt,
};

use super::TogglePayload;
use crate::error::ApiError;
use crate::middleware::session::AuthUser;
use crate::state::AppState;

/// GET /api/v1/prompts/count
///
/// Returns the total number of prompts owned by the authenticated user.
///
/// # Errors
///
/// Returns HTTP 500 if the database query fails.
pub async fn count_prompts(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> Result<Json<serde_json::Value>, ApiError> {
    let uid = auth.user_id;
    let pool = state.pool.clone();
    crate::error::run_blocking(move || {
        prompt_service::count_prompts(&pool, uid).map_err(ApiError::from)
    })
    .await
    .map(|count| Json(serde_json::json!({ "count": count })))
}

/// POST /api/v1/prompts/search
///
/// Lists prompts for the authenticated user, filtered and paginated according
/// to the provided `PromptFilter` body.
///
/// # Errors
///
/// Returns HTTP 500 if the database query fails.
pub async fn list_prompts(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(mut filter): Json<PromptFilter>,
) -> Result<Json<PaginatedList<Prompt>>, ApiError> {
    filter.user_id = Some(auth.user_id);
    let pool = state.pool.clone();
    crate::error::run_blocking(move || {
        prompt_service::list_prompts(&pool, &filter).map_err(ApiError::from)
    })
    .await
    .map(Json)
}

/// GET /api/v1/prompts/{prompt_id}
///
/// Retrieves a single prompt with its associated tags, categories, and
/// collections.
///
/// # Errors
///
/// Returns HTTP 404 if the prompt does not exist.
/// Returns HTTP 403 if the prompt is not owned by the authenticated user.
/// Returns HTTP 500 if the database query fails.
pub async fn get_prompt(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(prompt_id): Path<i64>,
) -> Result<Json<PromptWithAssociations>, ApiError> {
    let uid = auth.user_id;
    let pool = state.pool.clone();
    crate::error::run_blocking(move || {
        let pwa = prompt_service::get_prompt(&pool, prompt_id).map_err(ApiError::from)?;
        crate::middleware::auth::check_ownership(uid, pwa.prompt.user_id)?;
        Ok(pwa)
    })
    .await
    .map(Json)
}

/// POST /api/v1/prompts
///
/// Creates a prompt for the authenticated user. The `user_id` field in the
/// payload is overwritten with the session user ID.
///
/// # Errors
///
/// Returns HTTP 400 if the request body fails validation.
/// Returns HTTP 500 if the database insert fails.
pub async fn create_prompt(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    crate::error::ValidatedJson(mut payload): crate::error::ValidatedJson<NewPrompt>,
) -> Result<(axum::http::StatusCode, Json<Prompt>), ApiError> {
    payload.user_id = auth.user_id;
    let pool = state.pool.clone();
    let prompt = crate::error::run_blocking(move || {
        prompt_service::create_prompt(&pool, &payload).map_err(ApiError::from)
    })
    .await?;
    Ok((axum::http::StatusCode::CREATED, Json(prompt)))
}

/// PUT /api/v1/prompts/{prompt_id}
///
/// Replaces an existing prompt. The path `prompt_id` must match the
/// `prompt_id` in the JSON body.
///
/// # Errors
///
/// Returns HTTP 400 if the path ID does not match the payload ID.
/// Returns HTTP 404 if the prompt does not exist.
/// Returns HTTP 403 if the prompt is not owned by the authenticated user.
/// Returns HTTP 500 if the database update fails.
pub async fn update_prompt(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(prompt_id): Path<i64>,
    Json(payload): Json<UpdatePrompt>,
) -> Result<Json<Prompt>, ApiError> {
    // Validate path param matches payload ID.
    if payload.prompt_id != prompt_id {
        return Err(ApiError::new(
            axum::http::StatusCode::BAD_REQUEST,
            "ID_MISMATCH",
            format!(
                "path prompt_id ({prompt_id}) does not match payload prompt_id ({})",
                payload.prompt_id
            ),
        ));
    }
    let uid = auth.user_id;
    let pool = state.pool.clone();
    crate::error::run_blocking(move || {
        // Check ownership of existing resource before updating.
        let existing = prompt_service::get_prompt(&pool, prompt_id).map_err(ApiError::from)?;
        crate::middleware::auth::check_ownership(uid, existing.prompt.user_id)?;
        prompt_service::update_prompt(&pool, &payload).map_err(ApiError::from)
    })
    .await
    .map(Json)
}

/// DELETE /api/v1/prompts/{prompt_id}
///
/// Deletes a prompt owned by the authenticated user.
///
/// # Errors
///
/// Returns HTTP 404 if the prompt does not exist.
/// Returns HTTP 403 if the prompt is not owned by the authenticated user.
/// Returns HTTP 500 if the database deletion fails.
pub async fn delete_prompt(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(prompt_id): Path<i64>,
) -> Result<axum::http::StatusCode, ApiError> {
    let uid = auth.user_id;
    let pool = state.pool.clone();
    crate::error::run_blocking(move || {
        let existing = prompt_service::get_prompt(&pool, prompt_id).map_err(ApiError::from)?;
        crate::middleware::auth::check_ownership(uid, existing.prompt.user_id)?;
        prompt_service::delete_prompt(&pool, prompt_id).map_err(ApiError::from)
    })
    .await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

/// POST /api/v1/prompts/{prompt_id}/duplicate
///
/// Creates a copy of an existing prompt, including its associations.
///
/// # Errors
///
/// Returns HTTP 404 if the prompt does not exist.
/// Returns HTTP 403 if the prompt is not owned by the authenticated user.
/// Returns HTTP 500 if the database operation fails.
pub async fn duplicate_prompt(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(prompt_id): Path<i64>,
) -> Result<Json<Prompt>, ApiError> {
    let uid = auth.user_id;
    let pool = state.pool.clone();
    crate::error::run_blocking(move || {
        let existing = prompt_service::get_prompt(&pool, prompt_id).map_err(ApiError::from)?;
        crate::middleware::auth::check_ownership(uid, existing.prompt.user_id)?;
        prompt_service::duplicate_prompt(&pool, prompt_id).map_err(ApiError::from)
    })
    .await
    .map(Json)
}

/// PATCH /api/v1/prompts/{prompt_id}/favorite
///
/// Toggles or explicitly sets the favorite flag on a prompt. When the request
/// body contains a `value` field, that value is used; otherwise the current
/// flag is inverted.
///
/// # Errors
///
/// Returns HTTP 404 if the prompt does not exist.
/// Returns HTTP 403 if the prompt is not owned by the authenticated user.
/// Returns HTTP 500 if the database update fails.
pub async fn toggle_favorite(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(prompt_id): Path<i64>,
    body: Option<Json<TogglePayload>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let payload = body.map(|j| j.0).unwrap_or_default();
    let uid = auth.user_id;
    let pool = state.pool.clone();
    crate::error::run_blocking(move || {
        let existing = prompt_service::get_prompt(&pool, prompt_id).map_err(ApiError::from)?;
        crate::middleware::auth::check_ownership(uid, existing.prompt.user_id)?;
        let new_value = payload.value.unwrap_or(!existing.prompt.is_favorite);
        prompt_service::toggle_favorite(&pool, prompt_id, new_value).map_err(ApiError::from)?;
        Ok(serde_json::json!({ "is_favorite": new_value }))
    })
    .await
    .map(Json)
}

/// PATCH /api/v1/prompts/{prompt_id}/archive
///
/// Toggles or explicitly sets the archived flag on a prompt. When the request
/// body contains a `value` field, that value is used; otherwise the current
/// flag is inverted.
///
/// # Errors
///
/// Returns HTTP 404 if the prompt does not exist.
/// Returns HTTP 403 if the prompt is not owned by the authenticated user.
/// Returns HTTP 500 if the database update fails.
pub async fn toggle_archive(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(prompt_id): Path<i64>,
    body: Option<Json<TogglePayload>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let payload = body.map(|j| j.0).unwrap_or_default();
    let uid = auth.user_id;
    let pool = state.pool.clone();
    crate::error::run_blocking(move || {
        let existing = prompt_service::get_prompt(&pool, prompt_id).map_err(ApiError::from)?;
        crate::middleware::auth::check_ownership(uid, existing.prompt.user_id)?;
        let new_value = payload.value.unwrap_or(!existing.prompt.is_archived);
        prompt_service::toggle_archive(&pool, prompt_id, new_value).map_err(ApiError::from)?;
        Ok(serde_json::json!({ "is_archived": new_value }))
    })
    .await
    .map(Json)
}

/// GET /api/v1/prompts/languages
///
/// Returns the distinct set of languages used across all prompts owned by the
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
        prompt_service::list_prompt_languages(&pool, uid).map_err(ApiError::from)
    })
    .await
    .map(Json)
}

/// POST /api/v1/prompts/bulk-update
///
/// Applies bulk changes (favorite, archived, tag/category/collection
/// additions and removals) to multiple prompts at once.
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
        prompt_service::bulk_update(&pool, active_uid, &input).map_err(ApiError::from)
    })
    .await
    .map(|count| Json(serde_json::json!({"updated": count})))
}
