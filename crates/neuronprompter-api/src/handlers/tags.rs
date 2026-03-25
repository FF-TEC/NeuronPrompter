use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use neuronprompter_application::tag_service;
use neuronprompter_core::domain::tag::Tag;
use neuronprompter_core::validation;
use neuronprompter_db::ConnectionProvider;

use crate::error::ApiError;
use crate::middleware::session::AuthUser;
use crate::state::AppState;

/// GET /api/v1/tags/user/{user_id}
///
/// Lists all tags belonging to the specified user.
///
/// # Errors
///
/// Returns HTTP 403 if the path `user_id` does not match the authenticated
/// user.
/// Returns HTTP 500 if the database query fails.
pub async fn list_tags(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(user_id): Path<i64>,
) -> Result<Json<Vec<Tag>>, ApiError> {
    crate::middleware::auth::verify_user_id_param(auth.user_id, user_id)?;
    let pool = state.pool.clone();
    crate::error::run_blocking(move || {
        tag_service::list_tags(&pool, user_id).map_err(ApiError::from)
    })
    .await
    .map(Json)
}

#[derive(serde::Deserialize)]
pub struct CreateTagPayload {
    pub user_id: i64,
    pub name: String,
}

/// POST /api/v1/tags
///
/// Creates a tag for the specified user.
///
/// # Errors
///
/// Returns HTTP 403 if the payload `user_id` does not match the authenticated
/// user.
/// Returns HTTP 400 if the tag name fails taxonomy validation.
/// Returns HTTP 500 if the database insert fails.
pub async fn create_tag(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(payload): Json<CreateTagPayload>,
) -> Result<(axum::http::StatusCode, Json<Tag>), ApiError> {
    crate::middleware::auth::verify_user_id_param(auth.user_id, payload.user_id)?;
    validation::validate_taxonomy_name(&payload.name).map_err(ApiError::from)?;
    let pool = state.pool.clone();
    let tag = crate::error::run_blocking(move || {
        tag_service::create_tag(&pool, payload.user_id, &payload.name).map_err(ApiError::from)
    })
    .await?;
    Ok((axum::http::StatusCode::CREATED, Json(tag)))
}

#[derive(serde::Deserialize)]
pub struct RenamePayload {
    pub new_name: String,
}

/// PUT /api/v1/tags/{tag_id}
///
/// Renames an existing tag.
///
/// # Errors
///
/// Returns HTTP 400 if the new name fails taxonomy validation.
/// Returns HTTP 404 if the tag does not exist.
/// Returns HTTP 403 if the tag is not owned by the authenticated user.
/// Returns HTTP 500 if the database update fails.
pub async fn rename_tag(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(tag_id): Path<i64>,
    Json(payload): Json<RenamePayload>,
) -> Result<Json<()>, ApiError> {
    validation::validate_taxonomy_name(&payload.new_name).map_err(ApiError::from)?;
    let uid = auth.user_id;
    let pool = state.pool.clone();
    crate::error::run_blocking(move || {
        // Load tag to verify ownership before renaming.
        let tag = pool
            .with_connection(|conn| neuronprompter_db::repo::tags::get_tag(conn, tag_id))
            .map_err(|e| ApiError::from(neuronprompter_application::ServiceError::from(e)))?;
        crate::middleware::auth::check_ownership(uid, tag.user_id)?;
        tag_service::rename_tag(&pool, tag_id, &payload.new_name).map_err(ApiError::from)
    })
    .await
    .map(Json)
}

/// DELETE /api/v1/tags/{tag_id}
///
/// Deletes a tag owned by the authenticated user.
///
/// # Errors
///
/// Returns HTTP 404 if the tag does not exist.
/// Returns HTTP 403 if the tag is not owned by the authenticated user.
/// Returns HTTP 500 if the database deletion fails.
pub async fn delete_tag(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(tag_id): Path<i64>,
) -> Result<axum::http::StatusCode, ApiError> {
    let uid = auth.user_id;
    let pool = state.pool.clone();
    crate::error::run_blocking(move || {
        let tag = pool
            .with_connection(|conn| neuronprompter_db::repo::tags::get_tag(conn, tag_id))
            .map_err(|e| ApiError::from(neuronprompter_application::ServiceError::from(e)))?;
        crate::middleware::auth::check_ownership(uid, tag.user_id)?;
        tag_service::delete_tag(&pool, tag_id).map_err(ApiError::from)
    })
    .await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}
