// =============================================================================
// Settings service: thin wrappers around DB settings repository calls.
//
// Provides service-layer access to application and per-user settings,
// keeping the API handlers decoupled from the database layer.
// =============================================================================

use neuronprompter_core::domain::settings::{AppSetting, UserSettings};
use neuronprompter_core::validation;
use neuronprompter_db::ConnectionProvider;
use neuronprompter_db::repo::settings;

use crate::ServiceError;

/// Retrieves a global application setting by key. Returns `None` if the key
/// does not exist.
///
/// # Errors
///
/// Returns `ServiceError::Database` if the persistence layer fails.
pub fn get_app_setting(
    cp: &impl ConnectionProvider,
    key: &str,
) -> Result<Option<AppSetting>, ServiceError> {
    Ok(cp.with_connection(|conn| settings::get_app_setting(conn, key))?)
}

/// Inserts or updates a global application setting.
///
/// # Errors
///
/// Returns `ServiceError::Database` if the persistence layer fails.
pub fn set_app_setting(
    cp: &impl ConnectionProvider,
    key: &str,
    value: &str,
) -> Result<(), ServiceError> {
    Ok(cp.with_connection(|conn| settings::set_app_setting(conn, key, value))?)
}

/// Retrieves per-user settings by user id.
///
/// # Errors
///
/// Returns `ServiceError::Database` if the persistence layer fails.
pub fn get_user_settings(
    cp: &impl ConnectionProvider,
    user_id: i64,
) -> Result<UserSettings, ServiceError> {
    Ok(cp.with_connection(|conn| settings::get_user_settings(conn, user_id))?)
}

/// Inserts or updates per-user settings.
/// Validates the `ollama_base_url` to prevent SSRF before persisting.
/// Validates the `extra` field is valid JSON object under 64 KB.
///
/// # Errors
///
/// Returns `ServiceError::Core(Validation)` if `ollama_base_url` fails URL validation.
/// Returns `ServiceError::Core(Validation)` if the `extra` field exceeds 64 KB,
/// is not valid JSON, or is not a JSON object.
/// Returns `ServiceError::Database` if the persistence layer fails.
pub fn upsert_user_settings(
    cp: &impl ConnectionProvider,
    user_settings: &UserSettings,
) -> Result<(), ServiceError> {
    validation::validate_ollama_url(&user_settings.ollama_base_url)?;

    // Validate extra field if non-empty and not the default "{}".
    if !user_settings.extra.is_empty() && user_settings.extra != "{}" {
        const MAX_EXTRA_BYTES: usize = 64 * 1024;
        if user_settings.extra.len() > MAX_EXTRA_BYTES {
            return Err(ServiceError::Core(
                neuronprompter_core::CoreError::Validation {
                    field: "extra".to_owned(),
                    message: format!("extra field exceeds maximum size of {MAX_EXTRA_BYTES} bytes"),
                },
            ));
        }
        let parsed: serde_json::Value =
            serde_json::from_str(&user_settings.extra).map_err(|e| {
                ServiceError::Core(neuronprompter_core::CoreError::Validation {
                    field: "extra".to_owned(),
                    message: format!("extra field must be valid JSON: {e}"),
                })
            })?;
        if !parsed.is_object() {
            return Err(ServiceError::Core(
                neuronprompter_core::CoreError::Validation {
                    field: "extra".to_owned(),
                    message: "extra field must be a JSON object".to_owned(),
                },
            ));
        }
    }

    Ok(cp.with_connection(|conn| settings::upsert_user_settings(conn, user_settings))?)
}

/// Creates default settings for a new user.
///
/// # Errors
///
/// Returns `ServiceError::Database` if the persistence layer fails.
pub fn create_default_user_settings(
    cp: &impl ConnectionProvider,
    user_id: i64,
) -> Result<(), ServiceError> {
    Ok(cp.with_connection(|conn| settings::create_default_user_settings(conn, user_id))?)
}
