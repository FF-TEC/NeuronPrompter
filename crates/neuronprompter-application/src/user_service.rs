// =============================================================================
// User service: creation with default settings, deletion, switching.
//
// Coordinates user lifecycle operations that span multiple repositories.
// Creating a user also inserts a default `user_settings` row. Switching users
// updates the `last_user_id` app setting. Deleting a user cascades through
// all owned data via foreign key rules.
// =============================================================================

use neuronprompter_core::domain::user::{NewUser, User};
use neuronprompter_core::validation;
use neuronprompter_db::ConnectionProvider;
use neuronprompter_db::repo::{settings, users};

use crate::ServiceError;

/// Creates a user with validated username and inserts a default `user_settings`
/// row (theme=dark, `ollama_base_url`=<http://localhost:11434>,
/// `sort_field`=`updated_at`, `sort_direction`=desc).
///
/// # Errors
///
/// Returns `ServiceError::Core(Validation)` if the username or display name
/// fails validation.
/// Returns `ServiceError::Database` if the persistence layer fails (including
/// duplicate username).
pub fn create_user(cp: &impl ConnectionProvider, new_user: &NewUser) -> Result<User, ServiceError> {
    validation::validate_username(&new_user.username)?;
    validation::validate_display_name(&new_user.display_name)?;

    Ok(cp.with_transaction(|conn| {
        let user = users::create_user(conn, new_user)?;
        settings::create_default_user_settings(conn, user.id)?;
        Ok(user)
    })?)
}

/// Returns a user by ID.
///
/// # Errors
///
/// Returns `ServiceError::Database` if no user with the given ID exists or the
/// persistence layer fails.
pub fn get_user(cp: &impl ConnectionProvider, user_id: i64) -> Result<User, ServiceError> {
    Ok(cp.with_connection(|conn| users::get_user(conn, user_id))?)
}

/// Returns all users ordered by username.
///
/// # Errors
///
/// Returns `ServiceError::Database` if the persistence layer fails.
pub fn list_users(cp: &impl ConnectionProvider) -> Result<Vec<User>, ServiceError> {
    Ok(cp.with_connection(users::list_users)?)
}

/// Switches the active user by storing the user ID in the `app_settings`
/// key-value store under the `last_user_id` key.
///
/// # Errors
///
/// Returns `ServiceError::Database` if no user with the given ID exists or the
/// persistence layer fails.
pub fn switch_user(cp: &impl ConnectionProvider, user_id: i64) -> Result<(), ServiceError> {
    Ok(cp.with_connection(|conn| {
        // Verify the user exists before switching.
        users::get_user(conn, user_id)?;
        settings::set_app_setting(conn, "last_user_id", &user_id.to_string())?;
        Ok(())
    })?)
}

/// Updates a user's display_name and username.
/// Runs within a transaction because the uniqueness check and the update
/// must be atomic to prevent TOCTOU races on the username constraint.
///
/// # Errors
///
/// Returns `ServiceError::Core(Validation)` if the username or display name
/// fails validation.
/// Returns `ServiceError::Core(Duplicate)` if the username is already taken by
/// another user.
/// Returns `ServiceError::Database` if no user with the given ID exists or the
/// persistence layer fails.
pub fn update_user(
    cp: &impl ConnectionProvider,
    user_id: i64,
    display_name: &str,
    username: &str,
) -> Result<User, ServiceError> {
    validation::validate_username(username)?;
    validation::validate_display_name(display_name)?;
    Ok(cp.with_transaction(|conn| {
        // Check if username is taken by another user.
        if let Some(existing) = users::find_user_by_username(conn, username)? {
            if existing.id != user_id {
                return Err(neuronprompter_db::DbError::Core(
                    neuronprompter_core::CoreError::Duplicate {
                        entity: "User".to_owned(),
                        field: "username".to_owned(),
                        value: username.to_owned(),
                    },
                ));
            }
        }
        users::update_user(conn, user_id, display_name, username)
    })?)
}

/// Deletes a user and all associated data (prompts, tags, categories,
/// collections, settings) via foreign key CASCADE rules.
/// Also clears the `last_user_id` app setting if it references this user.
/// Runs within a transaction so that the setting clear and user deletion
/// are atomic.
///
/// # Errors
///
/// Returns `ServiceError::Database` if no user with the given ID exists or the
/// persistence layer fails.
pub fn delete_user(cp: &impl ConnectionProvider, user_id: i64) -> Result<(), ServiceError> {
    Ok(cp.with_transaction(|conn| {
        // Clear last_user_id if it references the user being deleted.
        if let Ok(Some(setting)) = settings::get_app_setting(conn, "last_user_id") {
            if setting.value.parse::<i64>().ok() == Some(user_id) {
                settings::set_app_setting(conn, "last_user_id", "")?;
            }
        }
        users::delete_user(conn, user_id)
    })?)
}
