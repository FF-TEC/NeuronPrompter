use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use neuronprompter_application::{chain_service, search_service};
use neuronprompter_core::domain::PaginatedList;
use neuronprompter_core::domain::chain::{
    Chain, ChainFilter, ChainWithSteps, NewChain, UpdateChain,
};
use serde::{Deserialize, Serialize};

use super::TogglePayload;
use crate::error::ApiError;
use crate::middleware::session::AuthUser;
use crate::state::AppState;

/// GET /api/v1/chains/count
///
/// Returns the total number of chains owned by the authenticated user.
///
/// # Errors
///
/// Returns HTTP 500 if the database query fails.
pub async fn count_chains(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> Result<Json<serde_json::Value>, ApiError> {
    let uid = auth.user_id;
    let pool = state.pool.clone();
    crate::error::run_blocking(move || {
        chain_service::count_chains(&pool, uid).map_err(ApiError::from)
    })
    .await
    .map(|count| Json(serde_json::json!({ "count": count })))
}

/// POST /api/v1/chains/search
///
/// Lists chains for the authenticated user, filtered and paginated according
/// to the provided `ChainFilter` body.
///
/// # Errors
///
/// Returns HTTP 500 if the database query fails.
pub async fn list_chains(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(mut filter): Json<ChainFilter>,
) -> Result<Json<PaginatedList<Chain>>, ApiError> {
    filter.user_id = Some(auth.user_id);
    let pool = state.pool.clone();
    crate::error::run_blocking(move || {
        chain_service::list_chains(&pool, &filter).map_err(ApiError::from)
    })
    .await
    .map(Json)
}

/// GET /api/v1/chains/{chain_id}
///
/// Retrieves a single chain with its ordered steps and resolved prompt/script
/// references.
///
/// # Errors
///
/// Returns HTTP 404 if the chain does not exist.
/// Returns HTTP 403 if the chain is not owned by the authenticated user.
/// Returns HTTP 500 if the database query fails.
pub async fn get_chain(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(chain_id): Path<i64>,
) -> Result<Json<ChainWithSteps>, ApiError> {
    let uid = auth.user_id;
    let pool = state.pool.clone();
    crate::error::run_blocking(move || {
        let cws = chain_service::get_chain(&pool, chain_id).map_err(ApiError::from)?;
        crate::middleware::auth::check_ownership(uid, cws.chain.user_id)?;
        Ok(cws)
    })
    .await
    .map(Json)
}

/// POST /api/v1/chains
///
/// Creates a chain for the authenticated user. The `user_id` field in the
/// payload is overwritten with the session user ID.
///
/// # Errors
///
/// Returns HTTP 400 if the request body fails validation.
/// Returns HTTP 500 if the database insert fails.
pub async fn create_chain(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    crate::error::ValidatedJson(mut payload): crate::error::ValidatedJson<NewChain>,
) -> Result<(axum::http::StatusCode, Json<Chain>), ApiError> {
    payload.user_id = auth.user_id;
    let pool = state.pool.clone();
    let chain = crate::error::run_blocking(move || {
        chain_service::create_chain(&pool, &payload).map_err(ApiError::from)
    })
    .await?;
    Ok((axum::http::StatusCode::CREATED, Json(chain)))
}

/// PUT /api/v1/chains/{chain_id}
///
/// Replaces an existing chain. The path `chain_id` must match the `chain_id`
/// in the JSON body.
///
/// # Errors
///
/// Returns HTTP 400 if the path ID does not match the payload ID.
/// Returns HTTP 404 if the chain does not exist.
/// Returns HTTP 403 if the chain is not owned by the authenticated user.
/// Returns HTTP 500 if the database update fails.
pub async fn update_chain(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(chain_id): Path<i64>,
    Json(payload): Json<UpdateChain>,
) -> Result<Json<Chain>, ApiError> {
    // Validate path param matches payload ID.
    if payload.chain_id != chain_id {
        return Err(ApiError::new(
            axum::http::StatusCode::BAD_REQUEST,
            "ID_MISMATCH",
            format!(
                "path chain_id ({chain_id}) does not match payload chain_id ({})",
                payload.chain_id
            ),
        ));
    }
    let uid = auth.user_id;
    let pool = state.pool.clone();
    crate::error::run_blocking(move || {
        let existing = chain_service::get_chain(&pool, chain_id).map_err(ApiError::from)?;
        crate::middleware::auth::check_ownership(uid, existing.chain.user_id)?;
        chain_service::update_chain(&pool, &payload).map_err(ApiError::from)
    })
    .await
    .map(Json)
}

/// DELETE /api/v1/chains/{chain_id}
///
/// Deletes a chain owned by the authenticated user.
///
/// # Errors
///
/// Returns HTTP 404 if the chain does not exist.
/// Returns HTTP 403 if the chain is not owned by the authenticated user.
/// Returns HTTP 500 if the database deletion fails.
pub async fn delete_chain(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(chain_id): Path<i64>,
) -> Result<axum::http::StatusCode, ApiError> {
    let uid = auth.user_id;
    let pool = state.pool.clone();
    crate::error::run_blocking(move || {
        let existing = chain_service::get_chain(&pool, chain_id).map_err(ApiError::from)?;
        crate::middleware::auth::check_ownership(uid, existing.chain.user_id)?;
        chain_service::delete_chain(&pool, chain_id).map_err(ApiError::from)
    })
    .await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

/// POST /api/v1/chains/{chain_id}/duplicate
///
/// Creates a copy of an existing chain, including its steps and associations.
///
/// # Errors
///
/// Returns HTTP 404 if the chain does not exist.
/// Returns HTTP 403 if the chain is not owned by the authenticated user.
/// Returns HTTP 500 if the database operation fails.
pub async fn duplicate_chain(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(chain_id): Path<i64>,
) -> Result<Json<Chain>, ApiError> {
    let uid = auth.user_id;
    let pool = state.pool.clone();
    crate::error::run_blocking(move || {
        let existing = chain_service::get_chain(&pool, chain_id).map_err(ApiError::from)?;
        crate::middleware::auth::check_ownership(uid, existing.chain.user_id)?;
        chain_service::duplicate_chain(&pool, chain_id).map_err(ApiError::from)
    })
    .await
    .map(Json)
}

/// PATCH /api/v1/chains/{chain_id}/favorite
///
/// Toggles or explicitly sets the favorite flag on a chain. When the request
/// body contains a `value` field, that value is used; otherwise the current
/// flag is inverted.
///
/// # Errors
///
/// Returns HTTP 404 if the chain does not exist.
/// Returns HTTP 403 if the chain is not owned by the authenticated user.
/// Returns HTTP 500 if the database update fails.
pub async fn toggle_favorite(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(chain_id): Path<i64>,
    body: Option<Json<TogglePayload>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let payload = body.map(|j| j.0).unwrap_or_default();
    let uid = auth.user_id;
    let pool = state.pool.clone();
    crate::error::run_blocking(move || {
        let existing = chain_service::get_chain(&pool, chain_id).map_err(ApiError::from)?;
        crate::middleware::auth::check_ownership(uid, existing.chain.user_id)?;
        let new_value = payload.value.unwrap_or(!existing.chain.is_favorite);
        chain_service::toggle_chain_favorite(&pool, chain_id, new_value).map_err(ApiError::from)?;
        Ok(serde_json::json!({ "is_favorite": new_value }))
    })
    .await
    .map(Json)
}

/// PATCH /api/v1/chains/{chain_id}/archive
///
/// Toggles or explicitly sets the archived flag on a chain. When the request
/// body contains a `value` field, that value is used; otherwise the current
/// flag is inverted.
///
/// # Errors
///
/// Returns HTTP 404 if the chain does not exist.
/// Returns HTTP 403 if the chain is not owned by the authenticated user.
/// Returns HTTP 500 if the database update fails.
pub async fn toggle_archive(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(chain_id): Path<i64>,
    body: Option<Json<TogglePayload>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let payload = body.map(|j| j.0).unwrap_or_default();
    let uid = auth.user_id;
    let pool = state.pool.clone();
    crate::error::run_blocking(move || {
        let existing = chain_service::get_chain(&pool, chain_id).map_err(ApiError::from)?;
        crate::middleware::auth::check_ownership(uid, existing.chain.user_id)?;
        let new_value = payload.value.unwrap_or(!existing.chain.is_archived);
        chain_service::toggle_chain_archive(&pool, chain_id, new_value).map_err(ApiError::from)?;
        Ok(serde_json::json!({ "is_archived": new_value }))
    })
    .await
    .map(Json)
}

#[derive(Serialize)]
pub struct ComposedContentResponse {
    pub content: String,
}

/// GET /api/v1/chains/{chain_id}/content
///
/// Returns the fully composed content of a chain by concatenating the content
/// of each step in order.
///
/// # Errors
///
/// Returns HTTP 404 if the chain does not exist.
/// Returns HTTP 403 if the chain is not owned by the authenticated user.
/// Returns HTTP 500 if the database query fails.
pub async fn get_composed_content(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(chain_id): Path<i64>,
) -> Result<Json<ComposedContentResponse>, ApiError> {
    let uid = auth.user_id;
    let pool = state.pool.clone();
    crate::error::run_blocking(move || {
        let existing = chain_service::get_chain(&pool, chain_id).map_err(ApiError::from)?;
        crate::middleware::auth::check_ownership(uid, existing.chain.user_id)?;
        chain_service::get_composed_content(&pool, chain_id)
            .map(|content| ComposedContentResponse { content })
            .map_err(ApiError::from)
    })
    .await
    .map(Json)
}

/// GET /api/v1/chains/by-prompt/{prompt_id}
///
/// Lists all chains owned by the authenticated user that reference the given
/// prompt in at least one step.
///
/// # Errors
///
/// Returns HTTP 500 if the database query fails.
pub async fn chains_for_prompt(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(prompt_id): Path<i64>,
) -> Result<Json<Vec<Chain>>, ApiError> {
    let uid = auth.user_id;
    let pool = state.pool.clone();
    crate::error::run_blocking(move || {
        chain_service::get_chains_for_prompt_by_user(&pool, uid, prompt_id).map_err(ApiError::from)
    })
    .await
    .map(Json)
}

#[derive(Serialize)]
pub struct StepVariableInfo {
    pub position: i32,
    pub step_type: String,
    pub title: String,
    pub variables: Vec<String>,
}

#[derive(Serialize)]
pub struct ChainVariablesResponse {
    pub variables: Vec<String>,
    pub steps: Vec<StepVariableInfo>,
}

/// GET /api/v1/chains/{chain_id}/variables
///
/// Extracts template variables from each step of the chain and returns them
/// grouped by step as well as a de-duplicated aggregate list.
///
/// # Errors
///
/// Returns HTTP 404 if the chain does not exist.
/// Returns HTTP 403 if the chain is not owned by the authenticated user.
/// Returns HTTP 500 if the database query fails.
pub async fn get_chain_variables(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(chain_id): Path<i64>,
) -> Result<Json<ChainVariablesResponse>, ApiError> {
    let uid = auth.user_id;
    let pool = state.pool.clone();
    crate::error::run_blocking(move || {
        let cws = chain_service::get_chain(&pool, chain_id).map_err(ApiError::from)?;
        crate::middleware::auth::check_ownership(uid, cws.chain.user_id)?;

        let mut all_vars: Vec<String> = Vec::new();
        let mut seen = std::collections::HashSet::new();
        let mut step_infos = Vec::new();

        for resolved in &cws.steps {
            let content = resolved
                .prompt
                .as_ref()
                .map(|p| p.content.as_str())
                .or_else(|| resolved.script.as_ref().map(|s| s.content.as_str()))
                .unwrap_or("");
            let title = resolved
                .prompt
                .as_ref()
                .map(|p| p.title.as_str())
                .or_else(|| resolved.script.as_ref().map(|s| s.title.as_str()))
                .unwrap_or("");
            let vars = neuronprompter_core::template::extract_template_variables(content);
            for v in &vars {
                if seen.insert(v.clone()) {
                    all_vars.push(v.clone());
                }
            }
            step_infos.push(StepVariableInfo {
                position: resolved.step.position,
                step_type: resolved.step.step_type.to_string(),
                title: title.to_owned(),
                variables: vars,
            });
        }

        Ok(ChainVariablesResponse {
            variables: all_vars,
            steps: step_infos,
        })
    })
    .await
    .map(Json)
}

#[derive(Deserialize)]
pub struct SearchChainsPayload {
    pub user_id: i64,
    pub query: String,
    #[serde(default)]
    pub filter: ChainFilter,
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

/// POST /api/v1/search/chains
///
/// Performs a full-text search across chains owned by the authenticated user,
/// with optional filtering by favorite, archived, tag, category, and
/// collection.
///
/// # Errors
///
/// Returns HTTP 403 if the payload `user_id` does not match the authenticated
/// user.
/// Returns HTTP 500 if the search query fails.
pub async fn search_chains(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(mut payload): Json<SearchChainsPayload>,
) -> Result<Json<Vec<Chain>>, ApiError> {
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
        search_service::search_chains(&pool, effective_user_id, &payload.query, &payload.filter)
            .map_err(ApiError::from)
    })
    .await
    .map(Json)
}

/// POST /api/v1/chains/bulk-update
///
/// Applies bulk changes (favorite, archived, tag/category/collection
/// additions and removals) to multiple chains at once.
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
        chain_service::bulk_update(&pool, active_uid, &input).map_err(ApiError::from)
    })
    .await
    .map(|count| Json(serde_json::json!({"updated": count})))
}
