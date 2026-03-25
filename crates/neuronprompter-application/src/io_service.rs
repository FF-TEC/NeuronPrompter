// =============================================================================
// I/O service: JSON and Markdown import/export, database backup.
//
// Handles serialization of prompts with their associations and versions into
// JSON export envelopes, deserialization on import with automatic creation of
// missing tags/categories/collections, Markdown bundle export with YAML
// front-matter, and filesystem-level database backup via file copy.
// =============================================================================

use std::io::Read;
use std::path::Path;

use serde::{Deserialize, Serialize};

use neuronprompter_core::domain::prompt::{NewPrompt, PromptWithAssociations};
use neuronprompter_core::domain::version::PromptVersion;
use neuronprompter_core::paths;
use neuronprompter_core::validation;
use neuronprompter_db::ConnectionProvider;
use neuronprompter_db::repo::{categories, collections, prompts, tags, versions};

use crate::ServiceError;

/// Maximum allowed size (in bytes) for a single import file (50 MiB).
const MAX_IMPORT_SIZE: u64 = 50 * 1024 * 1024;

/// Maximum number of Markdown files allowed in a single import directory.
const MAX_IMPORT_FILES: usize = 10_000;

// ---------------------------------------------------------------------------
// JSON export/import data structures
// ---------------------------------------------------------------------------

/// Metadata identifying the user who performed the export.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportedBy {
    pub username: String,
    pub display_name: String,
}

/// Top-level envelope wrapping exported prompts with provenance metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportEnvelope {
    pub exported_by: ExportedBy,
    pub exported_at: String,
    pub prompts: Vec<ExportedPrompt>,
}

/// A single prompt with its full data graph for export.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportedPrompt {
    pub title: String,
    pub content: String,
    pub description: Option<String>,
    pub notes: Option<String>,
    pub language: Option<String>,
    pub is_favorite: bool,
    pub tags: Vec<String>,
    pub categories: Vec<String>,
    pub collections: Vec<String>,
    pub versions: Vec<ExportedVersion>,
}

/// A version snapshot within an exported prompt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportedVersion {
    pub version_number: i64,
    pub title: String,
    pub content: String,
    pub description: Option<String>,
    pub notes: Option<String>,
    pub language: Option<String>,
    pub created_at: String,
}

/// Summary returned after an import operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportSummary {
    pub source_user: Option<ExportedBy>,
    pub prompts_imported: usize,
    pub tags_created: usize,
    pub categories_created: usize,
    pub collections_created: usize,
}

// ---------------------------------------------------------------------------
// Markdown front-matter structures
// ---------------------------------------------------------------------------

/// YAML front-matter embedded in Markdown export files.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarkdownFrontMatter {
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub is_favorite: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[serde(default)]
    pub collections: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

/// Helper for `#[serde(skip_serializing_if)]` on bool fields.
#[allow(clippy::trivially_copy_pass_by_ref)] // serde skip_serializing_if requires &bool
fn is_false(v: &bool) -> bool {
    !*v
}

/// Manifest file written alongside Markdown bundle exports.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleManifest {
    pub exported_by: ExportedBy,
    pub exported_at: String,
}

// ---------------------------------------------------------------------------
// JSON export
// ---------------------------------------------------------------------------

/// Exports the specified prompts as a JSON file containing an `ExportEnvelope`.
/// Each prompt is serialized with its tags, categories, collections, and
/// version history.
///
/// # Errors
///
/// Returns `ServiceError::Core(NotFound)` if a prompt does not exist or does
/// not belong to the exporting user.
/// Returns `ServiceError::Database` if the persistence layer fails.
/// Returns `ServiceError::SerializationError` if JSON serialization fails.
/// Returns `ServiceError::IoError` if writing the file to disk fails.
pub fn export_json(
    cp: &impl ConnectionProvider,
    user_id: i64,
    prompt_ids: &[i64],
    path: &Path,
) -> Result<(), ServiceError> {
    let path = &paths::sanitize_path(path)?;
    let envelope = cp.with_connection(|conn| {
        let user = neuronprompter_db::repo::users::get_user(conn, user_id)?;

        let mut exported_prompts = Vec::with_capacity(prompt_ids.len());
        // Performance note: Each prompt is loaded individually with its associations and version
        // history. For large exports, this results in 5N queries (N prompts x (1 prompt + 3
        // associations + 1 versions)). A batch-loading approach with WHERE id IN (...) queries
        // would be more efficient but requires additional repository functions. This is tracked
        // as a future optimization.
        for &pid in prompt_ids {
            let pwa = prompts::get_prompt_with_associations(conn, pid)?;
            // F2: Reject prompts that do not belong to the exporting user.
            if pwa.prompt.user_id != user_id {
                return Err(neuronprompter_db::DbError::Core(
                    neuronprompter_core::CoreError::NotFound {
                        entity: "Prompt".to_owned(),
                        id: pid,
                    },
                ));
            }
            let vers = versions::list_all_versions_for_prompt(conn, pid)?;
            exported_prompts.push(build_exported_prompt(&pwa, &vers));
        }

        Ok(ExportEnvelope {
            exported_by: ExportedBy {
                username: user.username,
                display_name: user.display_name,
            },
            exported_at: chrono::Utc::now().to_rfc3339(),
            prompts: exported_prompts,
        })
    })?;

    let json = serde_json::to_string_pretty(&envelope)
        .map_err(|e| ServiceError::SerializationError(e.to_string()))?;
    std::fs::write(path, json)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// JSON import
// ---------------------------------------------------------------------------

/// Imports prompts from a JSON export file into the specified user's account.
/// Missing tags, categories, and collections are created automatically.
///
/// # Errors
///
/// Returns `ServiceError::IoError` if the file cannot be opened, exceeds the
/// maximum import size, or cannot be read.
/// Returns `ServiceError::SerializationError` if JSON deserialization fails.
/// Returns `ServiceError::Core(Validation)` if any imported prompt field fails
/// validation, or if duplicate version numbers are detected.
/// Returns `ServiceError::Database` if the persistence layer fails.
pub fn import_json(
    cp: &impl ConnectionProvider,
    user_id: i64,
    path: &Path,
) -> Result<ImportSummary, ServiceError> {
    let path = &paths::sanitize_path(path)?;
    let file = std::fs::File::open(path)?;
    let file_len = file.metadata()?.len();
    if file_len > MAX_IMPORT_SIZE {
        return Err(ServiceError::IoError(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("import file exceeds maximum allowed size of {MAX_IMPORT_SIZE} bytes"),
        )));
    }
    // Read with a hard cap to guard against files growing between stat and read.
    #[allow(clippy::cast_possible_truncation)]
    // file_len <= MAX_IMPORT_SIZE (50 MiB), fits in usize
    let mut data = String::with_capacity(file_len as usize);
    file.take(MAX_IMPORT_SIZE + 1).read_to_string(&mut data)?;
    if data.len() as u64 > MAX_IMPORT_SIZE {
        return Err(ServiceError::IoError(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("import file exceeds maximum allowed size of {MAX_IMPORT_SIZE} bytes"),
        )));
    }
    let envelope: ExportEnvelope =
        serde_json::from_str(&data).map_err(|e| ServiceError::SerializationError(e.to_string()))?;

    let mut tags_created: usize = 0;
    let mut categories_created: usize = 0;
    let mut collections_created: usize = 0;
    let prompts_imported = envelope.prompts.len();

    cp.with_transaction(|conn| {
        for ep in &envelope.prompts {
            // Validate imported prompt fields before persisting.
            validation::validate_title(&ep.title).map_err(neuronprompter_db::DbError::Core)?;
            validation::validate_content(&ep.content).map_err(neuronprompter_db::DbError::Core)?;
            validation::validate_content_size(&ep.content, validation::MAX_CONTENT_BYTES)
                .map_err(neuronprompter_db::DbError::Core)?;
            validation::validate_language_code(ep.language.as_deref())
                .map_err(neuronprompter_db::DbError::Core)?;

            let tag_ids = resolve_tags(conn, user_id, &ep.tags, &mut tags_created)?;
            let cat_ids =
                resolve_categories(conn, user_id, &ep.categories, &mut categories_created)?;
            let col_ids =
                resolve_collections(conn, user_id, &ep.collections, &mut collections_created)?;

            let new = NewPrompt {
                user_id,
                title: ep.title.clone(),
                content: ep.content.clone(),
                description: ep.description.clone(),
                notes: ep.notes.clone(),
                language: ep.language.clone(),
                tag_ids,
                category_ids: cat_ids,
                collection_ids: col_ids,
            };
            let prompt = prompts::create_prompt(conn, &new)?;

            // Set favorite flag if the exported prompt was favorited.
            if ep.is_favorite {
                prompts::set_favorite(conn, prompt.id, true)?;
            }

            // F8: Validate version numbers are unique before inserting.
            {
                let mut seen_versions = std::collections::HashSet::new();
                for ev in &ep.versions {
                    if !seen_versions.insert(ev.version_number) {
                        return Err(neuronprompter_db::DbError::Core(
                            neuronprompter_core::CoreError::Validation {
                                field: "version_number".to_owned(),
                                message: format!(
                                    "Duplicate version number {} in import data",
                                    ev.version_number
                                ),
                            },
                        ));
                    }
                }
            }

            // Import version history.
            for ev in &ep.versions {
                versions::insert_version(
                    conn,
                    prompt.id,
                    ev.version_number,
                    &ev.title,
                    &ev.content,
                    ev.description.as_deref(),
                    ev.notes.as_deref(),
                    ev.language.as_deref(),
                )?;
            }

            // F8: Sync current_version to the highest imported version number.
            if let Some(max_ver) = ep.versions.iter().map(|v| v.version_number).max() {
                if max_ver > 1 {
                    prompts::update_current_version(conn, prompt.id, max_ver)?;
                }
            }
        }
        Ok(())
    })?;

    Ok(ImportSummary {
        source_user: Some(envelope.exported_by),
        prompts_imported,
        tags_created,
        categories_created,
        collections_created,
    })
}

// ---------------------------------------------------------------------------
// Markdown bundle export
// ---------------------------------------------------------------------------

/// Exports prompts as a directory of Markdown files with YAML front-matter
/// and a manifest.json file containing export provenance.
///
/// Reads all prompt data from the database first, then writes files outside
/// the database closure to avoid mixing `DbError` and `io::Error` return types.
///
/// # Errors
///
/// Returns `ServiceError::Core(NotFound)` if a prompt does not exist or does
/// not belong to the exporting user.
/// Returns `ServiceError::Database` if the persistence layer fails.
/// Returns `ServiceError::SerializationError` if JSON or YAML serialization fails.
/// Returns `ServiceError::IoError` if writing files to disk fails.
pub fn export_markdown(
    cp: &impl ConnectionProvider,
    user_id: i64,
    prompt_ids: &[i64],
    dir_path: &Path,
) -> Result<(), ServiceError> {
    let dir_path = &paths::sanitize_path(dir_path)?;
    // Phase 1: Read all data from the database.
    let (user, prompt_data) = cp.with_connection(|conn| {
        let user = neuronprompter_db::repo::users::get_user(conn, user_id)?;
        let mut data = Vec::with_capacity(prompt_ids.len());
        for &pid in prompt_ids {
            let pwa = prompts::get_prompt_with_associations(conn, pid)?;
            // F2: Reject prompts that do not belong to the exporting user.
            if pwa.prompt.user_id != user_id {
                return Err(neuronprompter_db::DbError::Core(
                    neuronprompter_core::CoreError::NotFound {
                        entity: "Prompt".to_owned(),
                        id: pid,
                    },
                ));
            }
            data.push(pwa);
        }
        Ok((user, data))
    })?;

    // Phase 2: Write files to disk outside the database closure.
    std::fs::create_dir_all(dir_path)?;

    let manifest = BundleManifest {
        exported_by: ExportedBy {
            username: user.username,
            display_name: user.display_name,
        },
        exported_at: chrono::Utc::now().to_rfc3339(),
    };
    let manifest_json = serde_json::to_string_pretty(&manifest)
        .map_err(|e| ServiceError::SerializationError(e.to_string()))?;
    std::fs::write(dir_path.join("manifest.json"), manifest_json)?;

    for (idx, pwa) in prompt_data.iter().enumerate() {
        let front = MarkdownFrontMatter {
            title: pwa.prompt.title.clone(),
            description: pwa.prompt.description.clone(),
            language: pwa.prompt.language.clone(),
            is_favorite: pwa.prompt.is_favorite,
            tags: pwa.tags.iter().map(|t| t.name.clone()).collect(),
            categories: pwa.categories.iter().map(|c| c.name.clone()).collect(),
            collections: pwa.collections.iter().map(|c| c.name.clone()).collect(),
            notes: pwa.prompt.notes.clone(),
        };
        // serde_yml replaces the deprecated serde_yaml crate for YAML serialization.
        let yaml = serde_yml::to_string(&front)
            .map_err(|e| ServiceError::SerializationError(e.to_string()))?;

        let md_content = format!("---\n{yaml}---\n\n{}", pwa.prompt.content);
        let filename = format!("prompt_{:03}.md", idx + 1);
        std::fs::write(dir_path.join(filename), md_content)?;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Markdown bundle import
// ---------------------------------------------------------------------------

/// Imports prompts from a directory of Markdown files with YAML front-matter.
/// Each .md file is parsed for front-matter metadata and body content.
///
/// Reads and parses all Markdown files first, then inserts into the database
/// inside a single transaction to avoid mixing `io::Error` with `DbError`.
///
/// Each file is checked against `MAX_IMPORT_SIZE` before reading. The parsed
/// front-matter fields are validated (title, content, content size, language)
/// before database insertion.
///
/// # Errors
///
/// Returns `ServiceError::IoError` if the directory cannot be read, a file
/// exceeds the maximum import size, the file count exceeds the limit, or a
/// file cannot be read.
/// Returns `ServiceError::SerializationError` if YAML front-matter parsing fails
/// or a Markdown file is missing delimiters.
/// Returns `ServiceError::Core(Validation)` if any parsed field fails validation.
/// Returns `ServiceError::Database` if the persistence layer fails.
pub fn import_markdown(
    cp: &impl ConnectionProvider,
    user_id: i64,
    dir_path: &Path,
) -> Result<ImportSummary, ServiceError> {
    let dir_path = &paths::sanitize_path(dir_path)?;
    // Phase 1: Read and parse all Markdown files from disk.
    let mut md_files: Vec<_> = std::fs::read_dir(dir_path)?
        .filter_map(|entry| match entry {
            Ok(e) => Some(e),
            Err(e) => {
                tracing::warn!(error = %e, "skipping unreadable directory entry");
                None
            }
        })
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "md"))
        .map(|entry| entry.path())
        .collect();
    md_files.sort();

    if md_files.len() > MAX_IMPORT_FILES {
        return Err(ServiceError::IoError(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!(
                "import directory contains {} files, exceeding maximum of {}",
                md_files.len(),
                MAX_IMPORT_FILES
            ),
        )));
    }

    let mut parsed: Vec<(MarkdownFrontMatter, String)> = Vec::with_capacity(md_files.len());
    for md_path in &md_files {
        // M-09: Check file size before reading to prevent excessive memory usage.
        let file_meta = std::fs::metadata(md_path).map_err(ServiceError::IoError)?;
        if file_meta.len() > MAX_IMPORT_SIZE {
            return Err(ServiceError::IoError(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Markdown file exceeds maximum allowed size of {MAX_IMPORT_SIZE} bytes"),
            )));
        }

        let raw = std::fs::read_to_string(md_path)?;
        let (front, content) = parse_markdown_frontmatter(&raw)?;
        parsed.push((front, content));
    }

    // Phase 2: Insert all parsed data into the database in a transaction.
    let mut tags_created: usize = 0;
    let mut categories_created: usize = 0;
    let mut collections_created: usize = 0;
    let prompts_imported = parsed.len();

    cp.with_transaction(|conn| {
        for (front, body) in &parsed {
            // M-08: Validate parsed front-matter fields before database insertion.
            validation::validate_title(&front.title).map_err(neuronprompter_db::DbError::Core)?;
            validation::validate_content(body).map_err(neuronprompter_db::DbError::Core)?;
            validation::validate_content_size(body, validation::MAX_CONTENT_BYTES)
                .map_err(neuronprompter_db::DbError::Core)?;
            if let Some(ref lang) = front.language {
                validation::validate_language_code(Some(lang.as_str()))
                    .map_err(neuronprompter_db::DbError::Core)?;
            }

            let tag_ids = resolve_tags(conn, user_id, &front.tags, &mut tags_created)?;
            let cat_ids =
                resolve_categories(conn, user_id, &front.categories, &mut categories_created)?;
            let col_ids =
                resolve_collections(conn, user_id, &front.collections, &mut collections_created)?;

            let new = NewPrompt {
                user_id,
                title: front.title.clone(),
                content: body.clone(),
                description: front.description.clone(),
                notes: front.notes.clone(),
                language: front.language.clone(),
                tag_ids,
                category_ids: cat_ids,
                collection_ids: col_ids,
            };
            let prompt = prompts::create_prompt(conn, &new)?;

            if front.is_favorite {
                prompts::set_favorite(conn, prompt.id, true)?;
            }
        }
        Ok(())
    })?;

    Ok(ImportSummary {
        source_user: None,
        prompts_imported,
        tags_created,
        categories_created,
        collections_created,
    })
}

// ---------------------------------------------------------------------------
// Database backup
// ---------------------------------------------------------------------------

/// Creates a consistent backup of the database using the SQLite backup API.
/// This is WAL-safe and avoids copying a potentially inconsistent database file.
///
/// # Errors
///
/// Returns `ServiceError::IoError` if the source or target path fails
/// sanitization, the SQLite connections cannot be opened, or the backup
/// operation fails.
pub fn backup_database(source_path: &Path, target_path: &Path) -> Result<(), ServiceError> {
    let source_path = &paths::sanitize_path(source_path)?;
    let target_path = &paths::sanitize_path(target_path)?;
    let src_conn = neuronprompter_db::SqliteConnection::open(source_path)
        .map_err(|e| ServiceError::IoError(std::io::Error::other(e.to_string())))?;
    let mut dst_conn = neuronprompter_db::SqliteConnection::open(target_path)
        .map_err(|e| ServiceError::IoError(std::io::Error::other(e.to_string())))?;
    let backup = neuronprompter_db::backup::Backup::new(&src_conn, &mut dst_conn)
        .map_err(|e| ServiceError::IoError(std::io::Error::other(e.to_string())))?;
    backup
        .step(-1)
        .map_err(|e| ServiceError::IoError(std::io::Error::other(e.to_string())))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Builds an `ExportedPrompt` from a prompt with associations and its version
/// history.
fn build_exported_prompt(pwa: &PromptWithAssociations, vers: &[PromptVersion]) -> ExportedPrompt {
    ExportedPrompt {
        title: pwa.prompt.title.clone(),
        content: pwa.prompt.content.clone(),
        description: pwa.prompt.description.clone(),
        notes: pwa.prompt.notes.clone(),
        language: pwa.prompt.language.clone(),
        is_favorite: pwa.prompt.is_favorite,
        tags: pwa.tags.iter().map(|t| t.name.clone()).collect(),
        categories: pwa.categories.iter().map(|c| c.name.clone()).collect(),
        collections: pwa.collections.iter().map(|c| c.name.clone()).collect(),
        versions: vers
            .iter()
            .map(|v| ExportedVersion {
                version_number: v.version_number,
                title: v.title.clone(),
                content: v.content.clone(),
                description: v.description.clone(),
                notes: v.notes.clone(),
                language: v.language.clone(),
                created_at: v.created_at.to_rfc3339(),
            })
            .collect(),
    }
}

/// Resolves tag names to IDs, creating missing tags under the target user.
/// Increments the `created` counter for each newly created tag.
/// Validates and trims each name before lookup/creation.
///
/// # Errors
///
/// Returns `DbError::Core(Validation)` if a tag name fails taxonomy name
/// validation.
/// Returns `DbError` if the persistence layer fails.
pub(crate) fn resolve_tags(
    conn: &rusqlite::Connection,
    user_id: i64,
    names: &[String],
    created: &mut usize,
) -> Result<Vec<i64>, neuronprompter_db::DbError> {
    let mut ids = Vec::with_capacity(names.len());
    for name in names {
        let trimmed =
            validation::validate_taxonomy_name(name).map_err(neuronprompter_db::DbError::Core)?;
        let tag = if let Some(existing) = tags::find_tag_by_name(conn, user_id, &trimmed)? {
            existing
        } else {
            *created += 1;
            tags::create_tag(conn, user_id, &trimmed)?
        };
        ids.push(tag.id);
    }
    Ok(ids)
}

/// Resolves category names to IDs, creating missing categories under the
/// target user.
/// Validates and trims each name before lookup/creation.
///
/// # Errors
///
/// Returns `DbError::Core(Validation)` if a category name fails taxonomy name
/// validation.
/// Returns `DbError` if the persistence layer fails.
pub(crate) fn resolve_categories(
    conn: &rusqlite::Connection,
    user_id: i64,
    names: &[String],
    created: &mut usize,
) -> Result<Vec<i64>, neuronprompter_db::DbError> {
    let mut ids = Vec::with_capacity(names.len());
    for name in names {
        let trimmed =
            validation::validate_taxonomy_name(name).map_err(neuronprompter_db::DbError::Core)?;
        let cat =
            if let Some(existing) = categories::find_category_by_name(conn, user_id, &trimmed)? {
                existing
            } else {
                *created += 1;
                categories::create_category(conn, user_id, &trimmed)?
            };
        ids.push(cat.id);
    }
    Ok(ids)
}

/// Resolves collection names to IDs, creating missing collections under the
/// target user.
/// Validates and trims each name before lookup/creation.
///
/// # Errors
///
/// Returns `DbError::Core(Validation)` if a collection name fails taxonomy
/// name validation.
/// Returns `DbError` if the persistence layer fails.
pub(crate) fn resolve_collections(
    conn: &rusqlite::Connection,
    user_id: i64,
    names: &[String],
    created: &mut usize,
) -> Result<Vec<i64>, neuronprompter_db::DbError> {
    let mut ids = Vec::with_capacity(names.len());
    for name in names {
        let trimmed =
            validation::validate_taxonomy_name(name).map_err(neuronprompter_db::DbError::Core)?;
        let col = if let Some(existing) =
            collections::find_collection_by_name(conn, user_id, &trimmed)?
        {
            existing
        } else {
            *created += 1;
            collections::create_collection(conn, user_id, &trimmed)?
        };
        ids.push(col.id);
    }
    Ok(ids)
}

/// Parses a Markdown file with YAML front-matter delimited by `---` lines.
/// Returns the parsed front-matter and the body content.
///
/// M-07: Line endings are normalized to `\n` before parsing so that files
/// with `\r\n` (Windows) or bare `\r` (legacy Mac) line endings are handled
/// consistently.
fn parse_markdown_frontmatter(raw: &str) -> Result<(MarkdownFrontMatter, String), ServiceError> {
    // Normalize all line ending variants to Unix-style `\n` before parsing.
    let normalized = raw.replace("\r\n", "\n").replace('\r', "\n");

    let trimmed = normalized.trim_start();
    if !trimmed.starts_with("---") {
        return Err(ServiceError::SerializationError(
            "Markdown file missing YAML front-matter delimiter".to_owned(),
        ));
    }

    // Find the closing `---` delimiter after the opening one.
    let after_first = &trimmed[3..];
    let closing_idx = after_first.find("\n---").ok_or_else(|| {
        ServiceError::SerializationError(
            "Markdown file missing closing front-matter delimiter".to_owned(),
        )
    })?;

    let yaml_str = &after_first[..closing_idx];
    let body_start = closing_idx + 4; // skip "\n---"
    let body = after_first[body_start..]
        .trim_start_matches('\n')
        .to_owned();

    // serde_yml replaces the deprecated serde_yaml crate for YAML deserialization.
    let front: MarkdownFrontMatter = serde_yml::from_str(yaml_str)
        .map_err(|e| ServiceError::SerializationError(e.to_string()))?;

    Ok((front, body))
}
