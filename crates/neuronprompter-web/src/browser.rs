// =============================================================================
// Cross-platform browser launcher.
//
// Opens the default system browser to the given URL using the `opener` crate.
// Errors are logged but do not prevent the server from starting.
// =============================================================================

/// Attempts to open the given URL in the user's default browser.
///
/// Logs a warning if the browser cannot be opened but does not return an error,
/// since the server should continue running regardless.
pub fn launch_browser(url: &str) {
    if let Err(e) = opener::open(url) {
        tracing::warn!(
            url = url,
            error = %e,
            "failed to open default browser; please navigate manually"
        );
    }
}
