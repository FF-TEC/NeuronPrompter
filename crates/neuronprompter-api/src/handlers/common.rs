// =============================================================================
// Shared handler utilities.
// =============================================================================

use std::path::Path;

use neuronprompter_core::paths;

use crate::error::ApiError;

/// Validates that the given path is within the user's home directory.
/// Rejects paths outside the home directory to prevent unauthorized
/// filesystem access through import/export operations.
///
/// # Errors
///
/// Returns `ApiError` with HTTP 500 if the home directory cannot be determined.
/// Returns `ApiError` with HTTP 400 if the path is outside the home directory.
pub fn validate_io_path(path: &Path) -> Result<(), ApiError> {
    let home = paths::home_dir()
        .ok_or_else(|| ApiError::internal("unable to determine home directory".to_owned()))?;
    paths::validate_path_within(path, &home).map_err(ApiError::from)?;
    Ok(())
}
