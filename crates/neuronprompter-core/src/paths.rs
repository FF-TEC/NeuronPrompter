// =============================================================================
// Centralized storage path resolution for the NeuronPrompter application.
//
// All persistent artifacts (SQLite database, backups, exports) are stored
// under a platform-specific root directory:
//
// - Windows:  %APPDATA%\NeuronPrompter  (fallback: Documents\NeuronPrompter)
// - Linux:    $XDG_DATA_HOME/NeuronPrompter  (fallback: ~/Documents/NeuronPrompter)
// - macOS:    ~/Documents/NeuronPrompter
//
// ```text
// <root>/NeuronPrompter/
// +-- neuronprompter.db        Main SQLite database
// +-- backups/                 Database backups
// ```
//
// This module is the single source of truth for all storage locations. Every
// other crate in the workspace calls these functions instead of computing
// paths independently.
// =============================================================================

use std::path::{Path, PathBuf};

/// Returns the root directory for all NeuronPrompter persistent data.
///
/// Resolution order:
///
/// - **Windows**: `%APPDATA%\NeuronPrompter`, then `%USERPROFILE%\Documents\NeuronPrompter`
/// - **Linux**: `$XDG_DATA_HOME/NeuronPrompter`, then `$HOME/Documents/NeuronPrompter`
/// - **macOS**: `$HOME/Documents/NeuronPrompter`
///
/// Falls back to `./NeuronPrompter` relative to the current working directory
/// if no environment variable is set.
#[must_use]
pub fn base_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        if let Ok(appdata) = std::env::var("APPDATA") {
            return PathBuf::from(appdata).join("NeuronPrompter");
        }
        if let Ok(profile) = std::env::var("USERPROFILE") {
            return PathBuf::from(profile)
                .join("Documents")
                .join("NeuronPrompter");
        }
    }

    #[cfg(target_os = "linux")]
    {
        if let Ok(xdg) = std::env::var("XDG_DATA_HOME") {
            return PathBuf::from(xdg).join("NeuronPrompter");
        }
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home).join("Documents").join("NeuronPrompter");
        }
    }

    #[cfg(target_os = "macos")]
    {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home).join("Documents").join("NeuronPrompter");
        }
    }

    PathBuf::from("NeuronPrompter")
}

/// Returns the path to the main SQLite database file.
///
/// Path: `<base_dir>/neuronprompter.db`
#[must_use]
pub fn db_path() -> PathBuf {
    base_dir().join("neuronprompter.db")
}

/// Returns the directory for database backups.
///
/// Path: `<base_dir>/backups/`
#[must_use]
pub fn backups_dir() -> PathBuf {
    base_dir().join("backups")
}

/// Ensures the base directory exists, creating it and all parent directories
/// if needed. Returns the base directory path on success.
///
/// # Errors
///
/// Returns `std::io::Error` if directory creation fails (e.g. permission denied).
pub fn ensure_base_dir() -> Result<PathBuf, std::io::Error> {
    let path = base_dir();
    std::fs::create_dir_all(&path)?;
    Ok(path)
}

/// Returns the path to the `.setup_complete` marker file. Its presence
/// indicates that the first-run welcome flow has been completed.
///
/// Path: `<base_dir>/.setup_complete`
#[must_use]
pub fn setup_complete_path() -> PathBuf {
    base_dir().join(".setup_complete")
}

/// Ensures the base directory exists and returns the database path.
/// Creates the base directory if it does not exist.
///
/// # Errors
///
/// Returns `std::io::Error` if directory creation fails.
pub fn ensure_db_path() -> Result<PathBuf, std::io::Error> {
    ensure_base_dir()?;
    Ok(db_path())
}

/// Returns the user's home directory, if detectable from environment variables.
///
/// - **Windows**: `%USERPROFILE%`
/// - **Unix** (macOS, Linux): `$HOME`
#[must_use]
pub fn home_dir() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        if let Ok(profile) = std::env::var("USERPROFILE") {
            return Some(PathBuf::from(profile));
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        if let Ok(home) = std::env::var("HOME") {
            return Some(PathBuf::from(home));
        }
    }
    None
}

/// Strips the Windows `\\?\` extended-length prefix from a `PathBuf`. This
/// prefix is an internal Windows API detail added by `std::fs::canonicalize`
/// and must not appear in user-facing output or database-stored paths, where
/// it would break equality comparisons with paths stored without the prefix.
///
/// Returns the input unchanged when the prefix is absent or on non-Windows
/// platforms.
fn strip_extended_length_prefix_path(path: &Path) -> PathBuf {
    let s = path.to_string_lossy();
    if let Some(stripped) = s.strip_prefix(r"\\?\") {
        PathBuf::from(stripped)
    } else {
        path.to_path_buf()
    }
}

/// Strips the Windows `\\?\` extended-length prefix from a path string.
/// Returns the input unchanged when the prefix is absent.
#[must_use]
pub fn strip_extended_length_prefix(path: &str) -> &str {
    path.strip_prefix(r"\\?\").unwrap_or(path)
}

/// Validates that `path` resolves to a location within `allowed_dir`.
///
/// Both paths are canonicalized (resolving symlinks, `..`, `.`). For paths
/// that do not yet exist (e.g. export targets), the parent directory is
/// canonicalized and the filename is joined.
///
/// # Errors
///
/// Returns `CoreError::PathTraversal` if the path escapes `allowed_dir`.
pub fn validate_path_within(
    path: &std::path::Path,
    allowed_dir: &std::path::Path,
) -> Result<PathBuf, crate::CoreError> {
    let canonical_allowed =
        strip_extended_length_prefix_path(&std::fs::canonicalize(allowed_dir).map_err(|_| {
            crate::CoreError::PathTraversal {
                path: allowed_dir.display().to_string(),
            }
        })?);

    // Try to canonicalize the path directly (works if it exists).
    let canonical = if path.exists() {
        strip_extended_length_prefix_path(&std::fs::canonicalize(path).map_err(|_| {
            crate::CoreError::PathTraversal {
                path: path.display().to_string(),
            }
        })?)
    } else {
        // For non-existent paths (e.g. export targets), canonicalize the parent
        // and join the filename.
        let parent = path
            .parent()
            .ok_or_else(|| crate::CoreError::PathTraversal {
                path: path.display().to_string(),
            })?;
        let filename = path
            .file_name()
            .ok_or_else(|| crate::CoreError::PathTraversal {
                path: path.display().to_string(),
            })?;
        let canonical_parent =
            strip_extended_length_prefix_path(&std::fs::canonicalize(parent).map_err(|_| {
                crate::CoreError::PathTraversal {
                    path: path.display().to_string(),
                }
            })?);
        canonical_parent.join(filename)
    };

    if canonical.starts_with(&canonical_allowed) {
        Ok(canonical)
    } else {
        Err(crate::CoreError::PathTraversal {
            path: path.display().to_string(),
        })
    }
}

/// Convenience: validates that `path` is within [`base_dir()`].
///
/// # Errors
///
/// Returns `CoreError::PathTraversal` if the path escapes the base directory.
pub fn validate_path(path: &std::path::Path) -> Result<PathBuf, crate::CoreError> {
    validate_path_within(path, &base_dir())
}

/// Sanitizes a user-provided path by canonicalizing it and rejecting null
/// bytes. Unlike [`validate_path`] / [`validate_path_within`], this does
/// not restrict the path to a specific directory -- it only resolves symlinks
/// and `..` components to produce a clean canonical path.
///
/// Use this for operations where the user explicitly chooses a file path
/// through a file picker (export/import), as opposed to operations where
/// the application constructs the path internally.
///
/// # Errors
///
/// Returns `CoreError::PathTraversal` if the path cannot be canonicalized
/// (e.g. it contains null bytes or refers to non-existent parent directories).
pub fn sanitize_path(path: &std::path::Path) -> Result<PathBuf, crate::CoreError> {
    // Reject null bytes in path string.
    if path.to_string_lossy().contains('\0') {
        return Err(crate::CoreError::PathTraversal {
            path: path.display().to_string(),
        });
    }

    // Reject parent directory traversal components.
    for component in path.components() {
        if matches!(component, std::path::Component::ParentDir) {
            return Err(crate::CoreError::PathTraversal {
                path: path.display().to_string(),
            });
        }
    }

    if path.exists() {
        Ok(strip_extended_length_prefix_path(
            &std::fs::canonicalize(path).map_err(|_| crate::CoreError::PathTraversal {
                path: path.display().to_string(),
            })?,
        ))
    } else {
        // For non-existent paths, canonicalize parent + join filename.
        let parent = path
            .parent()
            .ok_or_else(|| crate::CoreError::PathTraversal {
                path: path.display().to_string(),
            })?;
        let filename = path
            .file_name()
            .ok_or_else(|| crate::CoreError::PathTraversal {
                path: path.display().to_string(),
            })?;
        let canonical_parent =
            strip_extended_length_prefix_path(&std::fs::canonicalize(parent).map_err(|_| {
                crate::CoreError::PathTraversal {
                    path: path.display().to_string(),
                }
            })?);
        Ok(canonical_parent.join(filename))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn base_dir_contains_neuronprompter() {
        let dir = base_dir();
        let s = dir.to_string_lossy();
        assert!(
            s.contains("NeuronPrompter"),
            "base_dir must contain 'NeuronPrompter', got: {s}"
        );
    }

    #[test]
    fn db_path_under_base_dir() {
        assert!(db_path().starts_with(base_dir()));
    }

    #[test]
    fn db_path_has_correct_filename() {
        let p = db_path();
        assert_eq!(
            p.file_name().unwrap().to_str().unwrap(),
            "neuronprompter.db"
        );
    }

    #[test]
    fn backups_dir_under_base_dir() {
        assert!(backups_dir().starts_with(base_dir()));
    }

    #[test]
    fn ensure_base_dir_creates_directory() {
        let path = ensure_base_dir().expect("ensure_base_dir must succeed");
        assert!(path.is_dir(), "base_dir must exist after ensure_base_dir");
    }

    #[test]
    fn ensure_db_path_returns_path_under_base() {
        let p = ensure_db_path().expect("ensure_db_path must succeed");
        assert!(p.starts_with(base_dir()));
        assert_eq!(
            p.file_name().unwrap().to_str().unwrap(),
            "neuronprompter.db"
        );
    }

    // -----------------------------------------------------------------------
    // sanitize_path rejects `..` components
    // -----------------------------------------------------------------------

    #[test]
    fn sanitize_path_rejects_parent_dir_traversal() {
        let p = std::path::Path::new("/tmp/../etc/passwd");
        assert!(sanitize_path(p).is_err());
    }

    #[test]
    fn sanitize_path_rejects_double_parent_traversal() {
        let p = std::path::Path::new("/tmp/../../etc/shadow");
        assert!(sanitize_path(p).is_err());
    }

    #[test]
    fn sanitize_path_rejects_relative_parent_traversal() {
        let p = std::path::Path::new("../../etc/passwd");
        assert!(sanitize_path(p).is_err());
    }

    #[test]
    fn sanitize_path_rejects_null_bytes() {
        let p = std::path::Path::new("/tmp/file\0.txt");
        assert!(sanitize_path(p).is_err());
    }

    #[test]
    fn sanitize_path_allows_current_dir_component() {
        // "." components are fine, only ".." is dangerous.
        // Use temp_dir to get a path that exists on all platforms.
        let base = std::env::temp_dir();
        let p = base.join(".");
        // This may or may not succeed depending on canonicalization, but
        // it must NOT fail with PathTraversal.
        let result = sanitize_path(&p);
        if let Err(e) = &result {
            assert!(
                !matches!(e, crate::CoreError::PathTraversal { .. }),
                "current-dir path should not trigger PathTraversal"
            );
        }
    }

    // -----------------------------------------------------------------------
    // Windows extended-length prefix stripping
    // -----------------------------------------------------------------------

    #[test]
    fn strip_extended_length_prefix_removes_prefix() {
        assert_eq!(
            strip_extended_length_prefix(r"\\?\D:\AppData\NeuronPrompter"),
            r"D:\AppData\NeuronPrompter"
        );
    }

    #[test]
    fn strip_extended_length_prefix_returns_unchanged_without_prefix() {
        assert_eq!(
            strip_extended_length_prefix(r"D:\AppData\NeuronPrompter"),
            r"D:\AppData\NeuronPrompter"
        );
        assert_eq!(
            strip_extended_length_prefix("/home/user/docs"),
            "/home/user/docs"
        );
    }

    #[test]
    fn strip_extended_length_prefix_path_removes_prefix() {
        let path = Path::new(r"\\?\C:\Users\test\file.txt");
        let result = strip_extended_length_prefix_path(path);
        let s = result.to_string_lossy();
        assert!(
            !s.starts_with(r"\\?\"),
            "stripped path must not have \\\\?\\ prefix: {s}"
        );
    }

    #[test]
    fn validate_path_within_returns_path_without_prefix() {
        let tmp = tempfile::tempdir().expect("create temp dir");
        let file = tmp.path().join("test.txt");
        std::fs::write(&file, "content").expect("write test file");

        let result = validate_path_within(&file, tmp.path())
            .expect("validation must succeed for file within allowed dir");
        let s = result.to_string_lossy();
        assert!(
            !s.starts_with(r"\\?\"),
            "validated path must not have \\\\?\\ prefix: {s}"
        );
    }

    #[test]
    fn sanitize_path_returns_path_without_prefix() {
        let tmp = tempfile::tempdir().expect("create temp dir");
        let file = tmp.path().join("test.txt");
        std::fs::write(&file, "content").expect("write test file");

        let result = sanitize_path(&file).expect("sanitize must succeed for existing file");
        let s = result.to_string_lossy();
        assert!(
            !s.starts_with(r"\\?\"),
            "sanitized path must not have \\\\?\\ prefix: {s}"
        );
    }
}
