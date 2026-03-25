use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use neuronprompter_application::collection_service;
use neuronprompter_core::domain::collection::Collection;
use neuronprompter_core::validation;
use neuronprompter_db::ConnectionProvider;

use crate::error::ApiError;
use crate::middleware::session::AuthUser;
use crate::state::AppState;

/// GET /api/v1/collections/user/{user_id}
///
/// Lists all collections belonging to the specified user.
///
/// # Errors
///
/// Returns HTTP 403 if the path `user_id` does not match the authenticated
/// user.
/// Returns HTTP 500 if the database query fails.
pub async fn list_collections(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(user_id): Path<i64>,
) -> Result<Json<Vec<Collection>>, ApiError> {
    crate::middleware::auth::verify_user_id_param(auth.user_id, user_id)?;
    let pool = state.pool.clone();
    crate::error::run_blocking(move || {
        collection_service::list_collections(&pool, user_id).map_err(ApiError::from)
    })
    .await
    .map(Json)
}

#[derive(serde::Deserialize)]
pub struct CreateCollectionPayload {
    pub user_id: i64,
    pub name: String,
}

/// POST /api/v1/collections
///
/// Creates a collection for the specified user.
///
/// # Errors
///
/// Returns HTTP 403 if the payload `user_id` does not match the authenticated
/// user.
/// Returns HTTP 400 if the collection name fails taxonomy validation.
/// Returns HTTP 500 if the database insert fails.
pub async fn create_collection(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(payload): Json<CreateCollectionPayload>,
) -> Result<(axum::http::StatusCode, Json<Collection>), ApiError> {
    crate::middleware::auth::verify_user_id_param(auth.user_id, payload.user_id)?;
    validation::validate_taxonomy_name(&payload.name).map_err(ApiError::from)?;
    let pool = state.pool.clone();
    let collection = crate::error::run_blocking(move || {
        collection_service::create_collection(&pool, payload.user_id, &payload.name)
            .map_err(ApiError::from)
    })
    .await?;
    Ok((axum::http::StatusCode::CREATED, Json(collection)))
}

#[derive(serde::Deserialize)]
pub struct RenamePayload {
    pub new_name: String,
}

/// PUT /api/v1/collections/{collection_id}
///
/// Renames an existing collection.
///
/// # Errors
///
/// Returns HTTP 400 if the new name fails taxonomy validation.
/// Returns HTTP 404 if the collection does not exist.
/// Returns HTTP 403 if the collection is not owned by the authenticated user.
/// Returns HTTP 500 if the database update fails.
pub async fn rename_collection(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(collection_id): Path<i64>,
    Json(payload): Json<RenamePayload>,
) -> Result<Json<()>, ApiError> {
    validation::validate_taxonomy_name(&payload.new_name).map_err(ApiError::from)?;
    let uid = auth.user_id;
    let pool = state.pool.clone();
    crate::error::run_blocking(move || {
        let col = pool
            .with_connection(|conn| {
                neuronprompter_db::repo::collections::get_collection(conn, collection_id)
            })
            .map_err(|e| ApiError::from(neuronprompter_application::ServiceError::from(e)))?;
        crate::middleware::auth::check_ownership(uid, col.user_id)?;
        collection_service::rename_collection(&pool, collection_id, &payload.new_name)
            .map_err(ApiError::from)
    })
    .await
    .map(Json)
}

/// DELETE /api/v1/collections/{collection_id}
///
/// Deletes a collection owned by the authenticated user.
///
/// # Errors
///
/// Returns HTTP 404 if the collection does not exist.
/// Returns HTTP 403 if the collection is not owned by the authenticated user.
/// Returns HTTP 500 if the database deletion fails.
pub async fn delete_collection(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(collection_id): Path<i64>,
) -> Result<axum::http::StatusCode, ApiError> {
    let uid = auth.user_id;
    let pool = state.pool.clone();
    crate::error::run_blocking(move || {
        let col = pool
            .with_connection(|conn| {
                neuronprompter_db::repo::collections::get_collection(conn, collection_id)
            })
            .map_err(|e| ApiError::from(neuronprompter_application::ServiceError::from(e)))?;
        crate::middleware::auth::check_ownership(uid, col.user_id)?;
        collection_service::delete_collection(&pool, collection_id).map_err(ApiError::from)
    })
    .await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}
