use std::collections::HashMap;
use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use serde::Serialize;

use crate::error::ApiError;
use crate::middleware::session::AuthUser;
use crate::state::{AppState, ClipboardEntry};

#[derive(Debug, Clone, Serialize)]
pub struct CopyResult {
    pub copied: bool,
    pub variables: Vec<String>,
}

#[derive(serde::Deserialize)]
pub struct CopyPayload {
    pub content: String,
    pub prompt_title: String,
}

/// POST /api/v1/clipboard/copy
///
/// Copies content to the clipboard history. If the content contains
/// unresolved template variables, the copy is not performed and the list of
/// variable names is returned instead.
///
/// # Errors
///
/// Returns HTTP 500 if an internal error occurs.
pub async fn copy_to_clipboard(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(payload): Json<CopyPayload>,
) -> Result<Json<CopyResult>, ApiError> {
    let user_id = auth.user_id;
    let variables = neuronprompter_core::template::extract_template_variables(&payload.content);

    if !variables.is_empty() {
        return Ok(Json(CopyResult {
            copied: false,
            variables,
        }));
    }

    // In web mode, clipboard is handled by the browser's navigator.clipboard API.
    // The server records the history entry; the frontend does the actual clipboard write.
    state.clipboard.push(
        user_id,
        ClipboardEntry {
            content: payload.content,
            prompt_title: payload.prompt_title,
            copied_at: chrono::Utc::now().to_rfc3339(),
        },
    );

    Ok(Json(CopyResult {
        copied: true,
        variables: Vec::new(),
    }))
}

#[derive(serde::Deserialize)]
pub struct CopyWithSubstitutionPayload {
    pub content: String,
    pub prompt_title: String,
    pub values: HashMap<String, String>,
}

/// POST /api/v1/clipboard/copy-substituted
///
/// Substitutes template variables in the content using the provided value map,
/// then copies the result to the clipboard history.
///
/// # Errors
///
/// Returns HTTP 500 if an internal error occurs.
pub async fn copy_with_substitution(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(payload): Json<CopyWithSubstitutionPayload>,
) -> Result<Json<String>, ApiError> {
    let user_id = auth.user_id;
    let substituted =
        neuronprompter_core::template::substitute_variables(&payload.content, &payload.values);

    state.clipboard.push(
        user_id,
        ClipboardEntry {
            content: substituted.clone(),
            prompt_title: payload.prompt_title,
            copied_at: chrono::Utc::now().to_rfc3339(),
        },
    );

    Ok(Json(substituted))
}

/// GET /api/v1/clipboard/history
///
/// Returns the clipboard history entries for the authenticated user, ordered
/// from most recent to oldest.
///
/// # Errors
///
/// Returns HTTP 500 if an internal error occurs.
pub async fn get_clipboard_history(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> Result<Json<Vec<ClipboardEntry>>, ApiError> {
    let user_id = auth.user_id;
    Ok(Json(state.clipboard.entries(user_id)))
}

/// DELETE /api/v1/clipboard/history
///
/// Clears all clipboard history entries for the authenticated user.
///
/// # Errors
///
/// Returns HTTP 500 if an internal error occurs.
pub async fn clear_clipboard_history(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> Result<Json<()>, ApiError> {
    let user_id = auth.user_id;
    state.clipboard.clear(user_id);
    Ok(Json(()))
}
