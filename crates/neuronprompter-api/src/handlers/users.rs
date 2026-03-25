use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use neuronprompter_application::user_service;
use neuronprompter_core::domain::user::{NewUser, User};

use crate::error::ApiError;
use crate::middleware::session::{AuthContext, AuthSession, AuthUser};
use crate::state::AppState;

/// Minimal user info returned by the public user list endpoint.
/// Omits timestamps to reduce information exposure on unauthenticated routes.
#[derive(serde::Serialize)]
pub struct PublicUser {
    /// User row ID.
    pub id: i64,
    /// Unique login name.
    pub username: String,
    /// Display name shown in the UI.
    pub display_name: String,
}

/// GET /api/v1/users
///
/// Lists all users, returning only public fields (id, username, display_name).
/// This endpoint does not require user-level authentication and is used by
/// the login/session selection page.
///
/// # Errors
///
/// Returns HTTP 500 if an internal database error occurs.
pub async fn list_users(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<PublicUser>>, ApiError> {
    let pool = state.pool.clone();
    crate::error::run_blocking(move || {
        user_service::list_users(&pool)
            .map(|users| {
                users
                    .into_iter()
                    .map(|u| PublicUser {
                        id: u.id,
                        username: u.username,
                        display_name: u.display_name,
                    })
                    .collect()
            })
            .map_err(ApiError::from)
    })
    .await
    .map(Json)
}

/// POST /api/v1/users
///
/// Creates a user account. When at least one user exists, a valid session is
/// required. During first-run (zero users in the database), authentication is
/// skipped so the welcome flow can bootstrap the first user.
///
/// # Errors
///
/// Returns HTTP 401 if no session is present and the database already contains
/// at least one user.
/// Returns HTTP 400 if the request body fails validation.
/// Returns HTTP 500 if an internal database error occurs.
pub async fn create_user(
    State(state): State<Arc<AppState>>,
    auth_ctx: Option<axum::Extension<AuthContext>>,
    Json(payload): Json<NewUser>,
) -> Result<(axum::http::StatusCode, Json<User>), ApiError> {
    // When no session is present, only allow user creation if the database
    // contains zero users (first-run bootstrap).
    if auth_ctx.is_none() {
        let check_pool = state.pool.clone();
        let has_users = crate::error::run_blocking(move || {
            user_service::list_users(&check_pool)
                .map(|users| !users.is_empty())
                .map_err(ApiError::from)
        })
        .await?;

        if has_users {
            return Err(ApiError::new(
                axum::http::StatusCode::UNAUTHORIZED,
                "AUTH_REQUIRED",
                "a valid session is required".to_owned(),
            ));
        }
    }

    let pool = state.pool.clone();
    let user = crate::error::run_blocking(move || {
        user_service::create_user(&pool, &payload).map_err(ApiError::from)
    })
    .await?;
    Ok((axum::http::StatusCode::CREATED, Json(user)))
}

/// PUT /api/v1/users/{user_id}/switch
///
/// Switches the active user within the current session. The target user must
/// exist in the database.
///
/// # Errors
///
/// Returns HTTP 404 if the target user does not exist.
/// Returns HTTP 500 if an internal database error occurs.
pub async fn switch_user(
    State(state): State<Arc<AppState>>,
    auth: AuthSession,
    Path(user_id): Path<i64>,
) -> Result<Json<()>, ApiError> {
    // Verify user exists.
    let pool = state.pool.clone();
    crate::error::run_blocking(move || {
        user_service::switch_user(&pool, user_id).map_err(ApiError::from)
    })
    .await?;

    // Update session user.
    state
        .sessions
        .set_session_user(&auth.session_token, user_id);
    Ok(Json(()))
}

/// Request body for updating a user.
#[derive(serde::Deserialize)]
pub struct UpdateUserPayload {
    pub display_name: String,
    pub username: String,
}

/// PUT /api/v1/users/{user_id}
///
/// Updates the display name and username for the authenticated user.
///
/// # Errors
///
/// Returns HTTP 403 if the authenticated user does not match the path `user_id`.
/// Returns HTTP 404 if the user does not exist.
/// Returns HTTP 500 if an internal database error occurs.
pub async fn update_user(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(user_id): Path<i64>,
    Json(payload): Json<UpdateUserPayload>,
) -> Result<Json<User>, ApiError> {
    // Only the active user may update their own profile.
    crate::middleware::auth::verify_user_id_param(auth.user_id, user_id)?;
    tracing::info!(user_id, username = %payload.username, "updating user");
    let pool = state.pool.clone();
    crate::error::run_blocking(move || {
        user_service::update_user(&pool, user_id, &payload.display_name, &payload.username)
            .map_err(ApiError::from)
    })
    .await
    .map(Json)
}

/// DELETE /api/v1/users/{user_id}
///
/// Deletes the authenticated user's account and invalidates all associated
/// sessions.
///
/// # Errors
///
/// Returns HTTP 403 if the authenticated user does not match the path `user_id`.
/// Returns HTTP 404 if the user does not exist.
/// Returns HTTP 500 if an internal database error occurs.
pub async fn delete_user(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(user_id): Path<i64>,
) -> Result<axum::http::StatusCode, ApiError> {
    // Only the active user may delete their own account.
    crate::middleware::auth::verify_user_id_param(auth.user_id, user_id)?;
    let pool = state.pool.clone();
    crate::error::run_blocking(move || {
        user_service::delete_user(&pool, user_id).map_err(ApiError::from)
    })
    .await?;

    // Invalidate all sessions for the deleted user.
    state.sessions.remove_sessions_for_user(user_id);
    Ok(axum::http::StatusCode::NO_CONTENT)
}
