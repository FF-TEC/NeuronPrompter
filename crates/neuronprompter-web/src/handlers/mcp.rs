// =============================================================================
// MCP (Model Context Protocol) registration status and management handlers.
//
// Provides endpoints to check the MCP server registration status for both
// Claude Code and Claude Desktop App, install (register) the NeuronPrompter
// MCP server in either client's configuration, and uninstall (deregister) it.
// The actual registration logic lives in `neuronprompter_mcp::registration`.
// =============================================================================

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use serde::Serialize;

use neuronprompter_mcp::registration::{self, McpTarget};

use crate::WebState;

// Web handlers use manual session extraction instead of the API middleware
// extractors (AuthUser, AuthSession) because web-specific routes are mounted
// on a separate router with WebState and do not pass through the API session
// middleware layer. The manual approach reads the same HttpOnly session cookie
// and performs equivalent validation.

/// Helper: check session from cookie headers.
fn has_valid_session(web_state: &WebState, headers: &HeaderMap) -> bool {
    let token = neuronprompter_api::middleware::session::extract_cookie_token(headers);
    token
        .and_then(|t| web_state.app_state.sessions.get_session(&t))
        .is_some_and(|s| s.user_id().is_some())
}

/// Status for a single MCP target (Claude Code or Claude Desktop App).
#[derive(Serialize)]
pub struct McpTargetStatus {
    /// True when the `mcpServers.neuronprompter` key exists in the target's config.
    pub registered: bool,
    /// Filesystem path to the config file for this target.
    pub config_path: String,
}

/// Response body for `GET /api/v1/web/mcp/status`.
///
/// Returns registration status for both supported MCP targets independently,
/// plus the NeuronPrompter binary version string.
#[derive(Serialize)]
pub struct McpStatusResponse {
    /// Registration status for Claude Code (`~/.claude.json`).
    pub claude_code: McpTargetStatus,
    /// Registration status for Claude Desktop App (platform-specific path).
    pub claude_desktop: McpTargetStatus,
    /// The current NeuronPrompter binary version string from `CARGO_PKG_VERSION`.
    pub server_version: String,
}

/// Returns MCP registration status for both Claude Code and Claude Desktop App.
/// Requires an active user session to prevent leaking config file paths to
/// unauthenticated clients.
///
/// # Errors
///
/// Returns a 401 JSON error if the request has no valid session cookie or
/// no user is selected.
pub async fn mcp_status(
    State(state): State<Arc<WebState>>,
    headers: HeaderMap,
) -> Result<Json<McpStatusResponse>, (StatusCode, Json<serde_json::Value>)> {
    if !has_valid_session(&state, &headers) {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({
                "code": "AUTH_REQUIRED",
                "message": "an active user must be set to perform this action"
            })),
        ));
    }

    let code_status = registration::check_status(McpTarget::ClaudeCode);
    let desktop_status = registration::check_status(McpTarget::ClaudeDesktop);

    Ok(Json(McpStatusResponse {
        claude_code: McpTargetStatus {
            registered: code_status.registered,
            config_path: code_status.config_path.unwrap_or_default(),
        },
        claude_desktop: McpTargetStatus {
            registered: desktop_status.registered,
            config_path: desktop_status.config_path.unwrap_or_default(),
        },
        server_version: env!("CARGO_PKG_VERSION").to_string(),
    }))
}

/// Registers the NeuronPrompter MCP server for the specified target by calling
/// the registration module directly.
pub async fn mcp_install(
    State(state): State<Arc<WebState>>,
    headers: HeaderMap,
    Path(target): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    // Require an active user to perform MCP install operations.
    if !has_valid_session(&state, &headers) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({
                "code": "AUTH_REQUIRED",
                "message": "an active user must be set to perform this action"
            })),
        );
    }

    let mcp_target = match registration::parse_target(&target) {
        Ok(t) => t,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "code": "INVALID_TARGET", "message": e })),
            );
        }
    };

    match registration::install(None, mcp_target) {
        Ok(msg) => (
            StatusCode::ACCEPTED,
            Json(serde_json::json!({ "status": "installed", "target": target, "message": msg })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "code": "MCP_INSTALL_FAILED", "message": format!("MCP install failed: {e}")
            })),
        ),
    }
}

/// Unregisters the NeuronPrompter MCP server for the specified target by
/// calling the registration module directly.
pub async fn mcp_uninstall(
    State(state): State<Arc<WebState>>,
    headers: HeaderMap,
    Path(target): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    // Require an active user to perform MCP uninstall operations.
    if !has_valid_session(&state, &headers) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({
                "code": "AUTH_REQUIRED",
                "message": "an active user must be set to perform this action"
            })),
        );
    }

    let mcp_target = match registration::parse_target(&target) {
        Ok(t) => t,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "code": "INVALID_TARGET", "message": e })),
            );
        }
    };

    match registration::uninstall(mcp_target) {
        Ok(msg) => (
            StatusCode::ACCEPTED,
            Json(serde_json::json!({ "status": "uninstalled", "target": target, "message": msg })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "code": "MCP_UNINSTALL_FAILED", "message": format!("MCP uninstall failed: {e}")
            })),
        ),
    }
}
