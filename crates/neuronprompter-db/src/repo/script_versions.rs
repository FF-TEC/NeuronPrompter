// =============================================================================
// Script version repository operations.
//
// Provides CRUD functions for the script_versions table. Each version row
// captures an immutable snapshot of a script's content and metadata at a
// specific version number. Versions are sequentially numbered per script.
// =============================================================================

use neuronprompter_core::CoreError;
use neuronprompter_core::domain::script_version::ScriptVersion;
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
    script_id: i64,
    version_number: i64,
    title: &str,
    content: &str,
    description: Option<&str>,
    notes: Option<&str>,
    script_language: &str,
    language: Option<&str>,
) -> Result<ScriptVersion, DbError> {
    conn.execute(
        "INSERT INTO script_versions \
         (script_id, version_number, title, content, description, notes, script_language, language) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            script_id,
            version_number,
            title,
            content,
            description,
            notes,
            script_language,
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

/// Returns all version snapshots for a script, ordered by version number
/// ascending.
///
/// # Errors
///
/// Returns `DbError::Query` if the SQL statement fails.
pub fn list_versions_for_script(
    conn: &rusqlite::Connection,
    script_id: i64,
) -> Result<Vec<ScriptVersion>, DbError> {
    let mut stmt = conn
        .prepare(
            "SELECT id, script_id, version_number, title, content, \
             description, notes, script_language, language, created_at \
             FROM script_versions WHERE script_id = ?1 ORDER BY version_number LIMIT 500",
        )
        .map_err(|e| DbError::Query {
            operation: "list_versions_for_script".to_owned(),
            source: e,
        })?;
    let rows = stmt
        .query_map(params![script_id], row_to_version)
        .map_err(|e| DbError::Query {
            operation: "list_versions_for_script".to_owned(),
            source: e,
        })?;
    let mut result = Vec::new();
    for row in rows {
        result.push(row.map_err(|e| DbError::Query {
            operation: "list_versions_for_script".to_owned(),
            source: e,
        })?);
    }
    Ok(result)
}

/// Returns all version snapshots for a script without any row limit.
/// Used by export flows that must capture the complete version history.
///
/// # Errors
///
/// Returns `DbError::Query` if the SQL statement fails.
pub fn list_all_versions_for_script(
    conn: &rusqlite::Connection,
    script_id: i64,
) -> Result<Vec<ScriptVersion>, DbError> {
    let mut stmt = conn
        .prepare(
            "SELECT id, script_id, version_number, title, content, \
             description, notes, script_language, language, created_at \
             FROM script_versions WHERE script_id = ?1 ORDER BY version_number",
        )
        .map_err(|e| DbError::Query {
            operation: "list_all_versions_for_script".to_owned(),
            source: e,
        })?;
    let rows = stmt
        .query_map(params![script_id], row_to_version)
        .map_err(|e| DbError::Query {
            operation: "list_all_versions_for_script".to_owned(),
            source: e,
        })?;
    let mut result = Vec::new();
    for row in rows {
        result.push(row.map_err(|e| DbError::Query {
            operation: "list_all_versions_for_script".to_owned(),
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
pub fn get_version_by_id(conn: &rusqlite::Connection, id: i64) -> Result<ScriptVersion, DbError> {
    conn.query_row(
        "SELECT id, script_id, version_number, title, content, \
         description, notes, script_language, language, created_at \
         FROM script_versions WHERE id = ?1",
        params![id],
        row_to_version,
    )
    .map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => DbError::Core(CoreError::NotFound {
            entity: "ScriptVersion".to_owned(),
            id,
        }),
        other => DbError::Query {
            operation: "get_version_by_id".to_owned(),
            source: other,
        },
    })
}

/// Retrieves a specific version snapshot by script and version number.
///
/// # Errors
///
/// Returns `DbError::Core` with `CoreError::NotFound` if no version exists
/// with the given script id and version number.
/// Returns `DbError::Query` if the SQL statement fails.
pub fn get_version_by_number(
    conn: &rusqlite::Connection,
    script_id: i64,
    version_number: i64,
) -> Result<ScriptVersion, DbError> {
    conn.query_row(
        "SELECT id, script_id, version_number, title, content, \
         description, notes, script_language, language, created_at \
         FROM script_versions WHERE script_id = ?1 AND version_number = ?2",
        params![script_id, version_number],
        row_to_version,
    )
    .map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => DbError::Core(CoreError::NotFound {
            entity: "ScriptVersion".to_owned(),
            id: version_number,
        }),
        other => DbError::Query {
            operation: "get_version_by_number".to_owned(),
            source: other,
        },
    })
}

/// Maps a `rusqlite` row to a `ScriptVersion` struct. Column names must match
/// the SELECT statements above.
fn row_to_version(row: &rusqlite::Row<'_>) -> rusqlite::Result<ScriptVersion> {
    Ok(ScriptVersion {
        id: row.get("id")?,
        script_id: row.get("script_id")?,
        version_number: row.get("version_number")?,
        title: row.get("title")?,
        content: row.get("content")?,
        description: row.get("description")?,
        notes: row.get("notes")?,
        script_language: row.get("script_language")?,
        language: row.get("language")?,
        created_at: row.get("created_at")?,
    })
}
