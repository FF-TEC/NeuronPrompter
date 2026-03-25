use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use neuronprompter_application::search_service;
use neuronprompter_core::domain::prompt::{Prompt, PromptFilter};
use neuronprompter_core::domain::script::{Script, ScriptFilter};

use crate::error::ApiError;
use crate::middleware::session::AuthUser;
use crate::state::AppState;

#[derive(serde::Deserialize)]
pub struct SearchPayload {
    pub user_id: i64,
    pub query: String,
    #[serde(default)]
    pub filter: PromptFilter,
    /// Top-level limit accepted for convenience (overrides filter.limit).
    pub limit: Option<i64>,
    /// Top-level offset accepted for convenience (overrides filter.offset).
    pub offset: Option<i64>,
    /// Top-level filter aliases for convenience (override filter fields).
    #[serde(alias = "favorite")]
    pub is_favorite: Option<bool>,
    #[serde(alias = "archived")]
    pub is_archived: Option<bool>,
    pub tag_id: Option<i64>,
    pub category_id: Option<i64>,
    pub collection_id: Option<i64>,
}

/// Applies top-level convenience overrides from `SearchPayload` into the
/// nested `PromptFilter`.
fn apply_prompt_search_overrides(payload: &mut SearchPayload) {
    if let Some(limit) = payload.limit {
        payload.filter.limit = Some(limit.clamp(1, 1000));
    }
    if let Some(offset) = payload.offset {
        payload.filter.offset = Some(offset);
    }
    if let Some(fav) = payload.is_favorite {
        payload.filter.is_favorite = Some(fav);
    }
    if let Some(arch) = payload.is_archived {
        payload.filter.is_archived = Some(arch);
    }
    if let Some(tid) = payload.tag_id {
        payload.filter.tag_id = Some(tid);
    }
    if let Some(cid) = payload.category_id {
        payload.filter.category_id = Some(cid);
    }
    if let Some(cid) = payload.collection_id {
        payload.filter.collection_id = Some(cid);
    }
}

/// POST /api/v1/search/prompts
///
/// Performs a full-text search over the authenticated user's prompts. Accepts
/// optional filter, pagination, and taxonomy parameters.
///
/// # Errors
///
/// Returns HTTP 403 if the authenticated user does not match the payload `user_id`.
/// Returns HTTP 500 if an internal database error occurs.
pub async fn search_prompts(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(mut payload): Json<SearchPayload>,
) -> Result<Json<Vec<Prompt>>, ApiError> {
    crate::middleware::auth::verify_user_id_param(auth.user_id, payload.user_id)?;
    apply_prompt_search_overrides(&mut payload);
    let effective_user_id = auth.user_id;
    let pool = state.pool.clone();
    crate::error::run_blocking(move || {
        search_service::search_prompts(&pool, effective_user_id, &payload.query, &payload.filter)
            .map_err(ApiError::from)
    })
    .await
    .map(Json)
}

#[derive(serde::Deserialize)]
pub struct ScriptSearchPayload {
    pub user_id: i64,
    pub query: String,
    #[serde(default)]
    pub filter: ScriptFilter,
    /// Top-level limit accepted for convenience (overrides filter.limit).
    pub limit: Option<i64>,
    /// Top-level offset accepted for convenience (overrides filter.offset).
    pub offset: Option<i64>,
    /// Top-level filter aliases for convenience (override filter fields).
    #[serde(alias = "favorite")]
    pub is_favorite: Option<bool>,
    #[serde(alias = "archived")]
    pub is_archived: Option<bool>,
    pub tag_id: Option<i64>,
    pub category_id: Option<i64>,
    pub collection_id: Option<i64>,
}

/// POST /api/v1/search/scripts
///
/// Performs a full-text search over the authenticated user's scripts. Accepts
/// optional filter, pagination, and taxonomy parameters.
///
/// # Errors
///
/// Returns HTTP 403 if the authenticated user does not match the payload `user_id`.
/// Returns HTTP 500 if an internal database error occurs.
pub async fn search_scripts(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(mut payload): Json<ScriptSearchPayload>,
) -> Result<Json<Vec<Script>>, ApiError> {
    crate::middleware::auth::verify_user_id_param(auth.user_id, payload.user_id)?;
    if let Some(limit) = payload.limit {
        payload.filter.limit = Some(limit.clamp(1, 1000));
    }
    if let Some(offset) = payload.offset {
        payload.filter.offset = Some(offset);
    }
    if let Some(fav) = payload.is_favorite {
        payload.filter.is_favorite = Some(fav);
    }
    if let Some(arch) = payload.is_archived {
        payload.filter.is_archived = Some(arch);
    }
    if let Some(tid) = payload.tag_id {
        payload.filter.tag_id = Some(tid);
    }
    if let Some(cid) = payload.category_id {
        payload.filter.category_id = Some(cid);
    }
    if let Some(cid) = payload.collection_id {
        payload.filter.collection_id = Some(cid);
    }
    let effective_user_id = auth.user_id;
    let pool = state.pool.clone();
    crate::error::run_blocking(move || {
        search_service::search_scripts(&pool, effective_user_id, &payload.query, &payload.filter)
            .map_err(ApiError::from)
    })
    .await
    .map(Json)
}
