use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use neuronprompter_application::category_service;
use neuronprompter_core::domain::category::Category;
use neuronprompter_core::validation;
use neuronprompter_db::ConnectionProvider;

use crate::error::ApiError;
use crate::middleware::session::AuthUser;
use crate::state::AppState;

/// GET /api/v1/categories/user/{user_id}
///
/// Lists all categories belonging to the specified user.
///
/// # Errors
///
/// Returns HTTP 403 if the path `user_id` does not match the authenticated
/// user.
/// Returns HTTP 500 if the database query fails.
pub async fn list_categories(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(user_id): Path<i64>,
) -> Result<Json<Vec<Category>>, ApiError> {
    crate::middleware::auth::verify_user_id_param(auth.user_id, user_id)?;
    let pool = state.pool.clone();
    crate::error::run_blocking(move || {
        category_service::list_categories(&pool, user_id).map_err(ApiError::from)
    })
    .await
    .map(Json)
}

#[derive(serde::Deserialize)]
pub struct CreateCategoryPayload {
    pub user_id: i64,
    pub name: String,
}

/// POST /api/v1/categories
///
/// Creates a category for the specified user.
///
/// # Errors
///
/// Returns HTTP 403 if the payload `user_id` does not match the authenticated
/// user.
/// Returns HTTP 400 if the category name fails taxonomy validation.
/// Returns HTTP 500 if the database insert fails.
pub async fn create_category(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(payload): Json<CreateCategoryPayload>,
) -> Result<(axum::http::StatusCode, Json<Category>), ApiError> {
    crate::middleware::auth::verify_user_id_param(auth.user_id, payload.user_id)?;
    validation::validate_taxonomy_name(&payload.name).map_err(ApiError::from)?;
    let pool = state.pool.clone();
    let category = crate::error::run_blocking(move || {
        category_service::create_category(&pool, payload.user_id, &payload.name)
            .map_err(ApiError::from)
    })
    .await?;
    Ok((axum::http::StatusCode::CREATED, Json(category)))
}

#[derive(serde::Deserialize)]
pub struct RenamePayload {
    pub new_name: String,
}

/// PUT /api/v1/categories/{category_id}
///
/// Renames an existing category.
///
/// # Errors
///
/// Returns HTTP 400 if the new name fails taxonomy validation.
/// Returns HTTP 404 if the category does not exist.
/// Returns HTTP 403 if the category is not owned by the authenticated user.
/// Returns HTTP 500 if the database update fails.
pub async fn rename_category(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(category_id): Path<i64>,
    Json(payload): Json<RenamePayload>,
) -> Result<Json<()>, ApiError> {
    validation::validate_taxonomy_name(&payload.new_name).map_err(ApiError::from)?;
    let uid = auth.user_id;
    let pool = state.pool.clone();
    crate::error::run_blocking(move || {
        let cat = pool
            .with_connection(|conn| {
                neuronprompter_db::repo::categories::get_category(conn, category_id)
            })
            .map_err(|e| ApiError::from(neuronprompter_application::ServiceError::from(e)))?;
        crate::middleware::auth::check_ownership(uid, cat.user_id)?;
        category_service::rename_category(&pool, category_id, &payload.new_name)
            .map_err(ApiError::from)
    })
    .await
    .map(Json)
}

/// DELETE /api/v1/categories/{category_id}
///
/// Deletes a category owned by the authenticated user.
///
/// # Errors
///
/// Returns HTTP 404 if the category does not exist.
/// Returns HTTP 403 if the category is not owned by the authenticated user.
/// Returns HTTP 500 if the database deletion fails.
pub async fn delete_category(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(category_id): Path<i64>,
) -> Result<axum::http::StatusCode, ApiError> {
    let uid = auth.user_id;
    let pool = state.pool.clone();
    crate::error::run_blocking(move || {
        let cat = pool
            .with_connection(|conn| {
                neuronprompter_db::repo::categories::get_category(conn, category_id)
            })
            .map_err(|e| ApiError::from(neuronprompter_application::ServiceError::from(e)))?;
        crate::middleware::auth::check_ownership(uid, cat.user_id)?;
        category_service::delete_category(&pool, category_id).map_err(ApiError::from)
    })
    .await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}
