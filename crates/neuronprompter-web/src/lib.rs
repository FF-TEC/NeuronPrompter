//! Browser-based frontend server for NeuronPrompter.
//!
//! This crate provides:
//! - **Assets**: Embedded static file serving with SPA fallback routing
//! - **SSE**: Server-Sent Event endpoints for real-time log/model streaming
//! - **Browser**: Cross-platform browser launcher
//! - **BroadcastLayer**: tracing Layer for SSE log forwarding
//! - **Handlers**: Web-specific REST endpoints (MCP registration, Ollama mgmt)

pub mod assets;
pub mod broadcast_layer;
pub mod browser;
pub mod handlers;
#[cfg(feature = "gui")]
pub mod native_window;
#[cfg(feature = "gui")]
pub mod splash;
pub mod sse;

#[cfg(test)]
mod tests;

use std::sync::Arc;
use std::sync::atomic::AtomicUsize;

use axum::Router;
use tokio::sync::broadcast;

use neuronprompter_api::AppState;

/// Web-specific state that extends the core `AppState` with SSE channels,
/// connection tracking, and native dialog capabilities.
pub struct WebState {
    /// Reference to the shared application state.
    pub app_state: Arc<AppState>,
    /// Broadcast sender for log events (SSE consumers subscribe via `.subscribe()`).
    pub log_tx: broadcast::Sender<String>,
    /// Broadcast sender for model operation events.
    pub model_tx: broadcast::Sender<String>,
    /// Current number of active SSE connections.
    pub sse_connections: AtomicUsize,
    /// Whether native file dialogs are available (true in GUI mode, false in browser).
    pub native_dialogs: bool,
}

impl WebState {
    /// Creates a new `WebState` with native dialogs disabled (browser mode).
    #[must_use]
    pub fn new(app_state: Arc<AppState>, log_tx: broadcast::Sender<String>) -> Self {
        let (model_tx, _) = broadcast::channel(64);
        Self {
            app_state,
            log_tx,
            model_tx,
            sse_connections: AtomicUsize::new(0),
            native_dialogs: false,
        }
    }

    /// Creates a new `WebState` with explicit native dialog support setting.
    #[must_use]
    pub fn with_native_dialogs(
        app_state: Arc<AppState>,
        log_tx: broadcast::Sender<String>,
        native_dialogs: bool,
    ) -> Self {
        let (model_tx, _) = broadcast::channel(64);
        Self {
            app_state,
            log_tx,
            model_tx,
            sse_connections: AtomicUsize::new(0),
            native_dialogs,
        }
    }
}

/// Maximum number of concurrent SSE connections allowed.
pub const MAX_SSE_CONNECTIONS: usize = 100;

/// Builds the complete web router by merging the API router with SSE
/// endpoints, web-specific handlers, and embedded frontend asset serving.
#[allow(clippy::needless_pass_by_value)]
pub fn build_web_router(
    app_state: Arc<AppState>,
    web_state: Arc<WebState>,
    allowed_origin: &str,
) -> Router {
    let api_router = neuronprompter_api::build_router(app_state, allowed_origin);

    let web_routes = Router::new()
        // SSE endpoints
        .route("/api/v1/events/logs", axum::routing::get(sse::logs_stream))
        .route(
            "/api/v1/events/models",
            axum::routing::get(sse::models_stream),
        )
        // Setup / first-run endpoints
        .route(
            "/api/v1/web/setup/status",
            axum::routing::get(handlers::setup::get_setup_status),
        )
        .route(
            "/api/v1/web/setup/complete",
            axum::routing::post(handlers::setup::mark_setup_complete),
        )
        // Doctor / dependency probe endpoints
        .route(
            "/api/v1/web/doctor/probes",
            axum::routing::get(handlers::doctor::run_probes),
        )
        // MCP registration management endpoints
        .route(
            "/api/v1/web/mcp/status",
            axum::routing::get(handlers::mcp::mcp_status),
        )
        .route(
            "/api/v1/web/mcp/{target}/install",
            axum::routing::post(handlers::mcp::mcp_install),
        )
        .route(
            "/api/v1/web/mcp/{target}/uninstall",
            axum::routing::post(handlers::mcp::mcp_uninstall),
        )
        // Ollama model management endpoints
        .route(
            "/api/v1/web/ollama/status",
            axum::routing::get(handlers::ollama::ollama_status),
        )
        .route(
            "/api/v1/web/ollama/models",
            axum::routing::get(handlers::ollama::list_models),
        )
        .route(
            "/api/v1/web/ollama/running",
            axum::routing::get(handlers::ollama::list_running),
        )
        .route(
            "/api/v1/web/ollama/catalog",
            axum::routing::get(handlers::ollama::catalog),
        )
        .route(
            "/api/v1/web/ollama/pull",
            axum::routing::post(handlers::ollama::pull_model),
        )
        .route(
            "/api/v1/web/ollama/delete",
            axum::routing::post(handlers::ollama::delete_model),
        )
        .route(
            "/api/v1/web/ollama/show",
            axum::routing::post(handlers::ollama::show_model),
        )
        // Native file dialog endpoints
        .route(
            "/api/v1/web/dialog/save",
            axum::routing::post(handlers::dialogs::save_dialog),
        )
        .route(
            "/api/v1/web/dialog/open-file",
            axum::routing::post(handlers::dialogs::open_file_dialog),
        )
        .route(
            "/api/v1/web/dialog/open-dir",
            axum::routing::post(handlers::dialogs::open_dir_dialog),
        )
        .with_state(web_state.clone());

    // Merge API routes, web routes, and fallback to embedded assets.
    api_router
        .merge(web_routes)
        .fallback(assets::serve_embedded)
}
