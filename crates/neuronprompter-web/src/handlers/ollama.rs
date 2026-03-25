// =============================================================================
// Web-specific Ollama model management handlers.
//
// These endpoints allow the browser-based frontend to interact with a local
// Ollama instance for model management operations: listing installed and
// running models, browsing a curated catalog, pulling new models with
// streaming progress, deleting models, and showing model details.
//
// Endpoints:
// - GET  /api/v1/web/ollama/status  -- connectivity check
// - GET  /api/v1/web/ollama/models  -- list installed models
// - GET  /api/v1/web/ollama/running -- list loaded/running models
// - GET  /api/v1/web/ollama/catalog -- curated catalog of popular models
// - POST /api/v1/web/ollama/pull    -- start model download with progress
// - POST /api/v1/web/ollama/delete  -- remove an installed model
// - POST /api/v1/web/ollama/show    -- get detailed model metadata
// =============================================================================

use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use serde::{Deserialize, Serialize};
use tracing::{error, info};

use neuronprompter_application::ollama::client::OllamaClient;

use crate::WebState;

// Web handlers use manual session extraction instead of the API middleware
// extractors (AuthUser, AuthSession) because web-specific routes are mounted
// on a separate router with WebState and do not pass through the API session
// middleware layer. The manual approach reads the same HttpOnly session cookie
// and performs equivalent validation.

/// Helper: extract user_id from session cookie.
fn session_user_id(web_state: &WebState, headers: &HeaderMap) -> Option<i64> {
    let token = neuronprompter_api::middleware::session::extract_cookie_token(headers)?;
    let session_ref = web_state.app_state.sessions.get_session(&token)?;
    session_ref.user_id()
}

/// Helper: check session has valid user.
fn has_valid_session(web_state: &WebState, headers: &HeaderMap) -> bool {
    session_user_id(web_state, headers).is_some()
}

// ---------------------------------------------------------------------------
// Curated model catalog
// ---------------------------------------------------------------------------

/// A single entry in the curated Ollama model catalog.
#[derive(Debug, Clone, Serialize)]
pub struct CatalogEntry {
    /// Ollama model tag for pulling (e.g., "llama3.2:3b").
    pub name: &'static str,
    /// Model architecture family (e.g., "Llama", "Qwen").
    pub family: &'static str,
    /// Human-readable parameter count (e.g., "3B", "7B").
    pub params: &'static str,
    /// Short description of the model.
    pub description: &'static str,
}

/// Curated list of popular Ollama models.
static CATALOG: &[CatalogEntry] = &[
    CatalogEntry {
        name: "llama3.2:3b",
        family: "Llama",
        params: "3B",
        description: "Meta's Llama 3.2 3B",
    },
    CatalogEntry {
        name: "llama3.1:8b",
        family: "Llama",
        params: "8B",
        description: "Meta's Llama 3.1 8B",
    },
    CatalogEntry {
        name: "gemma2:9b",
        family: "Gemma",
        params: "9B",
        description: "Google's Gemma 2 9B",
    },
    CatalogEntry {
        name: "mistral:7b",
        family: "Mistral",
        params: "7B",
        description: "Mistral 7B",
    },
    CatalogEntry {
        name: "qwen2.5:7b",
        family: "Qwen",
        params: "7B",
        description: "Alibaba's Qwen 2.5 7B",
    },
    CatalogEntry {
        name: "phi3:mini",
        family: "Phi",
        params: "3.8B",
        description: "Microsoft's Phi-3 Mini",
    },
    CatalogEntry {
        name: "codellama:7b",
        family: "Code Llama",
        params: "7B",
        description: "Meta's Code Llama 7B",
    },
    CatalogEntry {
        name: "deepseek-coder-v2:16b",
        family: "DeepSeek",
        params: "16B",
        description: "DeepSeek Coder V2",
    },
];

// ---------------------------------------------------------------------------
// Request / response types
// ---------------------------------------------------------------------------

/// Request body for POST endpoints that target a specific model.
#[derive(Debug, Deserialize)]
pub struct ModelRequest {
    /// Model tag to operate on (e.g., "llama3.2:3b").
    pub model: String,
}

/// Response from the status endpoint.
#[derive(Debug, Serialize)]
pub struct StatusResponse {
    /// Whether the Ollama server responded to a health check.
    pub connected: bool,
}

// ---------------------------------------------------------------------------
// Helper: resolve Ollama base URL from user settings in the database
// ---------------------------------------------------------------------------

use neuronprompter_core::constants::DEFAULT_OLLAMA_URL;

/// Reads the active user's `ollama_base_url` from the database. Falls back to
/// the default URL if no active user is set or if the read fails.
///
/// Database access is performed inside `spawn_blocking` to avoid blocking the
/// async runtime.
async fn resolve_ollama_url(web_state: &WebState, user_id: Option<i64>) -> String {
    let Some(user_id) = user_id else {
        return DEFAULT_OLLAMA_URL.to_string();
    };

    let pool = web_state.app_state.pool.clone();

    let result = tokio::task::spawn_blocking(move || {
        let Ok(conn) = pool.get() else {
            return DEFAULT_OLLAMA_URL.to_string();
        };

        match neuronprompter_db::repo::settings::get_user_settings(&conn, user_id) {
            Ok(settings) => {
                let url = settings.ollama_base_url.trim().to_string();
                if url.is_empty() {
                    DEFAULT_OLLAMA_URL.to_string()
                } else if neuronprompter_core::validation::validate_ollama_url(&url).is_err() {
                    tracing::warn!(
                        url = %url,
                        "user-configured Ollama URL failed SSRF validation; falling back to default"
                    );
                    DEFAULT_OLLAMA_URL.to_string()
                } else {
                    url
                }
            }
            Err(_) => DEFAULT_OLLAMA_URL.to_string(),
        }
    })
    .await;

    result.unwrap_or_else(|_| DEFAULT_OLLAMA_URL.to_string())
}

// ---------------------------------------------------------------------------
// Handler functions
// ---------------------------------------------------------------------------

/// GET /api/v1/web/ollama/status
///
/// Checks Ollama server connectivity by attempting to list models.
pub async fn ollama_status(
    State(web_state): State<Arc<WebState>>,
    headers: HeaderMap,
) -> Json<StatusResponse> {
    let uid = session_user_id(&web_state, &headers);
    let url = resolve_ollama_url(&web_state, uid).await;
    let client = OllamaClient::with_base_url(&url);
    let connected = client.check_health().await.is_ok();
    Json(StatusResponse { connected })
}

/// GET /api/v1/web/ollama/models
///
/// Lists all models installed on the Ollama server.
///
/// # Errors
///
/// Returns a 502 (BAD_GATEWAY) JSON error if the Ollama server is
/// unreachable or the model listing request fails.
pub async fn list_models(
    State(web_state): State<Arc<WebState>>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, impl IntoResponse> {
    let uid = session_user_id(&web_state, &headers);
    let url = resolve_ollama_url(&web_state, uid).await;
    let client = OllamaClient::with_base_url(&url);

    match client.list_local_models().await {
        Ok(models) => Ok(Json(serde_json::json!({ "models": models }))),
        Err(e) => {
            tracing::warn!(error = %e, "failed to list Ollama models");
            Err((
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({
                    "code": "OLLAMA_LIST_MODELS_FAILED",
                    "message": "failed to list models from Ollama",
                })),
            ))
        }
    }
}

/// GET /api/v1/web/ollama/running
///
/// Lists models currently loaded in GPU/CPU RAM.
///
/// # Errors
///
/// Returns a 502 (BAD_GATEWAY) JSON error if the Ollama server is
/// unreachable or the running models request fails.
pub async fn list_running(
    State(web_state): State<Arc<WebState>>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, impl IntoResponse> {
    let uid = session_user_id(&web_state, &headers);
    let url = resolve_ollama_url(&web_state, uid).await;
    let client = OllamaClient::with_base_url(&url);

    match client.list_running_models().await {
        Ok(models) => Ok(Json(serde_json::json!({ "models": models }))),
        Err(e) => {
            tracing::warn!(error = %e, "failed to list running Ollama models");
            Err((
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({
                    "code": "OLLAMA_LIST_RUNNING_FAILED",
                    "message": "failed to list running models from Ollama",
                })),
            ))
        }
    }
}

/// GET /api/v1/web/ollama/catalog
///
/// Returns the curated list of popular Ollama models.
pub async fn catalog() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "models": CATALOG.to_vec() }))
}

/// POST /api/v1/web/ollama/pull
///
/// Initiates downloading a model from the Ollama registry. The pull runs
/// in a background tokio task. Progress events are broadcast via `model_tx`.
/// Returns 202 Accepted immediately.
///
/// # Errors
///
/// Returns a 401 JSON error if no valid session is present. Returns a 400
/// JSON error if the model name is empty. Returns a 502 JSON error if the
/// Ollama server is unreachable.
pub async fn pull_model(
    State(web_state): State<Arc<WebState>>,
    headers: HeaderMap,
    Json(req): Json<ModelRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), impl IntoResponse> {
    // Require an active user for model management operations.
    if !has_valid_session(&web_state, &headers) {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({
                "code": "AUTH_REQUIRED",
                "message": "an active user must be set to perform this action"
            })),
        ));
    }

    let model = req.model.trim().to_string();
    if model.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                serde_json::json!({ "code": "VALIDATION_ERROR", "message": "model name must not be empty" }),
            ),
        ));
    }

    let uid = session_user_id(&web_state, &headers);
    let url = resolve_ollama_url(&web_state, uid).await;
    let model_tx = web_state.model_tx.clone();

    // Verify Ollama is reachable before spawning.
    let client = OllamaClient::with_base_url(&url);
    if client.check_health().await.is_err() {
        tracing::warn!(url = %url, "Ollama unreachable for pull request");
        return Err((
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({
                "code": "OLLAMA_UNREACHABLE",
                "message": "Ollama is not reachable at the configured URL",
            })),
        ));
    }

    let model_for_task = model.clone();
    let tx = model_tx.clone();
    let cancel = web_state.app_state.cancellation.clone();

    tokio::spawn(async move {
        let client = OllamaClient::with_base_url(&url);
        let pull_fut = client.pull_model_streaming(&model_for_task, &tx);

        tokio::select! {
            result = pull_fut => {
                match result {
                    Ok(()) => {
                        info!(model = %model_for_task, "Ollama model pull completed");
                        let event = serde_json::json!({
                            "event": "ollama_pull_complete",
                            "model": &model_for_task,
                        });
                        let _ = tx.send(event.to_string());
                    }
                    Err(e) => {
                        error!(model = %model_for_task, error = %e, "Ollama model pull failed");
                        let event = serde_json::json!({
                            "event": "ollama_pull_error",
                            "model": &model_for_task,
                            "error": e.to_string(),
                        });
                        let _ = tx.send(event.to_string());
                    }
                }
            }
            () = cancel.cancelled() => {
                info!(model = %model_for_task, "Ollama model pull cancelled by shutdown");
                let event = serde_json::json!({
                    "event": "ollama_pull_error",
                    "model": &model_for_task,
                    "error": "pull cancelled due to server shutdown",
                });
                let _ = tx.send(event.to_string());
            }
        }
    });

    Ok((
        StatusCode::ACCEPTED,
        Json(serde_json::json!({
            "status": "pulling",
            "model": model,
        })),
    ))
}

/// POST /api/v1/web/ollama/delete
///
/// Removes an installed model from the Ollama server.
///
/// # Errors
///
/// Returns a 401 JSON error if no valid session is present. Returns a 400
/// JSON error if the model name is empty. Returns a 502 JSON error if
/// the Ollama server fails to delete the model.
pub async fn delete_model(
    State(web_state): State<Arc<WebState>>,
    headers: HeaderMap,
    Json(req): Json<ModelRequest>,
) -> Result<Json<serde_json::Value>, impl IntoResponse> {
    // Require an active user for model management operations.
    if !has_valid_session(&web_state, &headers) {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({
                "code": "AUTH_REQUIRED",
                "message": "an active user must be set to perform this action"
            })),
        ));
    }

    let model = req.model.trim().to_string();
    if model.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                serde_json::json!({ "code": "VALIDATION_ERROR", "message": "model name must not be empty" }),
            ),
        ));
    }

    let uid = session_user_id(&web_state, &headers);
    let url = resolve_ollama_url(&web_state, uid).await;
    let client = OllamaClient::with_base_url(&url);

    match client.delete_model(&model).await {
        Ok(()) => {
            info!(model = %model, "Ollama model deleted");
            Ok(Json(serde_json::json!({
                "status": "deleted",
                "model": model,
            })))
        }
        Err(e) => {
            tracing::warn!(model = %model, error = %e, "failed to delete Ollama model");
            Err((
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({
                    "code": "OLLAMA_DELETE_FAILED",
                    "message": "failed to delete the specified model",
                })),
            ))
        }
    }
}

/// POST /api/v1/web/ollama/show
///
/// Retrieves detailed metadata for a single installed model.
///
/// # Errors
///
/// Returns a 401 JSON error if no valid session is present. Returns a 400
/// JSON error if the model name is empty. Returns a 502 JSON error if
/// the Ollama server fails to return model details.
pub async fn show_model(
    State(web_state): State<Arc<WebState>>,
    headers: HeaderMap,
    Json(req): Json<ModelRequest>,
) -> Result<Json<serde_json::Value>, impl IntoResponse> {
    // F29: Require an active user, consistent with pull_model and delete_model.
    if !has_valid_session(&web_state, &headers) {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({
                "code": "AUTH_REQUIRED",
                "message": "an active user must be set to perform this action"
            })),
        ));
    }

    let model = req.model.trim().to_string();
    if model.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                serde_json::json!({ "code": "VALIDATION_ERROR", "message": "model name must not be empty" }),
            ),
        ));
    }

    let uid = session_user_id(&web_state, &headers);
    let url = resolve_ollama_url(&web_state, uid).await;
    let client = OllamaClient::with_base_url(&url);

    match client.show_model(&model).await {
        Ok(detail) => Ok(Json(detail)),
        Err(e) => {
            tracing::warn!(model = %model, error = %e, "failed to show Ollama model");
            Err((
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({
                    "code": "OLLAMA_SHOW_FAILED",
                    "message": "failed to retrieve model details",
                })),
            ))
        }
    }
}
