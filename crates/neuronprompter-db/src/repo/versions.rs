// =============================================================================
// Version repository operations.
//
// Provides CRUD functions for the prompt_versions table. Each version row
// captures an immutable snapshot of a prompt's content and metadata at a
// specific version number. Versions are sequentially numbered per prompt.
// =============================================================================

use neuronprompter_core::CoreError;
use neuronprompter_core::domain::version::PromptVersion;
use rusqlite::params;

use crate::DbError;

/// Inserts a version snapshot and returns the persisted entity with the
/// database-assigned id and timestamp.
///
/// # Errors
///
/// Returns `DbError::Query` if the SQL statement fails.
#[allow(clippy::too_many_arguments)]
pub fn insert_version(
    conn: &rusqlite::Connection,
    prompt_id: i64,
    version_number: i64,
    title: &str,
    content: &str,
    description: Option<&str>,
    notes: Option<&str>,
    language: Option<&str>,
) -> Result<PromptVersion, DbError> {
    conn.execute(
        "INSERT INTO prompt_versions \
         (prompt_id, version_number, title, content, description, notes, language) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            prompt_id,
            version_number,
            title,
            content,
            description,
            notes,
            language,
        ],
    )
    .map_err(|e| DbError::Query {
        operation: "insert_version".to_owned(),
        source: e,
    })?;
    let id = conn.last_insert_rowid();
    get_version_by_id(conn, id)
}

/// Returns all version snapshots for a prompt, ordered by version number
/// ascending.
///
/// # Errors
///
/// Returns `DbError::Query` if the SQL statement fails.
pub fn list_versions_for_prompt(
    conn: &rusqlite::Connection,
    prompt_id: i64,
) -> Result<Vec<PromptVersion>, DbError> {
    let mut stmt = conn
        .prepare(
            "SELECT id, prompt_id, version_number, title, content, \
             description, notes, language, created_at \
             FROM prompt_versions WHERE prompt_id = ?1 ORDER BY version_number LIMIT 500",
        )
        .map_err(|e| DbError::Query {
            operation: "list_versions_for_prompt".to_owned(),
            source: e,
        })?;
    let rows = stmt
        .query_map(params![prompt_id], row_to_version)
        .map_err(|e| DbError::Query {
            operation: "list_versions_for_prompt".to_owned(),
            source: e,
        })?;
    let mut result = Vec::new();
    for row in rows {
        result.push(row.map_err(|e| DbError::Query {
            operation: "list_versions_for_prompt".to_owned(),
            source: e,
        })?);
    }
    Ok(result)
}

/// Returns all version snapshots for a prompt without any row limit.
/// Used by export flows that must capture the complete version history.
///
/// # Errors
///
/// Returns `DbError::Query` if the SQL statement fails.
pub fn list_all_versions_for_prompt(
    conn: &rusqlite::Connection,
    prompt_id: i64,
) -> Result<Vec<PromptVersion>, DbError> {
    let mut stmt = conn
        .prepare(
            "SELECT id, prompt_id, version_number, title, content, \
             description, notes, language, created_at \
             FROM prompt_versions WHERE prompt_id = ?1 ORDER BY version_number",
        )
        .map_err(|e| DbError::Query {
            operation: "list_all_versions_for_prompt".to_owned(),
            source: e,
        })?;
    let rows = stmt
        .query_map(params![prompt_id], row_to_version)
        .map_err(|e| DbError::Query {
            operation: "list_all_versions_for_prompt".to_owned(),
            source: e,
        })?;
    let mut result = Vec::new();
    for row in rows {
        result.push(row.map_err(|e| DbError::Query {
            operation: "list_all_versions_for_prompt".to_owned(),
            source: e,
        })?);
    }
    Ok(result)
}

/// Retrieves a specific version snapshot by its primary key.
///
/// # Errors
///
/// Returns `DbError::Core` with `CoreError::NotFound` if no version exists
/// with the given id.
/// Returns `DbError::Query` if the SQL statement fails.
pub fn get_version_by_id(conn: &rusqlite::Connection, id: i64) -> Result<PromptVersion, DbError> {
    conn.query_row(
        "SELECT id, prompt_id, version_number, title, content, \
         description, notes, language, created_at \
         FROM prompt_versions WHERE id = ?1",
        params![id],
        row_to_version,
    )
    .map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => DbError::Core(CoreError::NotFound {
            entity: "PromptVersion".to_owned(),
            id,
        }),
        other => DbError::Query {
            operation: "get_version_by_id".to_owned(),
            source: other,
        },
    })
}

/// Retrieves a specific version snapshot by prompt and version number.
///
/// # Errors
///
/// Returns `DbError::Core` with `CoreError::NotFound` if no version exists
/// with the given prompt id and version number.
/// Returns `DbError::Query` if the SQL statement fails.
pub fn get_version_by_number(
    conn: &rusqlite::Connection,
    prompt_id: i64,
    version_number: i64,
) -> Result<PromptVersion, DbError> {
    conn.query_row(
        "SELECT id, prompt_id, version_number, title, content, \
         description, notes, language, created_at \
         FROM prompt_versions WHERE prompt_id = ?1 AND version_number = ?2",
        params![prompt_id, version_number],
        row_to_version,
    )
    .map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => DbError::Core(CoreError::NotFound {
            entity: "PromptVersion".to_owned(),
            id: version_number,
        }),
        other => DbError::Query {
            operation: "get_version_by_number".to_owned(),
            source: other,
        },
    })
}

/// Maps a `rusqlite` row to a `PromptVersion` struct. Column names must match
/// the SELECT statements above.
fn row_to_version(row: &rusqlite::Row<'_>) -> rusqlite::Result<PromptVersion> {
    Ok(PromptVersion {
        id: row.get("id")?,
        prompt_id: row.get("prompt_id")?,
        version_number: row.get("version_number")?,
        title: row.get("title")?,
        content: row.get("content")?,
        description: row.get("description")?,
        notes: row.get("notes")?,
        language: row.get("language")?,
        created_at: row.get("created_at")?,
    })
}
