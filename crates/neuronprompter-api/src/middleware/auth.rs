use axum::http::StatusCode;

use crate::error::ApiError;

/// Ownership check for use inside `run_blocking` closures.
/// Returns 404 (not 403) on mismatch to avoid leaking resource existence.
///
/// # Errors
///
/// Returns `ApiError` with status 404 (NOT_FOUND) if `session_user_id` does
/// not match `resource_user_id`.
pub fn check_ownership(session_user_id: i64, resource_user_id: i64) -> Result<(), ApiError> {
    if session_user_id == resource_user_id {
        Ok(())
    } else {
        Err(ApiError::new(
            StatusCode::NOT_FOUND,
            "NOT_FOUND",
            "resource not found".to_owned(),
        ))
    }
}

/// Verifies that a `user_id` from a path parameter matches the session user.
///
/// # Errors
///
/// Returns `ApiError` with status 403 (FORBIDDEN) if `session_user_id` does
/// not match `user_id`.
pub fn verify_user_id_param(session_user_id: i64, user_id: i64) -> Result<(), ApiError> {
    if session_user_id != user_id {
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            "FORBIDDEN",
            "cannot access resources of another user".to_owned(),
        ));
    }
    Ok(())
}
