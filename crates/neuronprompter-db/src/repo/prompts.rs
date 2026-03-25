// =============================================================================
// Prompt repository operations.
//
// Provides CRUD functions for the prompts table, which is the central aggregate
// of the application. Handles prompt creation, retrieval (with and without
// associations), updates, deletion, favorite/archive toggling, and duplication.
// =============================================================================

use std::fmt::Write;

use neuronprompter_core::CoreError;
use neuronprompter_core::domain::prompt::{
    NewPrompt, Prompt, PromptFilter, PromptWithAssociations,
};
use rusqlite::params;

use super::{categories, collections, tags};
use crate::DbError;

/// Inserts a prompt record and links initial associations (tags, categories,
/// collections) via junction tables. Returns the persisted prompt.
///
/// After the mutation, the prompt is re-fetched from the database to return the
/// complete row with server-generated values (id, created_at, updated_at, current_version).
/// This pattern ensures the returned Prompt struct matches the persisted state exactly.
///
/// # Errors
///
/// Returns `DbError::Query` if the SQL statement fails.
pub fn create_prompt(conn: &rusqlite::Connection, new: &NewPrompt) -> Result<Prompt, DbError> {
    super::with_savepoint(conn, "create_prompt", |conn| {
        conn.execute(
            "INSERT INTO prompts (user_id, title, content, description, \
             notes, language) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                new.user_id,
                new.title,
                new.content,
                new.description,
                new.notes,
                new.language,
            ],
        )
        .map_err(|e| DbError::Query {
            operation: "create_prompt".to_owned(),
            source: e,
        })?;
        let id = conn.last_insert_rowid();

        for tag_id in &new.tag_ids {
            tags::link_prompt_tag(conn, id, *tag_id)?;
        }
        for cat_id in &new.category_ids {
            categories::link_prompt_category(conn, id, *cat_id)?;
        }
        for col_id in &new.collection_ids {
            collections::link_prompt_collection(conn, id, *col_id)?;
        }

        get_prompt(conn, id)
    })
}

/// Returns the user_id for a prompt, or NotFound if the prompt doesn't exist.
/// Lightweight alternative to loading the full prompt for ownership checks.
///
/// # Errors
///
/// Returns `DbError::Core` with `CoreError::NotFound` if no prompt exists
/// with the given id.
/// Returns `DbError::Query` if the SQL statement fails.
pub fn get_prompt_owner(conn: &rusqlite::Connection, prompt_id: i64) -> Result<i64, DbError> {
    conn.query_row(
        "SELECT user_id FROM prompts WHERE id = ?1",
        params![prompt_id],
        |row| row.get(0),
    )
    .map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => DbError::Core(CoreError::NotFound {
            entity: "Prompt".to_owned(),
            id: prompt_id,
        }),
        other => DbError::Query {
            operation: "get_prompt_owner".to_owned(),
            source: other,
        },
    })
}

/// Returns the distinct non-null language codes used by a user's prompts.
///
/// # Errors
///
/// Returns `DbError::Query` if the SQL statement fails.
pub fn list_prompt_languages(
    conn: &rusqlite::Connection,
    user_id: i64,
) -> Result<Vec<String>, DbError> {
    let mut stmt = conn.prepare(
        "SELECT DISTINCT language FROM prompts WHERE user_id = ?1 AND language IS NOT NULL ORDER BY language"
    ).map_err(|e| DbError::Query { operation: "list_prompt_languages".to_owned(), source: e })?;
    let rows = stmt
        .query_map(params![user_id], |row| row.get(0))
        .map_err(|e| DbError::Query {
            operation: "list_prompt_languages".to_owned(),
            source: e,
        })?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| DbError::Query {
            operation: "list_prompt_languages".to_owned(),
            source: e,
        })
}

/// Retrieves a prompt by primary key without associations.
///
/// # Errors
///
/// Returns `DbError::Core` with `CoreError::NotFound` if no prompt exists
/// with the given id.
/// Returns `DbError::Query` if the SQL statement fails.
pub fn get_prompt(conn: &rusqlite::Connection, id: i64) -> Result<Prompt, DbError> {
    conn.query_row(
        "SELECT id, user_id, title, content, description, notes, \
         language, is_favorite, is_archived, current_version, created_at, updated_at \
         FROM prompts WHERE id = ?1",
        params![id],
        row_to_prompt,
    )
    .map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => DbError::Core(CoreError::NotFound {
            entity: "Prompt".to_owned(),
            id,
        }),
        other => DbError::Query {
            operation: "get_prompt".to_owned(),
            source: other,
        },
    })
}

/// Retrieves a prompt by primary key, enforcing ownership via `user_id`.
/// Returns `NotFound` if the prompt does not exist or does not belong to the
/// specified user.
///
/// # Errors
///
/// Returns `DbError::Core` with `CoreError::NotFound` if no prompt exists
/// with the given id and user_id combination.
/// Returns `DbError::Query` if the SQL statement fails.
pub fn get_prompt_for_user(
    conn: &rusqlite::Connection,
    id: i64,
    user_id: i64,
) -> Result<Prompt, DbError> {
    conn.query_row(
        "SELECT id, user_id, title, content, description, notes, \
         language, is_favorite, is_archived, current_version, created_at, updated_at \
         FROM prompts WHERE id = ?1 AND user_id = ?2",
        params![id, user_id],
        row_to_prompt,
    )
    .map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => DbError::Core(CoreError::NotFound {
            entity: "Prompt".to_owned(),
            id,
        }),
        other => DbError::Query {
            operation: "get_prompt_for_user".to_owned(),
            source: other,
        },
    })
}

/// Performance note: This function executes 4 separate queries (prompt, tags, categories,
/// collections) within a savepoint for atomicity. A single JOIN-based query would reduce
/// the round-trips but would require post-processing to deduplicate rows from the Cartesian
/// product of three many-to-many relationships. The current approach is simpler and
/// sufficient for single-entity detail views. For batch operations (e.g. export), callers
/// should implement batch-loading with WHERE id IN (...) queries instead of looping over
/// this function.
/// Retrieves a prompt with its resolved tags, categories, and collections.
/// The four queries (prompt row, tags, categories, collections) run inside a
/// SAVEPOINT so the reads form an atomic snapshot. Without the savepoint,
/// a concurrent write could modify associations between individual SELECTs,
/// producing an inconsistent aggregate.
///
/// # Errors
///
/// Returns `DbError::Core` with `CoreError::NotFound` if no prompt exists
/// with the given id.
/// Returns `DbError::Query` if the SQL statement fails.
pub fn get_prompt_with_associations(
    conn: &rusqlite::Connection,
    id: i64,
) -> Result<PromptWithAssociations, DbError> {
    super::with_savepoint(conn, "get_prompt_assoc", |conn| {
        let prompt = get_prompt(conn, id)?;
        let prompt_tags = tags::get_tags_for_prompt(conn, id)?;
        let prompt_categories = categories::get_categories_for_prompt(conn, id)?;
        let prompt_collections = collections::get_collections_for_prompt(conn, id)?;

        Ok(PromptWithAssociations {
            prompt,
            tags: prompt_tags,
            categories: prompt_categories,
            collections: prompt_collections,
        })
    })
}

/// Returns the total number of prompts owned by the given user.
///
/// # Errors
///
/// Returns `DbError::Query` if the SQL statement fails.
pub fn count_prompts(conn: &rusqlite::Connection, user_id: i64) -> Result<i64, DbError> {
    conn.query_row(
        "SELECT COUNT(*) FROM prompts WHERE user_id = ?1",
        params![user_id],
        |row| row.get(0),
    )
    .map_err(|e| DbError::Query {
        operation: "count_prompts".to_owned(),
        source: e,
    })
}

/// Returns the count of prompts matching the filter criteria (same WHERE
/// logic as `list_prompts` but without LIMIT/OFFSET).
///
/// # Errors
///
/// Returns `DbError::Query` if the SQL statement fails.
pub fn count_filtered_prompts(
    conn: &rusqlite::Connection,
    filter: &PromptFilter,
) -> Result<i64, DbError> {
    let mut sql = String::from("SELECT COUNT(DISTINCT p.id) FROM prompts p");
    let mut conditions: Vec<String> = Vec::new();
    let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    let mut param_idx = 1;

    if let Some(tag_id) = filter.tag_id {
        let _ = write!(
            sql,
            " INNER JOIN prompt_tags pt ON pt.prompt_id = p.id AND pt.tag_id = ?{param_idx}"
        );
        param_values.push(Box::new(tag_id));
        param_idx += 1;
    }

    if let Some(cat_id) = filter.category_id {
        let _ = write!(
            sql,
            " INNER JOIN prompt_categories pc ON pc.prompt_id = p.id \
             AND pc.category_id = ?{param_idx}"
        );
        param_values.push(Box::new(cat_id));
        param_idx += 1;
    }

    if let Some(col_id) = filter.collection_id {
        let _ = write!(
            sql,
            " INNER JOIN prompt_collections pcol ON pcol.prompt_id = p.id \
             AND pcol.collection_id = ?{param_idx}"
        );
        param_values.push(Box::new(col_id));
        param_idx += 1;
    }

    if let Some(uid) = filter.user_id {
        conditions.push(format!("p.user_id = ?{param_idx}"));
        param_values.push(Box::new(uid));
        param_idx += 1;
    }

    if let Some(fav) = filter.is_favorite {
        conditions.push(format!("p.is_favorite = ?{param_idx}"));
        param_values.push(Box::new(fav));
        param_idx += 1;
    }

    if let Some(arch) = filter.is_archived {
        conditions.push(format!("p.is_archived = ?{param_idx}"));
        param_values.push(Box::new(arch));
        param_idx += 1;
    }

    if let Some(has_vars) = filter.has_variables {
        // The LIKE '%{{%' pattern is a rough approximation of the template variable
        // syntax {{identifier}}. It may produce false positives for content containing
        // literal double-braces (e.g. JSON or Mustache templates). This is an acceptable
        // trade-off since SQL LIKE cannot express the full regex needed for precise matching.
        if has_vars {
            conditions.push("p.content LIKE '%{{%'".to_owned());
        } else {
            conditions.push("p.content NOT LIKE '%{{%'".to_owned());
        }
    }

    if let Some(ref var_name) = filter.variable_name {
        let escaped = var_name.replace('%', r"\%").replace('_', r"\_");
        let pattern = format!("%{{{{{escaped}}}}}%");
        conditions.push(format!("p.content LIKE ?{param_idx} ESCAPE '\\'"));
        param_values.push(Box::new(pattern));
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
            operation: "count_filtered_prompts".to_owned(),
            source: e,
        })
}

/// Performance note: The query selects the full content column for each prompt. For list
/// views that only display a truncated preview, a PromptSummary type with
/// SUBSTR(content, 1, 200) would reduce API payload size. This is tracked as a future
/// optimization.
/// Returns prompts matching the filter criteria, ordered by `updated_at`
/// descending.
///
/// # Errors
///
/// Returns `DbError::Query` if the SQL statement fails.
#[allow(clippy::too_many_lines)]
pub fn list_prompts(
    conn: &rusqlite::Connection,
    filter: &PromptFilter,
) -> Result<Vec<Prompt>, DbError> {
    let mut sql = String::from(
        "SELECT DISTINCT p.id, p.user_id, p.title, p.content, \
         p.description, p.notes, p.language, p.is_favorite, p.is_archived, \
         p.current_version, p.created_at, p.updated_at \
         FROM prompts p",
    );
    let mut conditions: Vec<String> = Vec::new();
    let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    let mut param_idx = 1;

    if let Some(tag_id) = filter.tag_id {
        let _ = write!(
            sql,
            " INNER JOIN prompt_tags pt ON pt.prompt_id = p.id AND pt.tag_id = ?{param_idx}"
        );
        param_values.push(Box::new(tag_id));
        param_idx += 1;
    }

    if let Some(cat_id) = filter.category_id {
        let _ = write!(
            sql,
            " INNER JOIN prompt_categories pc ON pc.prompt_id = p.id \
             AND pc.category_id = ?{param_idx}"
        );
        param_values.push(Box::new(cat_id));
        param_idx += 1;
    }

    if let Some(col_id) = filter.collection_id {
        let _ = write!(
            sql,
            " INNER JOIN prompt_collections pcol ON pcol.prompt_id = p.id \
             AND pcol.collection_id = ?{param_idx}"
        );
        param_values.push(Box::new(col_id));
        param_idx += 1;
    }

    if let Some(uid) = filter.user_id {
        conditions.push(format!("p.user_id = ?{param_idx}"));
        param_values.push(Box::new(uid));
        param_idx += 1;
    }

    if let Some(fav) = filter.is_favorite {
        conditions.push(format!("p.is_favorite = ?{param_idx}"));
        param_values.push(Box::new(fav));
        param_idx += 1;
    }

    if let Some(arch) = filter.is_archived {
        conditions.push(format!("p.is_archived = ?{param_idx}"));
        param_values.push(Box::new(arch));
        param_idx += 1;
    }

    if let Some(has_vars) = filter.has_variables {
        // The LIKE '%{{%' pattern is a rough approximation of the template variable
        // syntax {{identifier}}. It may produce false positives for content containing
        // literal double-braces (e.g. JSON or Mustache templates). This is an acceptable
        // trade-off since SQL LIKE cannot express the full regex needed for precise matching.
        if has_vars {
            conditions.push("p.content LIKE '%{{%'".to_owned());
        } else {
            conditions.push("p.content NOT LIKE '%{{%'".to_owned());
        }
    }

    if let Some(ref var_name) = filter.variable_name {
        let escaped = var_name.replace('%', r"\%").replace('_', r"\_");
        let pattern = format!("%{{{{{escaped}}}}}%");
        conditions.push(format!("p.content LIKE ?{param_idx} ESCAPE '\\'"));
        param_values.push(Box::new(pattern));
        #[allow(unused_assignments)]
        {
            param_idx += 1;
        }
    }

    if !conditions.is_empty() {
        sql.push_str(" WHERE ");
        sql.push_str(&conditions.join(" AND "));
    }

    sql.push_str(" ORDER BY p.updated_at DESC");

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
        operation: "list_prompts".to_owned(),
        source: e,
    })?;

    let params_ref: Vec<&dyn rusqlite::types::ToSql> =
        param_values.iter().map(AsRef::as_ref).collect();
    let rows = stmt
        .query_map(params_ref.as_slice(), row_to_prompt)
        .map_err(|e| DbError::Query {
            operation: "list_prompts".to_owned(),
            source: e,
        })?;

    let mut result = Vec::new();
    for row in rows {
        result.push(row.map_err(|e| DbError::Query {
            operation: "list_prompts".to_owned(),
            source: e,
        })?);
    }
    Ok(result)
}

/// Returns all prompts for a user without pagination limits.
/// Used by export and bulk-copy flows that must enumerate every record.
///
/// # Errors
///
/// Returns `DbError::Query` if the SQL statement fails.
pub fn list_all_prompts(conn: &rusqlite::Connection, user_id: i64) -> Result<Vec<Prompt>, DbError> {
    let sql = "SELECT id, user_id, title, content, description, notes, language, \
               is_favorite, is_archived, current_version, created_at, updated_at \
               FROM prompts WHERE user_id = ?1 ORDER BY updated_at DESC";
    let mut stmt = conn.prepare(sql).map_err(|e| DbError::Query {
        operation: "list_all_prompts".to_owned(),
        source: e,
    })?;
    let rows = stmt
        .query_map(params![user_id], row_to_prompt)
        .map_err(|e| DbError::Query {
            operation: "list_all_prompts".to_owned(),
            source: e,
        })?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| DbError::Query {
            operation: "list_all_prompts".to_owned(),
            source: e,
        })
}

/// Updates specific fields on a prompt. Only supplied values are changed.
/// Increments `current_version` and updates `updated_at` timestamp.
///
/// When `expected_version` is `Some(v)`, the update only succeeds if
/// `current_version` still equals `v`. Returns `CoreError::Conflict`
/// if the version has changed (optimistic concurrency control -- F10).
///
/// After the mutation, the prompt is re-fetched from the database to return the
/// complete row with server-generated values (id, created_at, updated_at, current_version).
/// This pattern ensures the returned Prompt struct matches the persisted state exactly.
///
/// # Errors
///
/// Returns `DbError::Core` with `CoreError::Conflict` if `expected_version`
/// is provided and does not match the current version.
/// Returns `DbError::Core` with `CoreError::NotFound` if no prompt exists
/// with the given id.
/// Returns `DbError::Query` if the SQL statement fails.
#[allow(clippy::too_many_arguments)]
pub fn update_prompt_fields(
    conn: &rusqlite::Connection,
    prompt_id: i64,
    title: Option<&str>,
    content: Option<&str>,
    description: Option<Option<&str>>,
    notes: Option<Option<&str>>,
    language: Option<Option<&str>>,
    expected_version: Option<i64>,
) -> Result<Prompt, DbError> {
    // If no content fields are provided, return the existing prompt without
    // incrementing the version number.
    if title.is_none()
        && content.is_none()
        && description.is_none()
        && notes.is_none()
        && language.is_none()
    {
        return get_prompt(conn, prompt_id);
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
    if let Some(v) = language {
        set_clauses.push(format!("language = ?{param_idx}"));
        param_values.push(Box::new(v.map(str::to_owned)));
        param_idx += 1;
    }

    // Always increment version and update timestamp.
    set_clauses.push("current_version = current_version + 1".to_owned());
    set_clauses.push("updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')".to_owned());

    // F10: Optionally include a version predicate for optimistic concurrency.
    let mut where_clause = format!("WHERE id = ?{param_idx}");
    param_values.push(Box::new(prompt_id));
    param_idx += 1;

    if let Some(ev) = expected_version {
        use std::fmt::Write;
        let _ = write!(where_clause, " AND current_version = ?{param_idx}");
        param_values.push(Box::new(ev));
    }

    let sql = format!(
        "UPDATE prompts SET {} {where_clause}",
        set_clauses.join(", ")
    );

    let params_ref: Vec<&dyn rusqlite::types::ToSql> =
        param_values.iter().map(AsRef::as_ref).collect();
    let affected = conn
        .execute(&sql, params_ref.as_slice())
        .map_err(|e| DbError::Query {
            operation: "update_prompt_fields".to_owned(),
            source: e,
        })?;

    if affected == 0 {
        // Distinguish "not found" from "version conflict".
        if expected_version.is_some() {
            if let Ok(existing) = get_prompt(conn, prompt_id) {
                return Err(DbError::Core(CoreError::Conflict {
                    entity: "Prompt".to_owned(),
                    id: prompt_id,
                    expected: expected_version.unwrap_or(0),
                    actual: existing.current_version,
                }));
            }
        }
        return Err(DbError::Core(CoreError::NotFound {
            entity: "Prompt".to_owned(),
            id: prompt_id,
        }));
    }

    // Re-fetch the prompt to capture the server-generated updated_at timestamp
    // and the incremented current_version value.
    get_prompt(conn, prompt_id)
}

/// Directly sets the `current_version` counter on a prompt row.
/// Used by import to synchronize version counters after restoring history.
///
/// # Errors
///
/// Returns `DbError::Core` with `CoreError::Validation` if the version number
/// is less than 1.
/// Returns `DbError::Core` with `CoreError::NotFound` if no prompt exists
/// with the given id.
/// Returns `DbError::Query` if the SQL statement fails.
pub fn update_current_version(
    conn: &rusqlite::Connection,
    prompt_id: i64,
    version: i64,
) -> Result<(), DbError> {
    // Reject version numbers below 1 since prompts start at version 1.
    if version < 1 {
        return Err(DbError::Core(neuronprompter_core::CoreError::Validation {
            field: "current_version".to_owned(),
            message: "version number must be at least 1".to_owned(),
        }));
    }

    let affected = conn
        .execute(
            "UPDATE prompts SET current_version = ?1 WHERE id = ?2",
            rusqlite::params![version, prompt_id],
        )
        .map_err(|e| DbError::Query {
            operation: "update_current_version".to_owned(),
            source: e,
        })?;
    if affected == 0 {
        return Err(DbError::Core(CoreError::NotFound {
            entity: "Prompt".to_owned(),
            id: prompt_id,
        }));
    }
    Ok(())
}

/// Deletes a prompt and all associated data (versions, junction rows) via
/// foreign key CASCADE rules.
///
/// # Errors
///
/// Returns `DbError::Core` with `CoreError::NotFound` if no prompt exists
/// with the given id.
/// Returns `DbError::Query` if the SQL statement fails.
pub fn delete_prompt(conn: &rusqlite::Connection, id: i64) -> Result<(), DbError> {
    let affected = conn
        .execute("DELETE FROM prompts WHERE id = ?1", params![id])
        .map_err(|e| DbError::Query {
            operation: "delete_prompt".to_owned(),
            source: e,
        })?;
    if affected == 0 {
        return Err(DbError::Core(CoreError::NotFound {
            entity: "Prompt".to_owned(),
            id,
        }));
    }
    Ok(())
}

/// Sets or clears the favorite flag on a prompt.
///
/// The WHERE clause filters by `id` only, without an additional `user_id`
/// predicate. There is a theoretical TOCTOU gap between the handler-level
/// ownership check (which loads the prompt via `get_prompt` and calls
/// `check_ownership`) and this UPDATE statement. In practice, the handler
/// verifies that `auth.user_id` matches the prompt's `user_id` before calling
/// this function, so an unauthorized toggle cannot occur unless the prompt is
/// deleted and re-created with a different owner between the check and the
/// UPDATE -- a scenario that would simply result in a NotFound error here.
///
/// # Errors
///
/// Returns `DbError::Core` with `CoreError::NotFound` if no prompt exists
/// with the given id.
/// Returns `DbError::Query` if the SQL statement fails.
pub fn set_favorite(
    conn: &rusqlite::Connection,
    id: i64,
    is_favorite: bool,
) -> Result<(), DbError> {
    let affected = conn
        .execute(
            "UPDATE prompts SET is_favorite = ?1, \
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
            entity: "Prompt".to_owned(),
            id,
        }));
    }
    Ok(())
}

/// Sets or clears the archived flag on a prompt.
///
/// The WHERE clause filters by `id` only, without an additional `user_id`
/// predicate. There is a theoretical TOCTOU gap between the handler-level
/// ownership check (which loads the prompt via `get_prompt` and calls
/// `check_ownership`) and this UPDATE statement. In practice, the handler
/// verifies that `auth.user_id` matches the prompt's `user_id` before calling
/// this function, so an unauthorized toggle cannot occur unless the prompt is
/// deleted and re-created with a different owner between the check and the
/// UPDATE -- a scenario that would simply result in a NotFound error here.
///
/// # Errors
///
/// Returns `DbError::Core` with `CoreError::NotFound` if no prompt exists
/// with the given id.
/// Returns `DbError::Query` if the SQL statement fails.
pub fn set_archived(
    conn: &rusqlite::Connection,
    id: i64,
    is_archived: bool,
) -> Result<(), DbError> {
    let affected = conn
        .execute(
            "UPDATE prompts SET is_archived = ?1, \
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
            entity: "Prompt".to_owned(),
            id,
        }));
    }
    Ok(())
}

/// Creates a copy of an existing prompt with version reset to 1 and a
/// modified title indicating it is a duplicate.
///
/// # Errors
///
/// Returns `DbError::Core` with `CoreError::NotFound` if no prompt exists
/// with the given id.
/// Returns `DbError::Query` if the SQL statement fails.
pub fn duplicate_prompt(conn: &rusqlite::Connection, id: i64) -> Result<Prompt, DbError> {
    super::with_savepoint(conn, "dup_prompt", |conn| {
        let original = get_prompt(conn, id)?;
        let suffix = " (copy)";
        let max_base_chars = 200 - suffix.len();
        let base: String = original.title.chars().take(max_base_chars).collect();
        let dup_title = format!("{base}{suffix}");

        conn.execute(
            "INSERT INTO prompts (user_id, title, content, description, \
             notes, language) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                original.user_id,
                dup_title,
                original.content,
                original.description,
                original.notes,
                original.language,
            ],
        )
        .map_err(|e| DbError::Query {
            operation: "duplicate_prompt".to_owned(),
            source: e,
        })?;
        let new_id = conn.last_insert_rowid();

        // Copy junction-table associations from the original prompt.
        conn.execute(
            "INSERT INTO prompt_tags (prompt_id, tag_id) \
             SELECT ?1, tag_id FROM prompt_tags WHERE prompt_id = ?2",
            params![new_id, id],
        )
        .map_err(|e| DbError::Query {
            operation: "duplicate_prompt_tags".to_owned(),
            source: e,
        })?;
        conn.execute(
            "INSERT INTO prompt_categories (prompt_id, category_id) \
             SELECT ?1, category_id FROM prompt_categories WHERE prompt_id = ?2",
            params![new_id, id],
        )
        .map_err(|e| DbError::Query {
            operation: "duplicate_prompt_categories".to_owned(),
            source: e,
        })?;
        conn.execute(
            "INSERT INTO prompt_collections (prompt_id, collection_id) \
             SELECT ?1, collection_id FROM prompt_collections WHERE prompt_id = ?2",
            params![new_id, id],
        )
        .map_err(|e| DbError::Query {
            operation: "duplicate_prompt_collections".to_owned(),
            source: e,
        })?;

        get_prompt(conn, new_id)
    })
}

/// Copies an existing prompt to a different user with pre-resolved taxonomy
/// IDs and a caller-supplied title. Resets version to 1, clears
/// favorite/archive flags. Does not copy version history.
///
/// # Errors
///
/// Returns `DbError::Core` with `CoreError::NotFound` if no prompt exists
/// with the given source id.
/// Returns `DbError::Query` if the SQL statement fails.
pub fn copy_prompt_to_user(
    conn: &rusqlite::Connection,
    source_id: i64,
    target_user_id: i64,
    new_title: &str,
    tag_ids: &[i64],
    category_ids: &[i64],
    collection_ids: &[i64],
) -> Result<Prompt, DbError> {
    super::with_savepoint(conn, "copy_prompt_to_user", |conn| {
        let original = get_prompt(conn, source_id)?;

        conn.execute(
            "INSERT INTO prompts (user_id, title, content, description, \
             notes, language) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                target_user_id,
                new_title,
                original.content,
                original.description,
                original.notes,
                original.language,
            ],
        )
        .map_err(|e| DbError::Query {
            operation: "copy_prompt_to_user".to_owned(),
            source: e,
        })?;
        let new_id = conn.last_insert_rowid();

        for tag_id in tag_ids {
            super::tags::link_prompt_tag(conn, new_id, *tag_id)?;
        }
        for cat_id in category_ids {
            super::categories::link_prompt_category(conn, new_id, *cat_id)?;
        }
        for col_id in collection_ids {
            super::collections::link_prompt_collection(conn, new_id, *col_id)?;
        }

        get_prompt(conn, new_id)
    })
}

/// Performance note: This query filters by user_id + title (+ content). The existing
/// idx_prompts_user index covers (user_id) but the title comparison requires a table scan
/// within the users prompts. A composite index on (user_id, title) would improve
/// deduplication query performance. This is tracked as a future schema migration.
/// Finds a prompt by title and content for the given user. Used for
/// deduplication during cross-user copy operations.
///
/// # Errors
///
/// Returns `DbError::Query` if the SQL statement fails.
pub fn find_prompt_by_title_and_content(
    conn: &rusqlite::Connection,
    user_id: i64,
    title: &str,
    content: &str,
) -> Result<Option<Prompt>, DbError> {
    let result = conn.query_row(
        "SELECT id, user_id, title, content, description, notes, \
         language, is_favorite, is_archived, current_version, created_at, updated_at \
         FROM prompts WHERE user_id = ?1 AND title = ?2 AND content = ?3",
        params![user_id, title, content],
        row_to_prompt,
    );
    match result {
        Ok(p) => Ok(Some(p)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(DbError::Query {
            operation: "find_prompt_by_title_and_content".to_owned(),
            source: e,
        }),
    }
}

/// Checks whether the given user already owns a prompt with the specified title.
///
/// # Errors
///
/// Returns `DbError::Query` if the SQL statement fails.
pub fn prompt_title_exists(
    conn: &rusqlite::Connection,
    user_id: i64,
    title: &str,
) -> Result<bool, DbError> {
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM prompts WHERE user_id = ?1 AND title = ?2",
            params![user_id, title],
            |row| row.get(0),
        )
        .map_err(|e| DbError::Query {
            operation: "prompt_title_exists".to_owned(),
            source: e,
        })?;
    Ok(count > 0)
}

/// Maps a `rusqlite` row to a `Prompt` struct. Column names must match the
/// SELECT statements used throughout this module.
pub(crate) fn row_to_prompt(row: &rusqlite::Row<'_>) -> rusqlite::Result<Prompt> {
    Ok(Prompt {
        id: row.get("id")?,
        user_id: row.get("user_id")?,
        title: row.get("title")?,
        content: row.get("content")?,
        description: row.get("description")?,
        notes: row.get("notes")?,
        language: row.get("language")?,
        is_favorite: row.get("is_favorite")?,
        is_archived: row.get("is_archived")?,
        current_version: row.get("current_version")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
    })
}
