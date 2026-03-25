// =============================================================================
// Embedded frontend asset serving with SPA fallback routing.
//
// The compiled SolidJS frontend from `frontend/dist/` is baked into the binary
// via `rust-embed` and served with correct MIME types. Any request that does
// not match a static file falls back to `index.html` for client-side routing.
// =============================================================================

use axum::http::{StatusCode, header};
use axum::response::{Html, IntoResponse, Response};

#[derive(rust_embed::Embed)]
#[folder = "frontend/dist"]
struct FrontendAssets;

/// Serves embedded frontend assets or falls back to `index.html` for SPA routing.
pub async fn serve_embedded(uri: axum::http::Uri) -> Response {
    let path = uri.path().trim_start_matches('/');

    // Try to serve the exact file path.
    if let Some(file) = FrontendAssets::get(path) {
        let mime = mime_guess::from_path(path).first_or_octet_stream();

        // Vite hashed assets get long-lived cache headers.
        let cache_control = if path.contains("/assets/") {
            "public, max-age=31536000, immutable"
        } else {
            "no-cache"
        };

        (
            StatusCode::OK,
            [
                (header::CONTENT_TYPE, mime.as_ref().to_owned()),
                (header::CACHE_CONTROL, cache_control.to_owned()),
            ],
            file.data.to_vec(),
        )
            .into_response()
    } else if let Some(index) = FrontendAssets::get("index.html") {
        // SPA fallback: serve index.html for client-side routing.
        // Uses String::from_utf8 instead of from_utf8_lossy to detect encoding
        // corruption rather than silently replacing invalid bytes with the
        // Unicode replacement character.
        let html = String::from_utf8(index.data.to_vec()).unwrap_or_else(|_| {
            "<!DOCTYPE html><html><body><h1>Frontend encoding error</h1></body></html>".to_owned()
        });
        Html(html).into_response()
    } else {
        (StatusCode::NOT_FOUND, "frontend not found").into_response()
    }
}
