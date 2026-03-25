// =============================================================================
// Sync service: file-system synchronization for scripts.
//
// Provides two main operations:
// 1. `sync_scripts` -- reads source files for all synced scripts and updates
//    their content when the file has changed on disk.
// 2. `import_file` -- imports a file from the filesystem as a new script,
//    optionally setting it up for ongoing synchronization.
// =============================================================================

use std::path::Path;

use serde::Serialize;

use neuronprompter_core::CoreError;
use neuronprompter_core::domain::script::{NewScript, Script};
use neuronprompter_core::paths;
use neuronprompter_core::validation;
use neuronprompter_db::ConnectionProvider;
use neuronprompter_db::repo::{self, scripts};

use crate::ServiceError;
use crate::script_version_service;

/// Maximum file size allowed for import/sync (1 MB).
const MAX_FILE_SIZE: u64 = 1_048_576;

/// Summary of a sync operation across all synced scripts.
#[derive(Debug, Clone, Serialize)]
pub struct SyncReport {
    pub updated: usize,
    pub unchanged: usize,
    pub errors: Vec<SyncError>,
}

/// Details about a single script that failed to sync.
#[derive(Debug, Clone, Serialize)]
pub struct SyncError {
    pub script_id: i64,
    pub title: String,
    pub source_path: String,
    pub message: String,
}

/// Synchronizes all synced scripts for a user by reading their source files
/// from disk and updating content when changes are detected.
///
/// # Errors
///
/// Returns `ServiceError::Database` if the persistence layer fails when
/// fetching the list of synced scripts. Individual per-script errors are
/// collected in the returned `SyncReport::errors` rather than propagated.
pub fn sync_scripts(
    cp: &impl ConnectionProvider,
    user_id: i64,
) -> Result<SyncReport, ServiceError> {
    // Step 1: Fetch the list of synced scripts (DB read).
    let synced = cp.with_connection(|conn| scripts::list_synced_scripts(conn, user_id))?;

    let mut updated = 0usize;
    let mut unchanged = 0usize;
    let mut errors = Vec::new();

    for script in &synced {
        let source_path = if let Some(p) = &script.source_path {
            p.clone()
        } else {
            // Synced script without a source path -- skip silently.
            unchanged += 1;
            continue;
        };

        // Step 2: Read file from disk OUTSIDE any DB connection/transaction.
        let file_content = match read_source_file(&source_path) {
            Ok(content) => content,
            Err(msg) => {
                errors.push(SyncError {
                    script_id: script.id,
                    title: script.title.clone(),
                    source_path,
                    message: msg,
                });
                continue;
            }
        };

        // Step 3: If content unchanged, skip without touching the DB.
        if file_content == script.content {
            unchanged += 1;
            continue;
        }

        // Step 4: Content differs -- snapshot and update inside a transaction
        // using with_connection + savepoint so we nest safely inside any outer
        // transaction (fixes AUD-047).
        match cp.with_connection(|conn| {
            repo::with_savepoint(conn, "sync_single_script", |conn| {
                script_version_service::create_version_snapshot(conn, script)?;

                scripts::update_script_fields(
                    conn,
                    script.id,
                    None,
                    Some(&file_content),
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                )?;

                Ok(())
            })
        }) {
            Ok(()) => updated += 1,
            Err(e) => {
                errors.push(SyncError {
                    script_id: script.id,
                    title: script.title.clone(),
                    source_path,
                    message: format!("failed to update script: {e}"),
                });
            }
        }
    }

    Ok(SyncReport {
        updated,
        unchanged,
        errors,
    })
}

/// Reads and validates a source file from disk. Returns the file content as a
/// string, or an error message describing the failure.
fn read_source_file(source_path: &str) -> Result<String, String> {
    let path = Path::new(source_path);

    // Validate path is within the user's home directory.
    let home = paths::home_dir().ok_or_else(|| "cannot determine home directory".to_owned())?;
    paths::validate_path_within(path, &home).map_err(|e| format!("path validation failed: {e}"))?;

    if !path.exists() {
        return Err(format!("file not found: {source_path}"));
    }
    if !path.is_file() {
        return Err(format!("not a regular file: {source_path}"));
    }

    let metadata = std::fs::metadata(path).map_err(|e| format!("failed to read metadata: {e}"))?;
    if metadata.len() > MAX_FILE_SIZE {
        return Err(format!(
            "file too large ({} bytes, max {})",
            metadata.len(),
            MAX_FILE_SIZE
        ));
    }

    let file_content = std::fs::read_to_string(path)
        .map_err(|e| format!("failed to read file (invalid UTF-8?): {e}"))?;

    if file_content.contains('\0') {
        return Err("file contains null bytes".to_owned());
    }

    Ok(file_content)
}

/// Imports a file from the filesystem as a new script.
///
/// File I/O (reading, validation) is performed outside the database
/// transaction. Only the DB insert runs inside `with_transaction`.
///
/// # Errors
///
/// Returns `ServiceError::Core(Validation)` if the path is outside the home
/// directory, the file does not exist, is not a regular file, is too large,
/// is not valid UTF-8, contains null bytes, has empty content, or the
/// script language override is invalid.
/// Returns `ServiceError::IoError` if filesystem metadata or read operations fail.
/// Returns `ServiceError::Database` if the persistence layer fails.
pub fn import_file(
    cp: &impl ConnectionProvider,
    user_id: i64,
    path: &str,
    is_synced: bool,
    script_language_override: Option<&str>,
) -> Result<Script, ServiceError> {
    // --- File I/O outside the DB transaction ---
    let file_path = Path::new(path);

    // Validate path is within the user's home directory.
    let home = paths::home_dir().ok_or_else(|| CoreError::Validation {
        field: "path".to_owned(),
        message: "cannot determine home directory".to_owned(),
    })?;
    paths::validate_path_within(file_path, &home)?;

    if !file_path.exists() {
        return Err(CoreError::Validation {
            field: "path".to_owned(),
            message: format!("file not found: {path}"),
        }
        .into());
    }
    if !file_path.is_file() {
        return Err(CoreError::Validation {
            field: "path".to_owned(),
            message: format!("not a regular file: {path}"),
        }
        .into());
    }

    let metadata = std::fs::metadata(file_path)?;
    if metadata.len() > MAX_FILE_SIZE {
        return Err(CoreError::Validation {
            field: "path".to_owned(),
            message: format!(
                "file too large ({} bytes, max {})",
                metadata.len(),
                MAX_FILE_SIZE
            ),
        }
        .into());
    }

    let raw_bytes = std::fs::read(file_path)?;
    let content = String::from_utf8(raw_bytes).map_err(|_| CoreError::Validation {
        field: "path".to_owned(),
        message: format!("file is not valid UTF-8: {path}"),
    })?;

    // Reject empty or whitespace-only files.
    validation::validate_content(&content)?;

    if content.contains('\0') {
        return Err(CoreError::Validation {
            field: "path".to_owned(),
            message: "file contains null bytes".to_owned(),
        }
        .into());
    }

    let filename = file_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("untitled")
        .to_owned();

    let script_language = if let Some(override_lang) = script_language_override {
        validation::validate_script_language(override_lang)?;
        override_lang.to_owned()
    } else {
        detect_language(&filename)
    };

    let new = NewScript {
        user_id,
        title: filename,
        content,
        script_language,
        description: None,
        notes: None,
        language: None,
        source_path: if is_synced {
            Some(path.to_owned())
        } else {
            None
        },
        is_synced,
        tag_ids: Vec::new(),
        category_ids: Vec::new(),
        collection_ids: Vec::new(),
    };

    // --- DB insert inside a transaction ---
    Ok(cp.with_transaction(|conn| scripts::create_script(conn, &new))?)
}

/// Detects the script language from a filename's extension or special name.
fn detect_language(filename: &str) -> String {
    // Check special filenames first.
    let base = Path::new(filename)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(filename);
    match base {
        "Dockerfile" | "dockerfile" => return "dockerfile".to_owned(),
        "Makefile" | "makefile" | "GNUmakefile" => return "makefile".to_owned(),
        "Vagrantfile" | "Rakefile" | "Gemfile" => return "ruby".to_owned(),
        "Jenkinsfile" => return "groovy".to_owned(),
        "CMakeLists.txt" => return "cmake".to_owned(),
        _ => {}
    }

    let ext = Path::new(filename)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    match ext.as_str() {
        "py" => "python",
        "js" => "javascript",
        "ts" | "tsx" => "typescript",
        "sh" => "bash",
        "rs" => "rust",
        "go" => "go",
        "rb" => "ruby",
        "java" => "java",
        "c" | "h" => "c",
        "cpp" | "cc" | "hpp" => "cpp",
        "cs" => "csharp",
        "swift" => "swift",
        "kt" | "kts" => "kotlin",
        "lua" => "lua",
        "php" => "php",
        "sql" => "sql",
        "html" | "htm" => "html",
        "css" => "css",
        "yaml" | "yml" => "yaml",
        "json" => "json",
        "xml" => "xml",
        "toml" => "toml",
        "md" | "markdown" => "markdown",
        "zsh" => "zsh",
        "fish" => "fish",
        "ps1" => "powershell",
        "bat" | "cmd" => "batch",
        _ => "text",
    }
    .to_owned()
}
