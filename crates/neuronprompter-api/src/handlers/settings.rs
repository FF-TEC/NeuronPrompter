use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use neuronprompter_application::settings_service;
use neuronprompter_core::domain::settings::{AppSetting, UserSettings};
use neuronprompter_db::ConnectionProvider;

use crate::error::ApiError;
use crate::middleware::session::AuthUser;
use crate::state::AppState;

/// GET /api/v1/settings/app/{key}
///
/// Retrieves a single application-level setting by key. Returns `null` in the
/// JSON body if the key does not exist.
///
/// # Errors
///
/// Returns HTTP 500 if an internal database error occurs.
pub async fn get_app_setting(
    State(state): State<Arc<AppState>>,
    _auth: AuthUser,
    Path(key): Path<String>,
) -> Result<Json<Option<AppSetting>>, ApiError> {
    let pool = state.pool.clone();
    crate::error::run_blocking(move || {
        settings_service::get_app_setting(&pool, &key).map_err(ApiError::from)
    })
    .await
    .map(Json)
}

#[derive(serde::Deserialize)]
pub struct SetAppSettingPayload {
    pub value: String,
}

/// Keys that may be written via the public app-settings endpoint.
const WRITABLE_APP_SETTING_KEYS: &[&str] = &["last_user_id"];

/// PUT /api/v1/settings/app/{key}
///
/// Sets an application-level setting. Only keys in the writable allowlist
/// (currently `last_user_id`) are accepted. For `last_user_id`, the value
/// must be a valid integer referencing an existing user.
///
/// # Errors
///
/// Returns HTTP 403 if the key is not in the writable allowlist.
/// Returns HTTP 400 if the value fails validation (e.g. non-integer
/// `last_user_id` or reference to a nonexistent user).
/// Returns HTTP 500 if an internal database error occurs.
pub async fn set_app_setting(
    State(state): State<Arc<AppState>>,
    _auth: AuthUser,
    Path(key): Path<String>,
    Json(payload): Json<SetAppSettingPayload>,
) -> Result<Json<()>, ApiError> {
    // Only allow writing to a known allowlist of keys.
    if !WRITABLE_APP_SETTING_KEYS.contains(&key.as_str()) {
        return Err(ApiError::new(
            axum::http::StatusCode::FORBIDDEN,
            "FORBIDDEN",
            format!("app setting key '{key}' is not writable"),
        ));
    }

    // Validate last_user_id: must be a valid i64 referencing an existing user.
    if key == "last_user_id" {
        let uid: i64 = payload.value.parse().map_err(|_| {
            ApiError::new(
                axum::http::StatusCode::BAD_REQUEST,
                "VALIDATION_ERROR",
                "last_user_id must be a valid integer".to_owned(),
            )
        })?;
        let pool_check = state.pool.clone();
        crate::error::run_blocking(move || {
            pool_check
                .with_connection(|conn| neuronprompter_db::repo::users::get_user(conn, uid))
                .map_err(|_| {
                    ApiError::new(
                        axum::http::StatusCode::BAD_REQUEST,
                        "VALIDATION_ERROR",
                        format!("user with id {uid} does not exist"),
                    )
                })
        })
        .await?;
    }

    let pool = state.pool.clone();
    crate::error::run_blocking(move || {
        settings_service::set_app_setting(&pool, &key, &payload.value).map_err(ApiError::from)
    })
    .await
    .map(Json)
}

/// GET /api/v1/settings/user/{user_id}
///
/// Retrieves all settings for the specified user.
///
/// # Errors
///
/// Returns HTTP 403 if the authenticated user does not match the path `user_id`.
/// Returns HTTP 500 if an internal database error occurs.
pub async fn get_user_settings(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(user_id): Path<i64>,
) -> Result<Json<UserSettings>, ApiError> {
    crate::middleware::auth::verify_user_id_param(auth.user_id, user_id)?;
    let pool = state.pool.clone();
    crate::error::run_blocking(move || {
        settings_service::get_user_settings(&pool, user_id).map_err(ApiError::from)
    })
    .await
    .map(Json)
}

/// PUT /api/v1/settings/user
///
/// Replaces all settings for the authenticated user with the provided values.
///
/// # Errors
///
/// Returns HTTP 403 if the authenticated user does not match the payload `user_id`.
/// Returns HTTP 500 if an internal database error occurs.
pub async fn update_user_settings(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(payload): Json<UserSettings>,
) -> Result<Json<()>, ApiError> {
    crate::middleware::auth::verify_user_id_param(auth.user_id, payload.user_id)?;
    let pool = state.pool.clone();
    crate::error::run_blocking(move || {
        settings_service::upsert_user_settings(&pool, &payload).map_err(ApiError::from)
    })
    .await
    .map(Json)
}

#[derive(serde::Deserialize)]
pub struct PatchUserSettingsPayload {
    pub user_id: i64,
    pub theme: Option<neuronprompter_core::domain::settings::Theme>,
    #[serde(
        default,
        deserialize_with = "neuronprompter_core::serde_helpers::deserialize_optional_field"
    )]
    pub last_collection_id: Option<Option<i64>>,
    pub sidebar_collapsed: Option<bool>,
    pub sort_field: Option<neuronprompter_core::domain::settings::SortField>,
    pub sort_direction: Option<neuronprompter_core::domain::settings::SortDirection>,
    pub ollama_base_url: Option<String>,
    #[serde(
        default,
        deserialize_with = "neuronprompter_core::serde_helpers::deserialize_optional_field"
    )]
    pub ollama_model: Option<Option<String>>,
    pub extra: Option<String>,
}

/// PATCH /api/v1/settings/user
///
/// Partially updates settings for the authenticated user. Only fields present
/// in the request body are applied; omitted fields retain their current values.
///
/// # Errors
///
/// Returns HTTP 403 if the authenticated user does not match the payload `user_id`.
/// Returns HTTP 400 if the `extra` field exceeds 64 KB, is not valid JSON, or
/// is not a JSON object.
/// Returns HTTP 500 if an internal database error occurs.
pub async fn patch_user_settings(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(payload): Json<PatchUserSettingsPayload>,
) -> Result<Json<()>, ApiError> {
    crate::middleware::auth::verify_user_id_param(auth.user_id, payload.user_id)?;
    let pool = state.pool.clone();
    crate::error::run_blocking(move || {
        // Load existing settings.
        let mut current =
            settings_service::get_user_settings(&pool, payload.user_id).map_err(ApiError::from)?;
        // Apply only the fields that are present in the patch.
        if let Some(theme) = payload.theme {
            current.theme = theme;
        }
        if let Some(last_col) = payload.last_collection_id {
            current.last_collection_id = last_col;
        }
        if let Some(collapsed) = payload.sidebar_collapsed {
            current.sidebar_collapsed = collapsed;
        }
        if let Some(sf) = payload.sort_field {
            current.sort_field = sf;
        }
        if let Some(sd) = payload.sort_direction {
            current.sort_direction = sd;
        }
        if let Some(url) = payload.ollama_base_url {
            current.ollama_base_url = url;
        }
        if let Some(model) = payload.ollama_model {
            current.ollama_model = model;
        }
        if let Some(ref extra) = payload.extra {
            // Validate extra: must be valid JSON object, max 64 KB.
            const MAX_EXTRA_BYTES: usize = 64 * 1024;
            if extra.len() > MAX_EXTRA_BYTES {
                return Err(ApiError::new(
                    axum::http::StatusCode::BAD_REQUEST,
                    "VALIDATION_ERROR",
                    format!("extra field exceeds maximum size of {MAX_EXTRA_BYTES} bytes"),
                ));
            }
            let parsed: serde_json::Value = serde_json::from_str(extra).map_err(|e| {
                ApiError::new(
                    axum::http::StatusCode::BAD_REQUEST,
                    "VALIDATION_ERROR",
                    format!("extra field must be valid JSON: {e}"),
                )
            })?;
            if !parsed.is_object() {
                return Err(ApiError::new(
                    axum::http::StatusCode::BAD_REQUEST,
                    "VALIDATION_ERROR",
                    "extra field must be a JSON object".to_owned(),
                ));
            }
            extra.clone_into(&mut current.extra);
        }
        settings_service::upsert_user_settings(&pool, &current).map_err(ApiError::from)
    })
    .await
    .map(Json)
}
