// =============================================================================
// Script repository operations.
//
// Provides CRUD functions for the scripts table, which mirrors the prompts
// aggregate but adds a `script_language` column. Handles script creation,
// retrieval (with and without associations), updates, deletion,
// favorite/archive toggling, duplication, and full-text search.
// =============================================================================

use std::fmt::Write;

use neuronprompter_core::CoreError;
use neuronprompter_core::domain::category::Category;
use neuronprompter_core::domain::collection::Collection;
use neuronprompter_core::domain::script::{
    NewScript, Script, ScriptFilter, ScriptWithAssociations,
};
use neuronprompter_core::domain::tag::Tag;
use rusqlite::params;

use crate::DbError;

// =============================================================================
// CRUD operations
// =============================================================================

/// Inserts a script record and links initial associations (tags, categories,
/// collections) via junction tables. Returns the persisted script.
///
/// # Errors
///
/// Returns `DbError::Query` if the SQL statement fails.
pub fn create_script(conn: &rusqlite::Connection, new: &NewScript) -> Result<Script, DbError> {
    super::with_savepoint(conn, "create_script", |conn| {
        conn.execute(
            "INSERT INTO scripts (user_id, title, content, description, \
             notes, script_language, language, source_path, is_synced) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                new.user_id,
                new.title,
                new.content,
                new.description,
                new.notes,
                new.script_language,
                new.language,
                new.source_path,
                new.is_synced,
            ],
        )
        .map_err(|e| DbError::Query {
            operation: "create_script".to_owned(),
            source: e,
        })?;
        let id = conn.last_insert_rowid();

        for tag_id in &new.tag_ids {
            link_script_tag(conn, id, *tag_id)?;
        }
        for cat_id in &new.category_ids {
            link_script_category(conn, id, *cat_id)?;
        }
        for col_id in &new.collection_ids {
            link_script_collection(conn, id, *col_id)?;
        }

        get_script(conn, id)
    })
}

/// Returns the user_id for a script, or NotFound if the script doesn't exist.
/// Lightweight alternative to loading the full script for ownership checks.
///
/// # Errors
///
/// Returns `DbError::Core` with `CoreError::NotFound` if no script exists
/// with the given id.
/// Returns `DbError::Query` if the SQL statement fails.
pub fn get_script_owner(conn: &rusqlite::Connection, script_id: i64) -> Result<i64, DbError> {
    conn.query_row(
        "SELECT user_id FROM scripts WHERE id = ?1",
        params![script_id],
        |row| row.get(0),
    )
    .map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => DbError::Core(CoreError::NotFound {
            entity: "Script".to_owned(),
            id: script_id,
        }),
        other => DbError::Query {
            operation: "get_script_owner".to_owned(),
            source: other,
        },
    })
}

/// Returns the distinct non-null script_language codes used by a user's scripts.
///
/// # Errors
///
/// Returns `DbError::Query` if the SQL statement fails.
pub fn list_script_languages(
    conn: &rusqlite::Connection,
    user_id: i64,
) -> Result<Vec<String>, DbError> {
    let mut stmt = conn.prepare(
        "SELECT DISTINCT script_language FROM scripts WHERE user_id = ?1 AND script_language IS NOT NULL ORDER BY script_language"
    ).map_err(|e| DbError::Query { operation: "list_script_languages".to_owned(), source: e })?;
    let rows = stmt
        .query_map(params![user_id], |row| row.get(0))
        .map_err(|e| DbError::Query {
            operation: "list_script_languages".to_owned(),
            source: e,
        })?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| DbError::Query {
            operation: "list_script_languages".to_owned(),
            source: e,
        })
}

/// Retrieves a script by primary key without associations.
///
/// # Errors
///
/// Returns `DbError::Core` with `CoreError::NotFound` if no script exists
/// with the given id.
/// Returns `DbError::Query` if the SQL statement fails.
pub fn get_script(conn: &rusqlite::Connection, id: i64) -> Result<Script, DbError> {
    conn.query_row(
        "SELECT id, user_id, title, content, description, notes, \
         script_language, language, is_favorite, is_archived, current_version, \
         created_at, updated_at, source_path, is_synced \
         FROM scripts WHERE id = ?1",
        params![id],
        row_to_script,
    )
    .map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => DbError::Core(CoreError::NotFound {
            entity: "Script".to_owned(),
            id,
        }),
        other => DbError::Query {
            operation: "get_script".to_owned(),
            source: other,
        },
    })
}

/// Retrieves a script by primary key, enforcing ownership via `user_id`.
/// Returns `NotFound` if the script does not exist or does not belong to the
/// specified user.
///
/// # Errors
///
/// Returns `DbError::Core` with `CoreError::NotFound` if no script exists
/// with the given id and user_id combination.
/// Returns `DbError::Query` if the SQL statement fails.
pub fn get_script_for_user(
    conn: &rusqlite::Connection,
    id: i64,
    user_id: i64,
) -> Result<Script, DbError> {
    conn.query_row(
        "SELECT id, user_id, title, content, description, notes, \
         script_language, language, is_favorite, is_archived, current_version, \
         created_at, updated_at, source_path, is_synced \
         FROM scripts WHERE id = ?1 AND user_id = ?2",
        params![id, user_id],
        row_to_script,
    )
    .map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => DbError::Core(CoreError::NotFound {
            entity: "Script".to_owned(),
            id,
        }),
        other => DbError::Query {
            operation: "get_script_for_user".to_owned(),
            source: other,
        },
    })
}

/// Retrieves a script with its resolved tags, categories, and collections.
/// The four queries (script row, tags, categories, collections) run inside a
/// SAVEPOINT so the reads form an atomic snapshot. Without the savepoint,
/// a concurrent write could modify associations between individual SELECTs,
/// producing an inconsistent aggregate.
///
/// # Errors
///
/// Returns `DbError::Core` with `CoreError::NotFound` if no script exists
/// with the given id.
/// Returns `DbError::Query` if the SQL statement fails.
pub fn get_script_with_associations(
    conn: &rusqlite::Connection,
    id: i64,
) -> Result<ScriptWithAssociations, DbError> {
    super::with_savepoint(conn, "get_script_assoc", |conn| {
        let script = get_script(conn, id)?;
        let script_tags = get_tags_for_script(conn, id)?;
        let script_categories = get_categories_for_script(conn, id)?;
        let script_collections = get_collections_for_script(conn, id)?;

        Ok(ScriptWithAssociations {
            script,
            tags: script_tags,
            categories: script_categories,
            collections: script_collections,
        })
    })
}

/// Returns scripts matching the filter criteria, ordered by `updated_at`
/// descending.
/// Returns the total number of scripts owned by the given user.
///
/// # Errors
///
/// Returns `DbError::Query` if the SQL statement fails.
pub fn count_scripts(conn: &rusqlite::Connection, user_id: i64) -> Result<i64, DbError> {
    conn.query_row(
        "SELECT COUNT(*) FROM scripts WHERE user_id = ?1",
        params![user_id],
        |row| row.get(0),
    )
    .map_err(|e| DbError::Query {
        operation: "count_scripts".to_owned(),
        source: e,
    })
}

/// Returns the count of scripts matching the filter criteria (same WHERE
/// logic as `list_scripts` but without LIMIT/OFFSET).
///
/// # Errors
///
/// Returns `DbError::Query` if the SQL statement fails.
pub fn count_filtered_scripts(
    conn: &rusqlite::Connection,
    filter: &ScriptFilter,
) -> Result<i64, DbError> {
    let mut sql = String::from("SELECT COUNT(DISTINCT s.id) FROM scripts s");
    let mut conditions: Vec<String> = Vec::new();
    let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    let mut param_idx = 1;

    if let Some(tag_id) = filter.tag_id {
        let _ = write!(
            sql,
            " INNER JOIN script_tags st ON st.script_id = s.id AND st.tag_id = ?{param_idx}"
        );
        param_values.push(Box::new(tag_id));
        param_idx += 1;
    }

    if let Some(cat_id) = filter.category_id {
        let _ = write!(
            sql,
            " INNER JOIN script_categories sc ON sc.script_id = s.id \
             AND sc.category_id = ?{param_idx}"
        );
        param_values.push(Box::new(cat_id));
        param_idx += 1;
    }

    if let Some(col_id) = filter.collection_id {
        let _ = write!(
            sql,
            " INNER JOIN script_collections scol ON scol.script_id = s.id \
             AND scol.collection_id = ?{param_idx}"
        );
        param_values.push(Box::new(col_id));
        param_idx += 1;
    }

    if let Some(uid) = filter.user_id {
        conditions.push(format!("s.user_id = ?{param_idx}"));
        param_values.push(Box::new(uid));
        param_idx += 1;
    }

    if let Some(fav) = filter.is_favorite {
        conditions.push(format!("s.is_favorite = ?{param_idx}"));
        param_values.push(Box::new(fav));
        param_idx += 1;
    }

    if let Some(arch) = filter.is_archived {
        conditions.push(format!("s.is_archived = ?{param_idx}"));
        param_values.push(Box::new(arch));
        param_idx += 1;
    }

    if let Some(synced) = filter.is_synced {
        conditions.push(format!("s.is_synced = ?{param_idx}"));
        param_values.push(Box::new(synced));
        #[allow(unused_assignments)]
        {
            param_idx += 1;
        }
    }

    if !conditions.is_empty() {
        sql.push_str(" WHERE ");
        sql.push_str(&conditions.join(" AND "));
    }

    let params_ref: Vec<&dyn rusqlite::types::ToSql> =
        param_values.iter().map(AsRef::as_ref).collect();
    conn.query_row(&sql, params_ref.as_slice(), |row| row.get(0))
        .map_err(|e| DbError::Query {
            operation: "count_filtered_scripts".to_owned(),
            source: e,
        })
}

/// Returns scripts matching the filter criteria, ordered by `updated_at`
/// descending with pagination.
///
/// # Errors
///
/// Returns `DbError::Query` if the SQL statement fails.
pub fn list_scripts(
    conn: &rusqlite::Connection,
    filter: &ScriptFilter,
) -> Result<Vec<Script>, DbError> {
    let mut sql = String::from(
        "SELECT DISTINCT s.id, s.user_id, s.title, s.content, \
         s.description, s.notes, s.script_language, s.language, s.is_favorite, \
         s.is_archived, s.current_version, s.created_at, s.updated_at, \
         s.source_path, s.is_synced \
         FROM scripts s",
    );
    let mut conditions: Vec<String> = Vec::new();
    let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    let mut param_idx = 1;

    if let Some(tag_id) = filter.tag_id {
        let _ = write!(
            sql,
            " INNER JOIN script_tags st ON st.script_id = s.id AND st.tag_id = ?{param_idx}"
        );
        param_values.push(Box::new(tag_id));
        param_idx += 1;
    }

    if let Some(cat_id) = filter.category_id {
        let _ = write!(
            sql,
            " INNER JOIN script_categories sc ON sc.script_id = s.id \
             AND sc.category_id = ?{param_idx}"
        );
        param_values.push(Box::new(cat_id));
        param_idx += 1;
    }

    if let Some(col_id) = filter.collection_id {
        let _ = write!(
            sql,
            " INNER JOIN script_collections scol ON scol.script_id = s.id \
             AND scol.collection_id = ?{param_idx}"
        );
        param_values.push(Box::new(col_id));
        param_idx += 1;
    }

    if let Some(uid) = filter.user_id {
        conditions.push(format!("s.user_id = ?{param_idx}"));
        param_values.push(Box::new(uid));
        param_idx += 1;
    }

    if let Some(fav) = filter.is_favorite {
        conditions.push(format!("s.is_favorite = ?{param_idx}"));
        param_values.push(Box::new(fav));
        param_idx += 1;
    }

    if let Some(arch) = filter.is_archived {
        conditions.push(format!("s.is_archived = ?{param_idx}"));
        param_values.push(Box::new(arch));
        param_idx += 1;
    }

    if let Some(synced) = filter.is_synced {
        conditions.push(format!("s.is_synced = ?{param_idx}"));
        param_values.push(Box::new(synced));
        param_idx += 1;
    }

    if !conditions.is_empty() {
        sql.push_str(" WHERE ");
        sql.push_str(&conditions.join(" AND "));
    }

    sql.push_str(" ORDER BY s.updated_at DESC");

    // Pagination: default 200, max 1000.
    let default_limit: i64 = 200;
    let limit = filter.limit.unwrap_or(default_limit).clamp(1, 1000);
    let offset = filter.offset.unwrap_or(0).max(0);
    let _ = write!(sql, " LIMIT ?{param_idx}");
    param_values.push(Box::new(limit));
    param_idx += 1;
    let _ = write!(sql, " OFFSET ?{param_idx}");
    param_values.push(Box::new(offset));
    let _ = param_idx;

    let mut stmt = conn.prepare(&sql).map_err(|e| DbError::Query {
        operation: "list_scripts".to_owned(),
        source: e,
    })?;

    let params_ref: Vec<&dyn rusqlite::types::ToSql> =
        param_values.iter().map(AsRef::as_ref).collect();
    let rows = stmt
        .query_map(params_ref.as_slice(), row_to_script)
        .map_err(|e| DbError::Query {
            operation: "list_scripts".to_owned(),
            source: e,
        })?;

    let mut result = Vec::new();
    for row in rows {
        result.push(row.map_err(|e| DbError::Query {
            operation: "list_scripts".to_owned(),
            source: e,
        })?);
    }
    Ok(result)
}

/// Returns all scripts for a user without pagination limits.
/// Used by export and bulk-copy flows that must enumerate every record.
///
/// # Errors
///
/// Returns `DbError::Query` if the SQL statement fails.
pub fn list_all_scripts(conn: &rusqlite::Connection, user_id: i64) -> Result<Vec<Script>, DbError> {
    let sql = "SELECT id, user_id, title, content, description, notes, \
               script_language, language, is_favorite, is_archived, current_version, \
               created_at, updated_at, source_path, is_synced \
               FROM scripts WHERE user_id = ?1 ORDER BY updated_at DESC";
    let mut stmt = conn.prepare(sql).map_err(|e| DbError::Query {
        operation: "list_all_scripts".to_owned(),
        source: e,
    })?;
    let rows = stmt
        .query_map(params![user_id], row_to_script)
        .map_err(|e| DbError::Query {
            operation: "list_all_scripts".to_owned(),
            source: e,
        })?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| DbError::Query {
            operation: "list_all_scripts".to_owned(),
            source: e,
        })
}

/// Updates specific fields on a script. Only supplied values are changed.
/// Increments `current_version` and updates `updated_at` timestamp.
///
/// When `expected_version` is `Some(v)`, the update only succeeds if
/// `current_version` still equals `v` (optimistic concurrency -- F10).
///
/// # Errors
///
/// Returns `DbError::Core` with `CoreError::Conflict` if `expected_version`
/// is provided and does not match the current version.
/// Returns `DbError::Core` with `CoreError::NotFound` if no script exists
/// with the given id.
/// Returns `DbError::Query` if the SQL statement fails.
#[allow(clippy::too_many_arguments)]
pub fn update_script_fields(
    conn: &rusqlite::Connection,
    script_id: i64,
    title: Option<&str>,
    content: Option<&str>,
    description: Option<Option<&str>>,
    notes: Option<Option<&str>>,
    script_language: Option<&str>,
    language: Option<Option<&str>>,
    source_path: Option<Option<&str>>,
    is_synced: Option<bool>,
    expected_version: Option<i64>,
) -> Result<Script, DbError> {
    // If no content fields are provided, return the existing script without
    // incrementing the version number.
    if title.is_none()
        && content.is_none()
        && description.is_none()
        && notes.is_none()
        && script_language.is_none()
        && language.is_none()
        && source_path.is_none()
        && is_synced.is_none()
    {
        return get_script(conn, script_id);
    }

    let mut set_clauses: Vec<String> = Vec::new();
    let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    let mut param_idx = 1;

    if let Some(v) = title {
        set_clauses.push(format!("title = ?{param_idx}"));
        param_values.push(Box::new(v.to_owned()));
        param_idx += 1;
    }
    if let Some(v) = content {
        set_clauses.push(format!("content = ?{param_idx}"));
        param_values.push(Box::new(v.to_owned()));
        param_idx += 1;
    }
    if let Some(v) = description {
        set_clauses.push(format!("description = ?{param_idx}"));
        param_values.push(Box::new(v.map(str::to_owned)));
        param_idx += 1;
    }
    if let Some(v) = notes {
        set_clauses.push(format!("notes = ?{param_idx}"));
        param_values.push(Box::new(v.map(str::to_owned)));
        param_idx += 1;
    }
    if let Some(v) = script_language {
        set_clauses.push(format!("script_language = ?{param_idx}"));
        param_values.push(Box::new(v.to_owned()));
        param_idx += 1;
    }
    if let Some(v) = language {
        set_clauses.push(format!("language = ?{param_idx}"));
        param_values.push(Box::new(v.map(str::to_owned)));
        param_idx += 1;
    }
    if let Some(v) = source_path {
        set_clauses.push(format!("source_path = ?{param_idx}"));
        param_values.push(Box::new(v.map(str::to_owned)));
        param_idx += 1;
    }
    if let Some(v) = is_synced {
        set_clauses.push(format!("is_synced = ?{param_idx}"));
        param_values.push(Box::new(v));
        param_idx += 1;
    }

    // Always increment version and update timestamp.
    set_clauses.push("current_version = current_version + 1".to_owned());
    set_clauses.push("updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')".to_owned());

    // F10: Optionally include a version predicate for optimistic concurrency.
    let mut where_clause = format!("WHERE id = ?{param_idx}");
    param_values.push(Box::new(script_id));
    param_idx += 1;

    if let Some(ev) = expected_version {
        use std::fmt::Write;
        let _ = write!(where_clause, " AND current_version = ?{param_idx}");
        param_values.push(Box::new(ev));
    }

    let sql = format!(
        "UPDATE scripts SET {} {where_clause}",
        set_clauses.join(", ")
    );

    let params_ref: Vec<&dyn rusqlite::types::ToSql> =
        param_values.iter().map(AsRef::as_ref).collect();
    let affected = conn
        .execute(&sql, params_ref.as_slice())
        .map_err(|e| DbError::Query {
            operation: "update_script_fields".to_owned(),
            source: e,
        })?;

    if affected == 0 {
        if expected_version.is_some() {
            if let Ok(existing) = get_script(conn, script_id) {
                return Err(DbError::Core(CoreError::Conflict {
                    entity: "Script".to_owned(),
                    id: script_id,
                    expected: expected_version.unwrap_or(0),
                    actual: existing.current_version,
                }));
            }
        }
        return Err(DbError::Core(CoreError::NotFound {
            entity: "Script".to_owned(),
            id: script_id,
        }));
    }

    get_script(conn, script_id)
}

/// Deletes a script and all associated data (versions, junction rows) via
/// foreign key CASCADE rules.
///
/// # Errors
///
/// Returns `DbError::Core` with `CoreError::NotFound` if no script exists
/// with the given id.
/// Returns `DbError::Query` if the SQL statement fails.
pub fn delete_script(conn: &rusqlite::Connection, id: i64) -> Result<(), DbError> {
    let affected = conn
        .execute("DELETE FROM scripts WHERE id = ?1", params![id])
        .map_err(|e| DbError::Query {
            operation: "delete_script".to_owned(),
            source: e,
        })?;
    if affected == 0 {
        return Err(DbError::Core(CoreError::NotFound {
            entity: "Script".to_owned(),
            id,
        }));
    }
    Ok(())
}

/// Sets or clears the favorite flag on a script.
///
/// The WHERE clause uses only the script id without a user_id predicate. This creates a
/// theoretical TOCTOU gap between the handler-level ownership check and this mutation.
/// In practice, the ownership check (which runs on the same connection within the same
/// request) provides sufficient protection for this desktop/LAN application. Adding
/// AND user_id = ? would eliminate the gap but requires propagating user_id through
/// the service layer.
///
/// # Errors
///
/// Returns `DbError::Core` with `CoreError::NotFound` if no script exists
/// with the given id.
/// Returns `DbError::Query` if the SQL statement fails.
pub fn set_favorite(
    conn: &rusqlite::Connection,
    id: i64,
    is_favorite: bool,
) -> Result<(), DbError> {
    let affected = conn
        .execute(
            "UPDATE scripts SET is_favorite = ?1, \
             updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now') \
             WHERE id = ?2",
            params![is_favorite, id],
        )
        .map_err(|e| DbError::Query {
            operation: "set_favorite".to_owned(),
            source: e,
        })?;
    if affected == 0 {
        return Err(DbError::Core(CoreError::NotFound {
            entity: "Script".to_owned(),
            id,
        }));
    }
    Ok(())
}

/// Sets or clears the archived flag on a script.
///
/// The WHERE clause uses only the script id without a user_id predicate. This creates a
/// theoretical TOCTOU gap between the handler-level ownership check and this mutation.
/// In practice, the ownership check (which runs on the same connection within the same
/// request) provides sufficient protection for this desktop/LAN application. Adding
/// AND user_id = ? would eliminate the gap but requires propagating user_id through
/// the service layer.
///
/// # Errors
///
/// Returns `DbError::Core` with `CoreError::NotFound` if no script exists
/// with the given id.
/// Returns `DbError::Query` if the SQL statement fails.
pub fn set_archived(
    conn: &rusqlite::Connection,
    id: i64,
    is_archived: bool,
) -> Result<(), DbError> {
    let affected = conn
        .execute(
            "UPDATE scripts SET is_archived = ?1, \
             updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now') \
             WHERE id = ?2",
            params![is_archived, id],
        )
        .map_err(|e| DbError::Query {
            operation: "set_archived".to_owned(),
            source: e,
        })?;
    if affected == 0 {
        return Err(DbError::Core(CoreError::NotFound {
            entity: "Script".to_owned(),
            id,
        }));
    }
    Ok(())
}

/// Creates a copy of an existing script with version reset to 1 and a
/// modified title indicating it is a duplicate. The `source_path` and
/// `is_synced` fields are intentionally omitted from the duplicate because a
/// copied script should not maintain a filesystem sync relationship with the
/// original source file.
///
/// # Errors
///
/// Returns `DbError::Core` with `CoreError::NotFound` if no script exists
/// with the given id.
/// Returns `DbError::Query` if the SQL statement fails.
pub fn duplicate_script(conn: &rusqlite::Connection, id: i64) -> Result<Script, DbError> {
    super::with_savepoint(conn, "dup_script", |conn| {
        let original = get_script(conn, id)?;
        let suffix = " (copy)";
        let max_base_chars = 200 - suffix.len();
        let base: String = original.title.chars().take(max_base_chars).collect();
        let dup_title = format!("{base}{suffix}");

        conn.execute(
            "INSERT INTO scripts (user_id, title, content, description, \
             notes, script_language, language) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                original.user_id,
                dup_title,
                original.content,
                original.description,
                original.notes,
                original.script_language,
                original.language,
            ],
        )
        .map_err(|e| DbError::Query {
            operation: "duplicate_script".to_owned(),
            source: e,
        })?;
        let new_id = conn.last_insert_rowid();

        // Copy junction-table associations from the original script.
        conn.execute(
            "INSERT INTO script_tags (script_id, tag_id) \
             SELECT ?1, tag_id FROM script_tags WHERE script_id = ?2",
            params![new_id, id],
        )
        .map_err(|e| DbError::Query {
            operation: "duplicate_script_tags".to_owned(),
            source: e,
        })?;
        conn.execute(
            "INSERT INTO script_categories (script_id, category_id) \
             SELECT ?1, category_id FROM script_categories WHERE script_id = ?2",
            params![new_id, id],
        )
        .map_err(|e| DbError::Query {
            operation: "duplicate_script_categories".to_owned(),
            source: e,
        })?;
        conn.execute(
            "INSERT INTO script_collections (script_id, collection_id) \
             SELECT ?1, collection_id FROM script_collections WHERE script_id = ?2",
            params![new_id, id],
        )
        .map_err(|e| DbError::Query {
            operation: "duplicate_script_collections".to_owned(),
            source: e,
        })?;

        get_script(conn, new_id)
    })
}

/// Copies an existing script to a different user with pre-resolved taxonomy
/// IDs and a caller-supplied title. Resets version to 1, clears
/// favorite/archive flags. Sets `source_path` to NULL and `is_synced` to false.
///
/// # Errors
///
/// Returns `DbError::Core` with `CoreError::NotFound` if no script exists
/// with the given source id.
/// Returns `DbError::Query` if the SQL statement fails.
pub fn copy_script_to_user(
    conn: &rusqlite::Connection,
    source_id: i64,
    target_user_id: i64,
    new_title: &str,
    tag_ids: &[i64],
    category_ids: &[i64],
    collection_ids: &[i64],
) -> Result<Script, DbError> {
    super::with_savepoint(conn, "copy_script_to_user", |conn| {
        let original = get_script(conn, source_id)?;

        conn.execute(
            "INSERT INTO scripts (user_id, title, content, description, \
             notes, script_language, language) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                target_user_id,
                new_title,
                original.content,
                original.description,
                original.notes,
                original.script_language,
                original.language,
            ],
        )
        .map_err(|e| DbError::Query {
            operation: "copy_script_to_user".to_owned(),
            source: e,
        })?;
        let new_id = conn.last_insert_rowid();

        for tag_id in tag_ids {
            link_script_tag(conn, new_id, *tag_id)?;
        }
        for cat_id in category_ids {
            link_script_category(conn, new_id, *cat_id)?;
        }
        for col_id in collection_ids {
            link_script_collection(conn, new_id, *col_id)?;
        }

        get_script(conn, new_id)
    })
}

/// Finds a script by title and content for the given user. Used for
/// deduplication during cross-user copy operations.
///
/// # Errors
///
/// Returns `DbError::Query` if the SQL statement fails.
pub fn find_script_by_title_and_content(
    conn: &rusqlite::Connection,
    user_id: i64,
    title: &str,
    content: &str,
) -> Result<Option<Script>, DbError> {
    let result = conn.query_row(
        "SELECT id, user_id, title, content, description, notes, \
         script_language, language, is_favorite, is_archived, current_version, \
         created_at, updated_at, source_path, is_synced \
         FROM scripts WHERE user_id = ?1 AND title = ?2 AND content = ?3",
        params![user_id, title, content],
        row_to_script,
    );
    match result {
        Ok(s) => Ok(Some(s)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(DbError::Query {
            operation: "find_script_by_title_and_content".to_owned(),
            source: e,
        }),
    }
}

/// Checks whether the given user already owns a script with the specified title.
///
/// # Errors
///
/// Returns `DbError::Query` if the SQL statement fails.
pub fn script_title_exists(
    conn: &rusqlite::Connection,
    user_id: i64,
    title: &str,
) -> Result<bool, DbError> {
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM scripts WHERE user_id = ?1 AND title = ?2",
            params![user_id, title],
            |row| row.get(0),
        )
        .map_err(|e| DbError::Query {
            operation: "script_title_exists".to_owned(),
            source: e,
        })?;
    Ok(count > 0)
}

/// Returns all synced scripts for a given user (where `is_synced = 1`).
///
/// # Errors
///
/// Returns `DbError::Query` if the SQL statement fails.
pub fn list_synced_scripts(
    conn: &rusqlite::Connection,
    user_id: i64,
) -> Result<Vec<Script>, DbError> {
    let mut stmt = conn
        .prepare(
            "SELECT id, user_id, title, content, description, notes, \
             script_language, language, is_favorite, is_archived, current_version, \
             created_at, updated_at, source_path, is_synced \
             FROM scripts WHERE user_id = ?1 AND is_synced = 1",
        )
        .map_err(|e| DbError::Query {
            operation: "list_synced_scripts".to_owned(),
            source: e,
        })?;
    let rows = stmt
        .query_map(params![user_id], row_to_script)
        .map_err(|e| DbError::Query {
            operation: "list_synced_scripts".to_owned(),
            source: e,
        })?;
    let mut result = Vec::new();
    for row in rows {
        result.push(row.map_err(|e| DbError::Query {
            operation: "list_synced_scripts".to_owned(),
            source: e,
        })?);
    }
    Ok(result)
}

// =============================================================================
// Full-text search
// =============================================================================

/// Searches scripts matching the query string with optional filters.
/// Each query term is suffixed with `*` for prefix matching. Results are
/// ranked by FTS5 BM25 relevance score and limited to 200 entries.
///
/// Archived scripts are excluded unless the filter explicitly sets
/// `is_archived = Some(true)`.
///
/// # Errors
///
/// Returns `DbError::Query` if the SQL statement fails.
pub fn search_scripts(
    conn: &rusqlite::Connection,
    user_id: i64,
    query: &str,
    filter: &ScriptFilter,
) -> Result<Vec<Script>, DbError> {
    // Tokenize the query and append wildcard for prefix matching.
    let fts_query = super::build_fts_query(query);
    if fts_query.is_empty() {
        return Ok(Vec::new());
    }

    let mut sql = String::from(
        "SELECT s.id, s.user_id, s.title, s.content, \
         s.description, s.notes, s.script_language, s.language, s.is_favorite, \
         s.is_archived, s.current_version, s.created_at, s.updated_at, \
         s.source_path, s.is_synced \
         FROM scripts s \
         INNER JOIN scripts_fts fts ON fts.rowid = s.id \
         WHERE fts.scripts_fts MATCH ?1 \
         AND s.user_id = ?2",
    );
    let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    param_values.push(Box::new(fts_query));
    param_values.push(Box::new(user_id));
    let mut param_idx: usize = 3;

    // Exclude archived scripts unless explicitly requested.
    let is_archived = filter.is_archived.unwrap_or(false);
    let _ = write!(sql, " AND s.is_archived = ?{param_idx}");
    param_values.push(Box::new(is_archived));
    param_idx += 1;

    if let Some(fav) = filter.is_favorite {
        let _ = write!(sql, " AND s.is_favorite = ?{param_idx}");
        param_values.push(Box::new(fav));
        param_idx += 1;
    }

    if let Some(tag_id) = filter.tag_id {
        let _ = write!(
            sql,
            " AND s.id IN (SELECT script_id FROM script_tags WHERE tag_id = ?{param_idx})"
        );
        param_values.push(Box::new(tag_id));
        param_idx += 1;
    }

    if let Some(cat_id) = filter.category_id {
        let _ = write!(
            sql,
            " AND s.id IN (SELECT script_id FROM script_categories WHERE category_id = ?{param_idx})"
        );
        param_values.push(Box::new(cat_id));
        param_idx += 1;
    }

    if let Some(col_id) = filter.collection_id {
        let _ = write!(
            sql,
            " AND s.id IN (SELECT script_id FROM script_collections WHERE collection_id = ?{param_idx})"
        );
        param_values.push(Box::new(col_id));
        param_idx += 1;
    }

    let effective_limit = filter.limit.unwrap_or(200).clamp(1, 1000);
    let _ = write!(sql, " ORDER BY bm25(scripts_fts) LIMIT ?{param_idx}");
    param_values.push(Box::new(effective_limit));
    param_idx += 1;

    let effective_offset = filter.offset.unwrap_or(0).max(0);
    if effective_offset > 0 {
        let _ = write!(sql, " OFFSET ?{param_idx}");
        param_values.push(Box::new(effective_offset));
    }

    let mut stmt = conn.prepare(&sql).map_err(|e| DbError::Query {
        operation: "search_scripts".to_owned(),
        source: e,
    })?;

    let params_ref: Vec<&dyn rusqlite::types::ToSql> =
        param_values.iter().map(AsRef::as_ref).collect();

    let rows = stmt
        .query_map(params_ref.as_slice(), row_to_script)
        .map_err(|e| DbError::Query {
            operation: "search_scripts".to_owned(),
            source: e,
        })?;

    let mut result = Vec::new();
    for row in rows {
        result.push(row.map_err(|e| DbError::Query {
            operation: "search_scripts".to_owned(),
            source: e,
        })?);
    }
    Ok(result)
}

// =============================================================================
// Junction-table helpers (tags)
// =============================================================================

/// Creates a junction-table link between a script and a tag. Silently
/// succeeds if the link already exists (ON CONFLICT DO NOTHING).
///
/// # Errors
///
/// Returns `DbError::Query` if the SQL statement fails.
pub fn link_script_tag(
    conn: &rusqlite::Connection,
    script_id: i64,
    tag_id: i64,
) -> Result<(), DbError> {
    conn.execute(
        "INSERT INTO script_tags (script_id, tag_id) VALUES (?1, ?2) ON CONFLICT (script_id, tag_id) DO NOTHING",
        params![script_id, tag_id],
    )
    .map_err(|e| DbError::Query {
        operation: "link_script_tag".to_owned(),
        source: e,
    })?;
    Ok(())
}

/// Removes the junction-table link between a script and a tag.
///
/// # Errors
///
/// Returns `DbError::Query` if the SQL statement fails.
pub fn unlink_script_tag(
    conn: &rusqlite::Connection,
    script_id: i64,
    tag_id: i64,
) -> Result<(), DbError> {
    conn.execute(
        "DELETE FROM script_tags WHERE script_id = ?1 AND tag_id = ?2",
        params![script_id, tag_id],
    )
    .map_err(|e| DbError::Query {
        operation: "unlink_script_tag".to_owned(),
        source: e,
    })?;
    Ok(())
}

/// Returns tags associated with a specific script via the `script_tags`
/// junction table.
///
/// # Errors
///
/// Returns `DbError::Query` if the SQL statement fails.
pub fn get_tags_for_script(
    conn: &rusqlite::Connection,
    script_id: i64,
) -> Result<Vec<Tag>, DbError> {
    let mut stmt = conn
        .prepare(
            "SELECT t.id, t.user_id, t.name, t.created_at \
             FROM tags t \
             INNER JOIN script_tags st ON st.tag_id = t.id \
             WHERE st.script_id = ?1 \
             ORDER BY t.name",
        )
        .map_err(|e| DbError::Query {
            operation: "get_tags_for_script".to_owned(),
            source: e,
        })?;
    let rows = stmt
        .query_map(params![script_id], |row| {
            Ok(Tag {
                id: row.get(0)?,
                user_id: row.get(1)?,
                name: row.get(2)?,
                created_at: row.get(3)?,
            })
        })
        .map_err(|e| DbError::Query {
            operation: "get_tags_for_script".to_owned(),
            source: e,
        })?;
    let mut result = Vec::new();
    for row in rows {
        result.push(row.map_err(|e| DbError::Query {
            operation: "get_tags_for_script".to_owned(),
            source: e,
        })?);
    }
    Ok(result)
}

// =============================================================================
// Junction-table helpers (categories)
// =============================================================================

/// Creates a junction-table link between a script and a category.
///
/// # Errors
///
/// Returns `DbError::Query` if the SQL statement fails.
pub fn link_script_category(
    conn: &rusqlite::Connection,
    script_id: i64,
    category_id: i64,
) -> Result<(), DbError> {
    conn.execute(
        "INSERT INTO script_categories (script_id, category_id) VALUES (?1, ?2) ON CONFLICT (script_id, category_id) DO NOTHING",
        params![script_id, category_id],
    )
    .map_err(|e| DbError::Query {
        operation: "link_script_category".to_owned(),
        source: e,
    })?;
    Ok(())
}

/// Removes the junction-table link between a script and a category.
///
/// # Errors
///
/// Returns `DbError::Query` if the SQL statement fails.
pub fn unlink_script_category(
    conn: &rusqlite::Connection,
    script_id: i64,
    category_id: i64,
) -> Result<(), DbError> {
    conn.execute(
        "DELETE FROM script_categories WHERE script_id = ?1 AND category_id = ?2",
        params![script_id, category_id],
    )
    .map_err(|e| DbError::Query {
        operation: "unlink_script_category".to_owned(),
        source: e,
    })?;
    Ok(())
}

/// Returns categories associated with a specific script via the
/// `script_categories` junction table.
///
/// # Errors
///
/// Returns `DbError::Query` if the SQL statement fails.
pub fn get_categories_for_script(
    conn: &rusqlite::Connection,
    script_id: i64,
) -> Result<Vec<Category>, DbError> {
    let mut stmt = conn
        .prepare(
            "SELECT c.id, c.user_id, c.name, c.created_at \
             FROM categories c \
             INNER JOIN script_categories sc ON sc.category_id = c.id \
             WHERE sc.script_id = ?1 \
             ORDER BY c.name",
        )
        .map_err(|e| DbError::Query {
            operation: "get_categories_for_script".to_owned(),
            source: e,
        })?;
    let rows = stmt
        .query_map(params![script_id], |row| {
            Ok(Category {
                id: row.get(0)?,
                user_id: row.get(1)?,
                name: row.get(2)?,
                created_at: row.get(3)?,
            })
        })
        .map_err(|e| DbError::Query {
            operation: "get_categories_for_script".to_owned(),
            source: e,
        })?;
    let mut result = Vec::new();
    for row in rows {
        result.push(row.map_err(|e| DbError::Query {
            operation: "get_categories_for_script".to_owned(),
            source: e,
        })?);
    }
    Ok(result)
}

// =============================================================================
// Junction-table helpers (collections)
// =============================================================================

/// Creates a junction-table link between a script and a collection.
///
/// # Errors
///
/// Returns `DbError::Query` if the SQL statement fails.
pub fn link_script_collection(
    conn: &rusqlite::Connection,
    script_id: i64,
    collection_id: i64,
) -> Result<(), DbError> {
    conn.execute(
        "INSERT INTO script_collections (script_id, collection_id) VALUES (?1, ?2) ON CONFLICT (script_id, collection_id) DO NOTHING",
        params![script_id, collection_id],
    )
    .map_err(|e| DbError::Query {
        operation: "link_script_collection".to_owned(),
        source: e,
    })?;
    Ok(())
}

/// Removes the junction-table link between a script and a collection.
///
/// # Errors
///
/// Returns `DbError::Query` if the SQL statement fails.
pub fn unlink_script_collection(
    conn: &rusqlite::Connection,
    script_id: i64,
    collection_id: i64,
) -> Result<(), DbError> {
    conn.execute(
        "DELETE FROM script_collections WHERE script_id = ?1 AND collection_id = ?2",
        params![script_id, collection_id],
    )
    .map_err(|e| DbError::Query {
        operation: "unlink_script_collection".to_owned(),
        source: e,
    })?;
    Ok(())
}

/// Returns collections associated with a specific script via the
/// `script_collections` junction table.
///
/// # Errors
///
/// Returns `DbError::Query` if the SQL statement fails.
pub fn get_collections_for_script(
    conn: &rusqlite::Connection,
    script_id: i64,
) -> Result<Vec<Collection>, DbError> {
    let mut stmt = conn
        .prepare(
            "SELECT c.id, c.user_id, c.name, c.created_at \
             FROM collections c \
             INNER JOIN script_collections sc ON sc.collection_id = c.id \
             WHERE sc.script_id = ?1 \
             ORDER BY c.name",
        )
        .map_err(|e| DbError::Query {
            operation: "get_collections_for_script".to_owned(),
            source: e,
        })?;
    let rows = stmt
        .query_map(params![script_id], |row| {
            Ok(Collection {
                id: row.get(0)?,
                user_id: row.get(1)?,
                name: row.get(2)?,
                created_at: row.get(3)?,
            })
        })
        .map_err(|e| DbError::Query {
            operation: "get_collections_for_script".to_owned(),
            source: e,
        })?;
    let mut result = Vec::new();
    for row in rows {
        result.push(row.map_err(|e| DbError::Query {
            operation: "get_collections_for_script".to_owned(),
            source: e,
        })?);
    }
    Ok(result)
}

// =============================================================================
// Private helpers
// =============================================================================

/// Maps a `rusqlite` row to a `Script` struct. Column names must match the
/// SELECT statements used throughout this module.
fn row_to_script(row: &rusqlite::Row<'_>) -> rusqlite::Result<Script> {
    Ok(Script {
        id: row.get("id")?,
        user_id: row.get("user_id")?,
        title: row.get("title")?,
        content: row.get("content")?,
        description: row.get("description")?,
        notes: row.get("notes")?,
        script_language: row.get("script_language")?,
        language: row.get("language")?,
        is_favorite: row.get("is_favorite")?,
        is_archived: row.get("is_archived")?,
        current_version: row.get("current_version")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
        source_path: row.get("source_path")?,
        is_synced: row.get("is_synced")?,
    })
}
