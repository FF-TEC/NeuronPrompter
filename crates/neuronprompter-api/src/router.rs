// =============================================================================
// API router construction.
//
// Builds the `/api/v1/` router tree with all REST endpoints, CORS middleware,
// security response headers, and shared application state. Split into
// sub-functions to stay within the clippy::too_many_lines threshold.
// =============================================================================

use std::sync::Arc;

use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::http::{HeaderValue, Method, header};
use axum::routing::{delete, get, patch, post, put};
use tower_http::cors::CorsLayer;
use tower_http::set_header::SetResponseHeaderLayer;

use crate::handlers;
use crate::middleware::rate_limit::rate_limit_layer;
use crate::state::AppState;

/// Builds the complete API router with all endpoints and middleware.
///
/// The `allowed_origin` parameter restricts CORS to the specified origin
/// (e.g. `http://127.0.0.1:3030`). Wildcard `"*"` is only accepted when
/// `is_dev` is `true`; otherwise it is replaced with the localhost default.
///
/// When `headless` is `true` (serve mode), the shutdown endpoint is not
/// registered because the session token cannot be securely delivered to
/// the caller. Use Ctrl+C or SIGTERM to stop the server instead.
pub fn build_router(state: Arc<AppState>, allowed_origin: &str) -> Router {
    build_router_with_options(state, allowed_origin, false, false)
}

/// Builds the API router for headless (serve) mode.
///
/// Same as [`build_router`] but omits the shutdown endpoint.
pub fn build_router_headless(state: Arc<AppState>, allowed_origin: &str) -> Router {
    build_router_with_options(state, allowed_origin, true, false)
}

/// Builds the API router with the `is_dev` flag controlling whether CORS
/// wildcard origins are permitted.
pub fn build_router_dev(state: Arc<AppState>, allowed_origin: &str, is_dev: bool) -> Router {
    build_router_with_options(state, allowed_origin, false, is_dev)
}

/// Logs a warning when the server bind address is not localhost.
pub fn warn_if_network_exposed(bind_addr: &str) {
    let host = bind_addr.rsplit_once(':').map_or(bind_addr, |(h, _)| h);
    let is_localhost = matches!(host, "127.0.0.1" | "::1" | "localhost");
    if !is_localhost {
        tracing::warn!("=========================================================================");
        tracing::warn!("WARNING: Server is binding to {bind_addr} which is NOT localhost.");
        tracing::warn!("This exposes the API to the network. Ensure this is intentional and");
        tracing::warn!("that appropriate firewall rules are in place.");
        tracing::warn!("=========================================================================");
    }
}

#[allow(clippy::expect_used, clippy::too_many_lines)]
fn build_router_with_options(
    state: Arc<AppState>,
    allowed_origin: &str,
    headless: bool,
    is_dev: bool,
) -> Router {
    let effective_origin = if allowed_origin == "*" && !is_dev {
        tracing::warn!(
            "CORS wildcard origin requested but --dev flag not set; \
             falling back to localhost default."
        );
        "http://127.0.0.1:3030"
    } else if allowed_origin == "*" {
        tracing::warn!("CORS wildcard origin enabled (dev mode). Do NOT use in production.");
        "*"
    } else {
        allowed_origin
    };

    let cors = if effective_origin == "*" {
        // Development mode: restrict CORS to common localhost origins instead of
        // accepting all origins. This prevents accidental exposure when dev mode
        // is combined with a network-facing bind address.
        CorsLayer::new()
            .allow_origin([
                "http://localhost:3000"
                    .parse::<HeaderValue>()
                    .expect("compile-time constant header value"),
                "http://localhost:3030"
                    .parse::<HeaderValue>()
                    .expect("compile-time constant header value"),
                "http://localhost:5173"
                    .parse::<HeaderValue>()
                    .expect("compile-time constant header value"),
                "http://127.0.0.1:3000"
                    .parse::<HeaderValue>()
                    .expect("compile-time constant header value"),
                "http://127.0.0.1:3030"
                    .parse::<HeaderValue>()
                    .expect("compile-time constant header value"),
                "http://127.0.0.1:5173"
                    .parse::<HeaderValue>()
                    .expect("compile-time constant header value"),
            ])
            .allow_methods([
                Method::GET,
                Method::POST,
                Method::PUT,
                Method::PATCH,
                Method::DELETE,
            ])
            .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION])
    } else {
        CorsLayer::new()
            .allow_origin(match effective_origin.parse::<HeaderValue>() {
                Ok(val) => val,
                Err(err) => {
                    tracing::warn!(
                        origin = %effective_origin,
                        error = %err,
                        "failed to parse CORS allowed_origin; falling back to 127.0.0.1 loopback"
                    );
                    HeaderValue::from_static("http://127.0.0.1:3030")
                }
            })
            .allow_methods([
                Method::GET,
                Method::POST,
                Method::PUT,
                Method::PATCH,
                Method::DELETE,
            ])
            .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION])
    };

    Router::new()
        .merge(system_routes(headless))
        .merge(session_routes())
        .merge(user_routes())
        .merge(prompt_routes())
        .merge(script_routes())
        .merge(chain_routes())
        .merge(copy_routes())
        .merge(taxonomy_routes())
        .merge(version_routes())
        .merge(script_version_routes())
        .merge(data_routes())
        .merge(ollama_routes())
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            crate::middleware::session::session_middleware,
        ))
        .layer(DefaultBodyLimit::max(2_097_152)) // 2 MiB default body limit
        .layer(axum::middleware::from_fn_with_state(state.clone(), rate_limit_layer))
        // Security response headers -- applied to every response to mitigate
        // common browser-side attack vectors (clickjacking, MIME sniffing, etc.).
        .layer(SetResponseHeaderLayer::overriding(
            header::HeaderName::from_static("x-content-type-options"),
            HeaderValue::from_static("nosniff"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::HeaderName::from_static("x-frame-options"),
            HeaderValue::from_static("DENY"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::HeaderName::from_static("referrer-policy"),
            HeaderValue::from_static("strict-origin-when-cross-origin"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::HeaderName::from_static("permissions-policy"),
            HeaderValue::from_static("camera=(), microphone=(), geolocation=()"),
        ))
        // 'unsafe-inline' in style-src is required because SolidJS generates
        // inline styles at runtime for dynamic UI components (e.g. drag-and-drop
        // positioning, conditional visibility). Removing it breaks the frontend.
        .layer(SetResponseHeaderLayer::overriding(
            header::HeaderName::from_static("content-security-policy"),
            HeaderValue::from_static("default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src 'self'"),
        ))
        .layer(cors)
        .with_state(state)
}

fn session_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/v1/sessions", post(handlers::sessions::create_session))
        .route(
            "/api/v1/sessions/switch",
            put(handlers::sessions::switch_session),
        )
        .route("/api/v1/sessions", delete(handlers::sessions::logout))
        .route("/api/v1/sessions/me", get(handlers::sessions::session_me))
}

fn system_routes(headless: bool) -> Router<Arc<AppState>> {
    let router = Router::new().route("/api/v1/health", get(handlers::health::health));
    if headless {
        router
    } else {
        router.route("/api/v1/shutdown", post(handlers::shutdown::shutdown))
    }
}

fn user_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/v1/users", get(handlers::users::list_users))
        .route("/api/v1/users", post(handlers::users::create_user))
        .route("/api/v1/users/{user_id}", put(handlers::users::update_user))
        .route(
            "/api/v1/users/{user_id}/switch",
            put(handlers::users::switch_user),
        )
        .route(
            "/api/v1/users/{user_id}",
            delete(handlers::users::delete_user),
        )
}

fn prompt_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route(
            "/api/v1/prompts/search",
            post(handlers::prompts::list_prompts),
        )
        .route("/api/v1/prompts", post(handlers::prompts::create_prompt))
        .route(
            "/api/v1/prompts/{prompt_id}",
            get(handlers::prompts::get_prompt),
        )
        .route(
            "/api/v1/prompts/{prompt_id}",
            put(handlers::prompts::update_prompt),
        )
        .route(
            "/api/v1/prompts/{prompt_id}",
            delete(handlers::prompts::delete_prompt),
        )
        .route(
            "/api/v1/prompts/{prompt_id}/duplicate",
            post(handlers::prompts::duplicate_prompt),
        )
        .route(
            "/api/v1/prompts/{prompt_id}/favorite",
            patch(handlers::prompts::toggle_favorite),
        )
        .route(
            "/api/v1/prompts/{prompt_id}/archive",
            patch(handlers::prompts::toggle_archive),
        )
        .route(
            "/api/v1/search/prompts",
            post(handlers::search::search_prompts),
        )
        .route(
            "/api/v1/prompts/bulk-update",
            post(handlers::prompts::bulk_update),
        )
        .route(
            "/api/v1/prompts/count",
            get(handlers::prompts::count_prompts),
        )
        .route(
            "/api/v1/prompts/languages",
            get(handlers::prompts::list_languages),
        )
}

fn script_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route(
            "/api/v1/scripts/search",
            post(handlers::scripts::list_scripts),
        )
        .route("/api/v1/scripts", post(handlers::scripts::create_script))
        .route(
            "/api/v1/scripts/sync",
            post(handlers::scripts::sync_scripts),
        )
        .route(
            "/api/v1/scripts/import-file",
            post(handlers::scripts::import_file),
        )
        .route(
            "/api/v1/scripts/{script_id}",
            get(handlers::scripts::get_script),
        )
        .route(
            "/api/v1/scripts/{script_id}",
            put(handlers::scripts::update_script),
        )
        .route(
            "/api/v1/scripts/{script_id}",
            delete(handlers::scripts::delete_script),
        )
        .route(
            "/api/v1/scripts/{script_id}/duplicate",
            post(handlers::scripts::duplicate_script),
        )
        .route(
            "/api/v1/scripts/{script_id}/favorite",
            patch(handlers::scripts::toggle_favorite),
        )
        .route(
            "/api/v1/scripts/{script_id}/archive",
            patch(handlers::scripts::toggle_archive),
        )
        .route(
            "/api/v1/search/scripts",
            post(handlers::search::search_scripts),
        )
        .route(
            "/api/v1/scripts/bulk-update",
            post(handlers::scripts::bulk_update),
        )
        .route(
            "/api/v1/scripts/count",
            get(handlers::scripts::count_scripts),
        )
        .route(
            "/api/v1/scripts/languages",
            get(handlers::scripts::list_languages),
        )
}

fn chain_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/v1/chains/search", post(handlers::chains::list_chains))
        .route("/api/v1/chains", post(handlers::chains::create_chain))
        .route(
            "/api/v1/chains/{chain_id}",
            get(handlers::chains::get_chain),
        )
        .route(
            "/api/v1/chains/{chain_id}",
            put(handlers::chains::update_chain),
        )
        .route(
            "/api/v1/chains/{chain_id}",
            delete(handlers::chains::delete_chain),
        )
        .route(
            "/api/v1/chains/{chain_id}/duplicate",
            post(handlers::chains::duplicate_chain),
        )
        .route(
            "/api/v1/chains/{chain_id}/favorite",
            patch(handlers::chains::toggle_favorite),
        )
        .route(
            "/api/v1/chains/{chain_id}/archive",
            patch(handlers::chains::toggle_archive),
        )
        .route(
            "/api/v1/chains/{chain_id}/content",
            get(handlers::chains::get_composed_content),
        )
        .route(
            "/api/v1/chains/{chain_id}/variables",
            get(handlers::chains::get_chain_variables),
        )
        .route(
            "/api/v1/chains/by-prompt/{prompt_id}",
            get(handlers::chains::chains_for_prompt),
        )
        .route(
            "/api/v1/search/chains",
            post(handlers::chains::search_chains),
        )
        .route(
            "/api/v1/chains/bulk-update",
            post(handlers::chains::bulk_update),
        )
        .route("/api/v1/chains/count", get(handlers::chains::count_chains))
}

fn copy_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route(
            "/api/v1/prompts/{prompt_id}/copy-to-user",
            post(handlers::copy::copy_prompt_to_user),
        )
        .route(
            "/api/v1/scripts/{script_id}/copy-to-user",
            post(handlers::copy::copy_script_to_user),
        )
        .route(
            "/api/v1/chains/{chain_id}/copy-to-user",
            post(handlers::copy::copy_chain_to_user),
        )
        .route(
            "/api/v1/users/bulk-copy",
            post(handlers::copy::bulk_copy_all),
        )
}

fn taxonomy_routes() -> Router<Arc<AppState>> {
    Router::new()
        // Tags
        .route(
            "/api/v1/tags/user/{user_id}",
            get(handlers::tags::list_tags),
        )
        .route("/api/v1/tags", post(handlers::tags::create_tag))
        .route("/api/v1/tags/{tag_id}", put(handlers::tags::rename_tag))
        .route("/api/v1/tags/{tag_id}", delete(handlers::tags::delete_tag))
        // Collections
        .route(
            "/api/v1/collections/user/{user_id}",
            get(handlers::collections::list_collections),
        )
        .route(
            "/api/v1/collections",
            post(handlers::collections::create_collection),
        )
        .route(
            "/api/v1/collections/{collection_id}",
            put(handlers::collections::rename_collection),
        )
        .route(
            "/api/v1/collections/{collection_id}",
            delete(handlers::collections::delete_collection),
        )
        // Categories
        .route(
            "/api/v1/categories/user/{user_id}",
            get(handlers::categories::list_categories),
        )
        .route(
            "/api/v1/categories",
            post(handlers::categories::create_category),
        )
        .route(
            "/api/v1/categories/{category_id}",
            put(handlers::categories::rename_category),
        )
        .route(
            "/api/v1/categories/{category_id}",
            delete(handlers::categories::delete_category),
        )
}

fn version_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route(
            "/api/v1/versions/prompt/{prompt_id}",
            get(handlers::versions::list_versions),
        )
        .route(
            "/api/v1/versions/{version_id}",
            get(handlers::versions::get_version),
        )
        .route(
            "/api/v1/versions/prompt/{prompt_id}/restore",
            post(handlers::versions::restore_version),
        )
        .route(
            "/api/v1/versions/compare",
            get(handlers::versions::compare_versions),
        )
}

fn script_version_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route(
            "/api/v1/script-versions/script/{script_id}",
            get(handlers::script_versions::list_script_versions),
        )
        .route(
            "/api/v1/script-versions/{version_id}",
            get(handlers::script_versions::get_script_version),
        )
        .route(
            "/api/v1/script-versions/script/{script_id}/restore",
            post(handlers::script_versions::restore_script_version),
        )
        .route(
            "/api/v1/script-versions/compare",
            get(handlers::script_versions::compare_script_versions),
        )
}

fn data_routes() -> Router<Arc<AppState>> {
    Router::new()
        // Import/Export
        .route("/api/v1/io/export/json", post(handlers::io::export_json))
        .route("/api/v1/io/import/json", post(handlers::io::import_json))
        .route(
            "/api/v1/io/export/markdown",
            post(handlers::io::export_markdown),
        )
        .route(
            "/api/v1/io/import/markdown",
            post(handlers::io::import_markdown),
        )
        .route("/api/v1/io/backup", post(handlers::io::backup_database))
        // Clipboard
        .route(
            "/api/v1/clipboard/copy",
            post(handlers::clipboard::copy_to_clipboard),
        )
        .route(
            "/api/v1/clipboard/copy-substituted",
            post(handlers::clipboard::copy_with_substitution),
        )
        .route(
            "/api/v1/clipboard/history",
            get(handlers::clipboard::get_clipboard_history),
        )
        .route(
            "/api/v1/clipboard/history",
            delete(handlers::clipboard::clear_clipboard_history),
        )
        // Settings
        .route(
            "/api/v1/settings/db-path",
            get(handlers::health::get_db_path),
        )
        .route(
            "/api/v1/settings/app/{key}",
            get(handlers::settings::get_app_setting),
        )
        .route(
            "/api/v1/settings/app/{key}",
            put(handlers::settings::set_app_setting),
        )
        .route(
            "/api/v1/settings/user/{user_id}",
            get(handlers::settings::get_user_settings),
        )
        .route(
            "/api/v1/settings/user",
            put(handlers::settings::update_user_settings),
        )
        .route(
            "/api/v1/settings/user",
            patch(handlers::settings::patch_user_settings),
        )
        .route_layer(DefaultBodyLimit::max(10_485_760)) // 10 MiB for import endpoints
}

fn ollama_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route(
            "/api/v1/ollama/status",
            post(handlers::ollama::ollama_status),
        )
        .route(
            "/api/v1/ollama/improve",
            post(handlers::ollama::ollama_improve),
        )
        .route(
            "/api/v1/ollama/translate",
            post(handlers::ollama::ollama_translate),
        )
        .route(
            "/api/v1/ollama/autofill",
            post(handlers::ollama::ollama_autofill),
        )
}
