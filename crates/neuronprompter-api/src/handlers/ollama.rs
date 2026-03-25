use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use neuronprompter_application::ollama::client::OllamaClient;
use neuronprompter_application::ollama::metadata::{self, DerivedMetadata};
use neuronprompter_application::ollama::{improve, translate};
use neuronprompter_core::validation;
use serde::Serialize;

use crate::error::ApiError;
use crate::middleware::session::{AuthSession, AuthUser};
use crate::state::AppState;

#[derive(Debug, Clone, Serialize)]
pub struct OllamaStatus {
    pub connected: bool,
    pub models: Vec<String>,
}

#[derive(serde::Deserialize)]
pub struct StatusPayload {
    pub base_url: String,
}

/// POST /api/v1/ollama/status -- check Ollama connectivity and list available models.
/// Requires a valid session (AuthSession) so that unauthenticated clients on the
/// LAN cannot probe internal Ollama infrastructure.
///
/// # Errors
///
/// Returns HTTP 400 if the Ollama URL fails validation.
pub async fn ollama_status(
    State(state): State<Arc<AppState>>,
    _auth: AuthSession,
    Json(payload): Json<StatusPayload>,
) -> Result<Json<OllamaStatus>, ApiError> {
    validation::validate_ollama_url(&payload.base_url)
        .map_err(|e| ApiError::from(neuronprompter_application::ServiceError::Core(e)))?;
    // Reuse the shared HTTP client from application state instead of
    // constructing a per-request client. This shares the underlying
    // connection pool and TLS session cache across all Ollama handlers.
    let client = OllamaClient::with_client_and_url(state.ollama.http.clone(), &payload.base_url);
    // Ollama not being reachable is a normal operational state (user may not have
    // Ollama installed or running). The status endpoint always returns a 200 response
    // with {connected: true/false} instead of an HTTP error, so the frontend status
    // indicator updates cleanly without triggering error handling or log spam.
    let (connected, models) = if let Ok(models) = client.check_health().await {
        (true, models)
    } else {
        (false, vec![])
    };

    {
        let mut guard = state.ollama.connected.write().unwrap_or_else(|e| {
            tracing::warn!("RwLock poisoned on ollama state field, recovering: {e}");
            e.into_inner()
        });
        *guard = connected;
    }
    {
        let mut guard = state.ollama.models.write().unwrap_or_else(|e| {
            tracing::warn!("RwLock poisoned on ollama state field, recovering: {e}");
            e.into_inner()
        });
        guard.clone_from(&models);
    }

    Ok(Json(OllamaStatus { connected, models }))
}

#[derive(serde::Deserialize)]
pub struct ImprovePayload {
    pub base_url: String,
    pub model: String,
    pub content: String,
}

/// POST /api/v1/ollama/improve -- send prompt content to Ollama for improvement.
/// Requires an authenticated user (AuthUser) because this triggers an outbound
/// request to the configured Ollama instance on behalf of a specific user.
///
/// # Errors
///
/// Returns HTTP 400 if the URL, content, or content size fails validation.
/// Returns HTTP 500 if the Ollama request fails.
pub async fn ollama_improve(
    State(state): State<Arc<AppState>>,
    _auth: AuthUser,
    Json(payload): Json<ImprovePayload>,
) -> Result<Json<String>, ApiError> {
    validation::validate_ollama_url(&payload.base_url)
        .map_err(|e| ApiError::from(neuronprompter_application::ServiceError::Core(e)))?;
    validation::validate_content(&payload.content)
        .map_err(|e| ApiError::from(neuronprompter_application::ServiceError::Core(e)))?;
    validation::validate_content_size(&payload.content, validation::MAX_CONTENT_BYTES)
        .map_err(|e| ApiError::from(neuronprompter_application::ServiceError::Core(e)))?;
    // Reuse the shared HTTP client from application state to avoid
    // allocating a separate connection pool for each improve request.
    let client = OllamaClient::with_client_and_url(state.ollama.http.clone(), &payload.base_url);
    improve::improve_prompt(&client, &payload.model, &payload.content)
        .await
        .map(Json)
        .map_err(ApiError::from)
}

#[derive(serde::Deserialize)]
pub struct TranslatePayload {
    pub base_url: String,
    pub model: String,
    pub content: String,
    pub target_language: String,
}

/// POST /api/v1/ollama/translate -- send prompt content to Ollama for translation.
/// Requires an authenticated user (AuthUser) because this triggers an outbound
/// request to the configured Ollama instance on behalf of a specific user.
///
/// # Errors
///
/// Returns HTTP 400 if the URL, content, or content size fails validation.
/// Returns HTTP 500 if the Ollama request fails.
pub async fn ollama_translate(
    State(state): State<Arc<AppState>>,
    _auth: AuthUser,
    Json(payload): Json<TranslatePayload>,
) -> Result<Json<String>, ApiError> {
    validation::validate_ollama_url(&payload.base_url)
        .map_err(|e| ApiError::from(neuronprompter_application::ServiceError::Core(e)))?;
    validation::validate_content(&payload.content)
        .map_err(|e| ApiError::from(neuronprompter_application::ServiceError::Core(e)))?;
    validation::validate_content_size(&payload.content, validation::MAX_CONTENT_BYTES)
        .map_err(|e| ApiError::from(neuronprompter_application::ServiceError::Core(e)))?;
    // Reuse the shared HTTP client from application state to avoid
    // allocating a separate connection pool for each translate request.
    let client = OllamaClient::with_client_and_url(state.ollama.http.clone(), &payload.base_url);
    translate::translate_prompt(
        &client,
        &payload.model,
        &payload.content,
        &payload.target_language,
    )
    .await
    .map(Json)
    .map_err(ApiError::from)
}

#[derive(serde::Deserialize)]
pub struct AutofillPayload {
    pub base_url: String,
    pub model: String,
    pub content: String,
}

/// POST /api/v1/ollama/autofill -- derive metadata fields from prompt content via Ollama.
/// Requires an authenticated user (AuthUser) because this triggers an outbound
/// request to the configured Ollama instance on behalf of a specific user.
///
/// # Errors
///
/// Returns HTTP 400 if the URL, content, or content size fails validation.
/// Returns HTTP 500 if the Ollama request fails.
pub async fn ollama_autofill(
    State(state): State<Arc<AppState>>,
    _auth: AuthUser,
    Json(payload): Json<AutofillPayload>,
) -> Result<Json<DerivedMetadata>, ApiError> {
    validation::validate_ollama_url(&payload.base_url)
        .map_err(|e| ApiError::from(neuronprompter_application::ServiceError::Core(e)))?;
    validation::validate_content(&payload.content)
        .map_err(|e| ApiError::from(neuronprompter_application::ServiceError::Core(e)))?;
    validation::validate_content_size(&payload.content, validation::MAX_CONTENT_BYTES)
        .map_err(|e| ApiError::from(neuronprompter_application::ServiceError::Core(e)))?;
    // Reuse the shared HTTP client from application state to avoid
    // allocating a separate connection pool for each autofill request.
    let client = OllamaClient::with_client_and_url(state.ollama.http.clone(), &payload.base_url);
    metadata::derive_all_metadata(&client, &payload.model, &payload.content)
        .await
        .map(Json)
        .map_err(ApiError::from)
}
