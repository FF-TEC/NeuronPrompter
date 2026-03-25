// =============================================================================
// Chain repository operations.
//
// Provides CRUD functions for the chains and chain_steps tables. Handles chain
// creation, retrieval (with resolved steps and associations), updates, deletion,
// favorite/archive toggling, duplication, composed content generation, and
// chain-prompt reference lookups.
// =============================================================================

use std::fmt::Write;

use neuronprompter_core::CoreError;
use neuronprompter_core::domain::chain::{
    Chain, ChainFilter, ChainStep, ChainStepInput, ChainWithSteps, NewChain, ResolvedChainStep,
    StepType,
};
use neuronprompter_core::domain::prompt::Prompt;
use neuronprompter_core::domain::script::Script;
use rusqlite::params;

use crate::DbError;

// ---------------------------------------------------------------------------
// Create
// ---------------------------------------------------------------------------

/// Inserts a chain record, creates ordered steps, and links initial taxonomy
/// associations. Returns the persisted chain.
///
/// # Errors
///
/// Returns `DbError::Query` if the SQL statement fails.
pub fn create_chain(conn: &rusqlite::Connection, new: &NewChain) -> Result<Chain, DbError> {
    super::with_savepoint(conn, "create_chain", |conn| {
        let separator = new.separator.as_deref().unwrap_or("\n\n");

        conn.execute(
            "INSERT INTO chains (user_id, title, description, notes, \
             language, separator) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                new.user_id,
                new.title,
                new.description,
                new.notes,
                new.language,
                separator,
            ],
        )
        .map_err(|e| DbError::Query {
            operation: "create_chain".to_owned(),
            source: e,
        })?;
        let chain_id = conn.last_insert_rowid();

        // Insert ordered steps. The `steps` field takes precedence over legacy
        // `prompt_ids` when both are provided.
        let resolved_steps = resolve_step_inputs(new);
        for (position, step_input) in resolved_steps.iter().enumerate() {
            let (prompt_id, script_id) = match step_input.step_type {
                StepType::Prompt => (Some(step_input.item_id), None),
                StepType::Script => (None, Some(step_input.item_id)),
            };
            insert_step_polymorphic(
                conn,
                chain_id,
                step_input.step_type.as_str(),
                prompt_id,
                script_id,
                i32::try_from(position).unwrap_or(i32::MAX),
            )?;
        }

        // Link taxonomy associations.
        for tag_id in &new.tag_ids {
            link_chain_tag(conn, chain_id, *tag_id)?;
        }
        for cat_id in &new.category_ids {
            link_chain_category(conn, chain_id, *cat_id)?;
        }
        for col_id in &new.collection_ids {
            link_chain_collection(conn, chain_id, *col_id)?;
        }

        get_chain(conn, chain_id)
    })
}

// ---------------------------------------------------------------------------
// Read
// ---------------------------------------------------------------------------

/// Returns the user_id for a chain, or NotFound if the chain doesn't exist.
/// Lightweight alternative to loading the full chain for ownership checks.
///
/// # Errors
///
/// Returns `DbError::Core` with `CoreError::NotFound` if no chain exists
/// with the given id.
/// Returns `DbError::Query` if the SQL statement fails.
pub fn get_chain_owner(conn: &rusqlite::Connection, chain_id: i64) -> Result<i64, DbError> {
    conn.query_row(
        "SELECT user_id FROM chains WHERE id = ?1",
        params![chain_id],
        |row| row.get(0),
    )
    .map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => DbError::Core(CoreError::NotFound {
            entity: "Chain".to_owned(),
            id: chain_id,
        }),
        other => DbError::Query {
            operation: "get_chain_owner".to_owned(),
            source: other,
        },
    })
}

/// Retrieves a chain by primary key without steps or associations.
///
/// # Errors
///
/// Returns `DbError::Core` with `CoreError::NotFound` if no chain exists
/// with the given id.
/// Returns `DbError::Query` if the SQL statement fails.
pub fn get_chain(conn: &rusqlite::Connection, id: i64) -> Result<Chain, DbError> {
    conn.query_row(
        "SELECT id, user_id, title, description, notes, language, \
         separator, is_favorite, is_archived, created_at, updated_at \
         FROM chains WHERE id = ?1",
        params![id],
        row_to_chain,
    )
    .map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => DbError::Core(CoreError::NotFound {
            entity: "Chain".to_owned(),
            id,
        }),
        other => DbError::Query {
            operation: "get_chain".to_owned(),
            source: other,
        },
    })
}

/// Retrieves a chain by primary key, enforcing ownership via `user_id`.
/// Returns `NotFound` if the chain does not exist or does not belong to the
/// specified user.
///
/// # Errors
///
/// Returns `DbError::Core` with `CoreError::NotFound` if no chain exists
/// with the given id and user_id combination.
/// Returns `DbError::Query` if the SQL statement fails.
pub fn get_chain_for_user(
    conn: &rusqlite::Connection,
    id: i64,
    user_id: i64,
) -> Result<Chain, DbError> {
    conn.query_row(
        "SELECT id, user_id, title, description, notes, language, \
         separator, is_favorite, is_archived, created_at, updated_at \
         FROM chains WHERE id = ?1 AND user_id = ?2",
        params![id, user_id],
        row_to_chain,
    )
    .map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => DbError::Core(CoreError::NotFound {
            entity: "Chain".to_owned(),
            id,
        }),
        other => DbError::Query {
            operation: "get_chain_for_user".to_owned(),
            source: other,
        },
    })
}

/// Performance note: This function executes 5 separate queries (chain, steps, tags,
/// categories, collections) within a savepoint for atomicity. A single JOIN-based query
/// would reduce the round-trips but would require post-processing to deduplicate rows from
/// the Cartesian product of steps and three many-to-many relationships. The current approach
/// is simpler and sufficient for single-entity detail views. For batch operations, callers
/// should implement batch-loading with WHERE id IN (...) queries instead of looping over
/// this function.
/// Retrieves a chain with its resolved steps (including prompt data) and
/// taxonomy associations. The five queries (chain row, steps, tags, categories,
/// collections) run inside a SAVEPOINT so the reads form an atomic snapshot.
/// Without the savepoint, a concurrent write could modify steps or associations
/// between individual SELECTs, producing an inconsistent aggregate.
///
/// # Errors
///
/// Returns `DbError::Core` with `CoreError::NotFound` if no chain exists
/// with the given id.
/// Returns `DbError::Query` if the SQL statement fails.
pub fn get_chain_with_steps(
    conn: &rusqlite::Connection,
    id: i64,
) -> Result<ChainWithSteps, DbError> {
    super::with_savepoint(conn, "get_chain_steps", |conn| {
        let chain = get_chain(conn, id)?;
        let steps = get_resolved_steps(conn, id)?;
        let chain_tags = get_tags_for_chain(conn, id)?;
        let chain_categories = get_categories_for_chain(conn, id)?;
        let chain_collections = get_collections_for_chain(conn, id)?;

        Ok(ChainWithSteps {
            chain,
            steps,
            tags: chain_tags,
            categories: chain_categories,
            collections: chain_collections,
        })
    })
}

/// Returns chains matching the filter criteria, ordered by `updated_at` descending.
/// Returns the total number of chains owned by the given user.
///
/// # Errors
///
/// Returns `DbError::Query` if the SQL statement fails.
pub fn count_chains(conn: &rusqlite::Connection, user_id: i64) -> Result<i64, DbError> {
    conn.query_row(
        "SELECT COUNT(*) FROM chains WHERE user_id = ?1",
        params![user_id],
        |row| row.get(0),
    )
    .map_err(|e| DbError::Query {
        operation: "count_chains".to_owned(),
        source: e,
    })
}

/// Returns the count of chains matching the filter criteria (same WHERE
/// logic as `list_chains` but without LIMIT/OFFSET).
///
/// # Errors
///
/// Returns `DbError::Query` if the SQL statement fails.
pub fn count_filtered_chains(
    conn: &rusqlite::Connection,
    filter: &ChainFilter,
) -> Result<i64, DbError> {
    let mut sql = String::from("SELECT COUNT(DISTINCT c.id) FROM chains c");
    let mut conditions: Vec<String> = Vec::new();
    let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    let mut param_idx = 1;

    if let Some(tag_id) = filter.tag_id {
        let _ = write!(
            sql,
            " INNER JOIN chain_tags ct ON ct.chain_id = c.id AND ct.tag_id = ?{param_idx}"
        );
        param_values.push(Box::new(tag_id));
        param_idx += 1;
    }

    if let Some(cat_id) = filter.category_id {
        let _ = write!(
            sql,
            " INNER JOIN chain_categories cc ON cc.chain_id = c.id \
             AND cc.category_id = ?{param_idx}"
        );
        param_values.push(Box::new(cat_id));
        param_idx += 1;
    }

    if let Some(col_id) = filter.collection_id {
        let _ = write!(
            sql,
            " INNER JOIN chain_collections ccol ON ccol.chain_id = c.id \
             AND ccol.collection_id = ?{param_idx}"
        );
        param_values.push(Box::new(col_id));
        param_idx += 1;
    }

    if let Some(uid) = filter.user_id {
        conditions.push(format!("c.user_id = ?{param_idx}"));
        param_values.push(Box::new(uid));
        param_idx += 1;
    }

    if let Some(fav) = filter.is_favorite {
        conditions.push(format!("c.is_favorite = ?{param_idx}"));
        param_values.push(Box::new(fav));
        param_idx += 1;
    }

    if let Some(arch) = filter.is_archived {
        conditions.push(format!("c.is_archived = ?{param_idx}"));
        param_values.push(Box::new(arch));
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
            operation: "count_filtered_chains".to_owned(),
            source: e,
        })
}

/// Returns chains matching the filter criteria, ordered by `updated_at`
/// descending with pagination.
///
/// # Errors
///
/// Returns `DbError::Query` if the SQL statement fails.
pub fn list_chains(
    conn: &rusqlite::Connection,
    filter: &ChainFilter,
) -> Result<Vec<Chain>, DbError> {
    let mut sql = String::from(
        "SELECT DISTINCT c.id, c.user_id, c.title, c.description, \
         c.notes, c.language, c.separator, c.is_favorite, c.is_archived, \
         c.created_at, c.updated_at \
         FROM chains c",
    );
    let mut conditions: Vec<String> = Vec::new();
    let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    let mut param_idx = 1;

    if let Some(tag_id) = filter.tag_id {
        let _ = write!(
            sql,
            " INNER JOIN chain_tags ct ON ct.chain_id = c.id AND ct.tag_id = ?{param_idx}"
        );
        param_values.push(Box::new(tag_id));
        param_idx += 1;
    }

    if let Some(cat_id) = filter.category_id {
        let _ = write!(
            sql,
            " INNER JOIN chain_categories cc ON cc.chain_id = c.id \
             AND cc.category_id = ?{param_idx}"
        );
        param_values.push(Box::new(cat_id));
        param_idx += 1;
    }

    if let Some(col_id) = filter.collection_id {
        let _ = write!(
            sql,
            " INNER JOIN chain_collections ccol ON ccol.chain_id = c.id \
             AND ccol.collection_id = ?{param_idx}"
        );
        param_values.push(Box::new(col_id));
        param_idx += 1;
    }

    if let Some(uid) = filter.user_id {
        conditions.push(format!("c.user_id = ?{param_idx}"));
        param_values.push(Box::new(uid));
        param_idx += 1;
    }

    if let Some(fav) = filter.is_favorite {
        conditions.push(format!("c.is_favorite = ?{param_idx}"));
        param_values.push(Box::new(fav));
        param_idx += 1;
    }

    if let Some(arch) = filter.is_archived {
        conditions.push(format!("c.is_archived = ?{param_idx}"));
        param_values.push(Box::new(arch));
        param_idx += 1;
    }

    if !conditions.is_empty() {
        sql.push_str(" WHERE ");
        sql.push_str(&conditions.join(" AND "));
    }

    sql.push_str(" ORDER BY c.updated_at DESC");

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
        operation: "list_chains".to_owned(),
        source: e,
    })?;

    let params_ref: Vec<&dyn rusqlite::types::ToSql> =
        param_values.iter().map(AsRef::as_ref).collect();
    let rows = stmt
        .query_map(params_ref.as_slice(), row_to_chain)
        .map_err(|e| DbError::Query {
            operation: "list_chains".to_owned(),
            source: e,
        })?;

    let mut result = Vec::new();
    for row in rows {
        result.push(row.map_err(|e| DbError::Query {
            operation: "list_chains".to_owned(),
            source: e,
        })?);
    }
    Ok(result)
}

/// Returns all chains for a user without pagination limits.
/// Used by export and bulk-copy flows that must enumerate every record.
///
/// # Errors
///
/// Returns `DbError::Query` if the SQL statement fails.
pub fn list_all_chains(conn: &rusqlite::Connection, user_id: i64) -> Result<Vec<Chain>, DbError> {
    let sql = "SELECT id, user_id, title, description, notes, language, \
               separator, is_favorite, is_archived, created_at, updated_at \
               FROM chains WHERE user_id = ?1 ORDER BY updated_at DESC";
    let mut stmt = conn.prepare(sql).map_err(|e| DbError::Query {
        operation: "list_all_chains".to_owned(),
        source: e,
    })?;
    let rows = stmt
        .query_map(params![user_id], row_to_chain)
        .map_err(|e| DbError::Query {
            operation: "list_all_chains".to_owned(),
            source: e,
        })?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| DbError::Query {
            operation: "list_all_chains".to_owned(),
            source: e,
        })
}

// ---------------------------------------------------------------------------
// Update
// ---------------------------------------------------------------------------

/// Updates specific metadata fields on a chain. Only supplied values are changed.
/// Always updates the `updated_at` timestamp.
///
/// # Errors
///
/// Returns `DbError::Core` with `CoreError::NotFound` if no chain exists
/// with the given id.
/// Returns `DbError::Query` if the SQL statement fails.
#[allow(clippy::too_many_arguments)]
pub fn update_chain_fields(
    conn: &rusqlite::Connection,
    chain_id: i64,
    title: Option<&str>,
    description: Option<Option<&str>>,
    notes: Option<Option<&str>>,
    language: Option<Option<&str>>,
    separator: Option<&str>,
) -> Result<Chain, DbError> {
    // If no content fields are provided, return the existing chain without
    // updating the timestamp.
    if title.is_none()
        && description.is_none()
        && notes.is_none()
        && language.is_none()
        && separator.is_none()
    {
        return get_chain(conn, chain_id);
    }

    let mut set_clauses: Vec<String> = Vec::new();
    let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    let mut param_idx = 1;

    if let Some(v) = title {
        set_clauses.push(format!("title = ?{param_idx}"));
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
    if let Some(v) = separator {
        set_clauses.push(format!("separator = ?{param_idx}"));
        param_values.push(Box::new(v.to_owned()));
        param_idx += 1;
    }

    // Always update timestamp.
    set_clauses.push("updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')".to_owned());

    let sql = format!(
        "UPDATE chains SET {} WHERE id = ?{param_idx}",
        set_clauses.join(", ")
    );
    param_values.push(Box::new(chain_id));

    let params_ref: Vec<&dyn rusqlite::types::ToSql> =
        param_values.iter().map(AsRef::as_ref).collect();
    let affected = conn
        .execute(&sql, params_ref.as_slice())
        .map_err(|e| DbError::Query {
            operation: "update_chain_fields".to_owned(),
            source: e,
        })?;

    if affected == 0 {
        return Err(DbError::Core(CoreError::NotFound {
            entity: "Chain".to_owned(),
            id: chain_id,
        }));
    }

    get_chain(conn, chain_id)
}

/// Replaces all steps in a chain with a new ordered list of prompt IDs.
/// Legacy convenience wrapper -- delegates to `replace_chain_steps_mixed`.
///
/// # Errors
///
/// Returns `DbError::Query` if the SQL statement fails.
pub fn replace_chain_steps(
    conn: &rusqlite::Connection,
    chain_id: i64,
    prompt_ids: &[i64],
) -> Result<(), DbError> {
    let steps: Vec<ChainStepInput> = prompt_ids
        .iter()
        .map(|id| ChainStepInput {
            step_type: StepType::Prompt,
            item_id: *id,
        })
        .collect();
    replace_chain_steps_mixed(conn, chain_id, &steps)
}

/// Replaces all steps in a chain with a mixed sequence of prompt and script
/// references. Deletes existing steps and inserts fresh ones with positions 0..N.
///
/// # Errors
///
/// Returns `DbError::Query` if the SQL statement fails.
pub fn replace_chain_steps_mixed(
    conn: &rusqlite::Connection,
    chain_id: i64,
    steps: &[ChainStepInput],
) -> Result<(), DbError> {
    super::with_savepoint(conn, "replace_steps", |conn| {
        conn.execute(
            "DELETE FROM chain_steps WHERE chain_id = ?1",
            params![chain_id],
        )
        .map_err(|e| DbError::Query {
            operation: "replace_chain_steps_delete".to_owned(),
            source: e,
        })?;

        for (position, step) in steps.iter().enumerate() {
            let (prompt_id, script_id) = match step.step_type {
                StepType::Prompt => (Some(step.item_id), None),
                StepType::Script => (None, Some(step.item_id)),
            };
            insert_step_polymorphic(
                conn,
                chain_id,
                step.step_type.as_str(),
                prompt_id,
                script_id,
                i32::try_from(position).unwrap_or(i32::MAX),
            )?;
        }

        // Touch updated_at.
        conn.execute(
            "UPDATE chains SET updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now') WHERE id = ?1",
            params![chain_id],
        )
        .map_err(|e| DbError::Query {
            operation: "replace_chain_steps_touch".to_owned(),
            source: e,
        })?;

        Ok(())
    })
}

// ---------------------------------------------------------------------------
// Delete
// ---------------------------------------------------------------------------

/// Deletes a chain and all associated data (steps, junction rows) via CASCADE.
///
/// # Errors
///
/// Returns `DbError::Core` with `CoreError::NotFound` if no chain exists
/// with the given id.
/// Returns `DbError::Query` if the SQL statement fails.
pub fn delete_chain(conn: &rusqlite::Connection, id: i64) -> Result<(), DbError> {
    let affected = conn
        .execute("DELETE FROM chains WHERE id = ?1", params![id])
        .map_err(|e| DbError::Query {
            operation: "delete_chain".to_owned(),
            source: e,
        })?;
    if affected == 0 {
        return Err(DbError::Core(CoreError::NotFound {
            entity: "Chain".to_owned(),
            id,
        }));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Favorite / Archive
// ---------------------------------------------------------------------------

/// Sets or clears the favorite flag on a chain.
///
/// # Errors
///
/// Returns `DbError::Core` with `CoreError::NotFound` if no chain exists
/// with the given id.
/// Returns `DbError::Query` if the SQL statement fails.
pub fn set_chain_favorite(
    conn: &rusqlite::Connection,
    id: i64,
    is_favorite: bool,
) -> Result<(), DbError> {
    let affected = conn
        .execute(
            "UPDATE chains SET is_favorite = ?1, \
             updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now') \
             WHERE id = ?2",
            params![is_favorite, id],
        )
        .map_err(|e| DbError::Query {
            operation: "set_chain_favorite".to_owned(),
            source: e,
        })?;
    if affected == 0 {
        return Err(DbError::Core(CoreError::NotFound {
            entity: "Chain".to_owned(),
            id,
        }));
    }
    Ok(())
}

/// Sets or clears the archived flag on a chain.
///
/// # Errors
///
/// Returns `DbError::Core` with `CoreError::NotFound` if no chain exists
/// with the given id.
/// Returns `DbError::Query` if the SQL statement fails.
pub fn set_chain_archived(
    conn: &rusqlite::Connection,
    id: i64,
    is_archived: bool,
) -> Result<(), DbError> {
    let affected = conn
        .execute(
            "UPDATE chains SET is_archived = ?1, \
             updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now') \
             WHERE id = ?2",
            params![is_archived, id],
        )
        .map_err(|e| DbError::Query {
            operation: "set_chain_archived".to_owned(),
            source: e,
        })?;
    if affected == 0 {
        return Err(DbError::Core(CoreError::NotFound {
            entity: "Chain".to_owned(),
            id,
        }));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Duplicate
// ---------------------------------------------------------------------------

/// Creates a copy of an existing chain with a modified title, copying all
/// steps and taxonomy associations.
///
/// # Errors
///
/// Returns `DbError::Core` with `CoreError::NotFound` if no chain exists
/// with the given id.
/// Returns `DbError::Query` if the SQL statement fails.
pub fn duplicate_chain(conn: &rusqlite::Connection, id: i64) -> Result<Chain, DbError> {
    super::with_savepoint(conn, "dup_chain", |conn| {
        let original = get_chain(conn, id)?;
        let suffix = " (copy)";
        let max_base_chars = 200 - suffix.len();
        let base: String = original.title.chars().take(max_base_chars).collect();
        let dup_title = format!("{base}{suffix}");

        conn.execute(
            "INSERT INTO chains (user_id, title, description, notes, \
             language, separator) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                original.user_id,
                dup_title,
                original.description,
                original.notes,
                original.language,
                original.separator,
            ],
        )
        .map_err(|e| DbError::Query {
            operation: "duplicate_chain".to_owned(),
            source: e,
        })?;
        let new_id = conn.last_insert_rowid();

        // Copy steps preserving order and polymorphic type.
        conn.execute(
            "INSERT INTO chain_steps (chain_id, step_type, prompt_id, script_id, position) \
             SELECT ?1, step_type, prompt_id, script_id, position FROM chain_steps WHERE chain_id = ?2 \
             ORDER BY position",
            params![new_id, id],
        )
        .map_err(|e| DbError::Query {
            operation: "duplicate_chain_steps".to_owned(),
            source: e,
        })?;

        // Copy junction-table associations.
        conn.execute(
            "INSERT INTO chain_tags (chain_id, tag_id) \
             SELECT ?1, tag_id FROM chain_tags WHERE chain_id = ?2",
            params![new_id, id],
        )
        .map_err(|e| DbError::Query {
            operation: "duplicate_chain_tags".to_owned(),
            source: e,
        })?;
        conn.execute(
            "INSERT INTO chain_categories (chain_id, category_id) \
             SELECT ?1, category_id FROM chain_categories WHERE chain_id = ?2",
            params![new_id, id],
        )
        .map_err(|e| DbError::Query {
            operation: "duplicate_chain_categories".to_owned(),
            source: e,
        })?;
        conn.execute(
            "INSERT INTO chain_collections (chain_id, collection_id) \
             SELECT ?1, collection_id FROM chain_collections WHERE chain_id = ?2",
            params![new_id, id],
        )
        .map_err(|e| DbError::Query {
            operation: "duplicate_chain_collections".to_owned(),
            source: e,
        })?;

        get_chain(conn, new_id)
    })
}

/// Copies an existing chain to a different user with pre-resolved taxonomy
/// IDs, a caller-supplied title, and remapped steps. Each entry in
/// `step_mapping` is `(step_type, new_item_id)` in position order.
///
/// # Errors
///
/// Returns `DbError::Core` with `CoreError::NotFound` if no chain exists
/// with the given source id.
/// Returns `DbError::Core` with `CoreError::Validation` if a step_mapping
/// entry contains an unsupported step type.
/// Returns `DbError::Query` if the SQL statement fails.
#[allow(clippy::too_many_arguments)]
pub fn copy_chain_to_user(
    conn: &rusqlite::Connection,
    source_id: i64,
    target_user_id: i64,
    new_title: &str,
    tag_ids: &[i64],
    category_ids: &[i64],
    collection_ids: &[i64],
    step_mapping: &[(String, i64)],
) -> Result<Chain, DbError> {
    super::with_savepoint(conn, "copy_chain_to_user", |conn| {
        let original = get_chain(conn, source_id)?;

        conn.execute(
            "INSERT INTO chains (user_id, title, description, notes, \
             language, separator) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                target_user_id,
                new_title,
                original.description,
                original.notes,
                original.language,
                original.separator,
            ],
        )
        .map_err(|e| DbError::Query {
            operation: "copy_chain_to_user".to_owned(),
            source: e,
        })?;
        let new_id = conn.last_insert_rowid();

        // Insert remapped steps in order.
        for (position, (step_type, item_id)) in step_mapping.iter().enumerate() {
            let (prompt_id, script_id) = match step_type.as_str() {
                "prompt" => (Some(*item_id), None),
                "script" => (None, Some(*item_id)),
                other => {
                    return Err(DbError::Core(neuronprompter_core::CoreError::Validation {
                        field: "step_type".to_owned(),
                        message: format!("step type '{other}' is not supported for chain copy"),
                    }));
                }
            };
            let pos = i32::try_from(position).unwrap_or(i32::MAX);
            insert_step_polymorphic(conn, new_id, step_type, prompt_id, script_id, pos)?;
        }

        for tag_id in tag_ids {
            link_chain_tag(conn, new_id, *tag_id)?;
        }
        for cat_id in category_ids {
            link_chain_category(conn, new_id, *cat_id)?;
        }
        for col_id in collection_ids {
            link_chain_collection(conn, new_id, *col_id)?;
        }

        get_chain(conn, new_id)
    })
}

/// Checks whether the given user already owns a chain with the specified title.
///
/// # Errors
///
/// Returns `DbError::Query` if the SQL statement fails.
pub fn chain_title_exists(
    conn: &rusqlite::Connection,
    user_id: i64,
    title: &str,
) -> Result<bool, DbError> {
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM chains WHERE user_id = ?1 AND title = ?2",
            params![user_id, title],
            |row| row.get(0),
        )
        .map_err(|e| DbError::Query {
            operation: "chain_title_exists".to_owned(),
            source: e,
        })?;
    Ok(count > 0)
}

// ---------------------------------------------------------------------------
// Composed Content
// ---------------------------------------------------------------------------

/// Returns the concatenated content of all chain steps, joined by the chain's
/// separator. Steps are ordered by position. This is the "live reference"
/// function -- content always reflects the current state of referenced prompts
/// and scripts.
///
/// # Errors
///
/// Returns `DbError::Core` with `CoreError::NotFound` if no chain exists
/// with the given id.
/// Returns `DbError::Query` if the SQL statement fails.
pub fn get_composed_content(conn: &rusqlite::Connection, chain_id: i64) -> Result<String, DbError> {
    let chain = get_chain(conn, chain_id)?;

    let mut stmt = conn
        .prepare(
            "SELECT COALESCE(p.content, s.content) AS content \
             FROM chain_steps cs \
             LEFT JOIN prompts p ON p.id = cs.prompt_id AND cs.step_type = 'prompt' \
             LEFT JOIN scripts s ON s.id = cs.script_id AND cs.step_type = 'script' \
             WHERE cs.chain_id = ?1 \
             ORDER BY cs.position",
        )
        .map_err(|e| DbError::Query {
            operation: "get_composed_content".to_owned(),
            source: e,
        })?;

    let contents: Vec<String> = stmt
        .query_map(params![chain_id], |row| row.get(0))
        .map_err(|e| DbError::Query {
            operation: "get_composed_content".to_owned(),
            source: e,
        })?
        .collect::<Result<_, _>>()
        .map_err(|e| DbError::Query {
            operation: "get_composed_content".to_owned(),
            source: e,
        })?;

    Ok(contents.join(&chain.separator))
}

// ---------------------------------------------------------------------------
// Chain-Prompt Reference Lookups
// ---------------------------------------------------------------------------

/// Returns all chains that reference a given prompt ID.
///
/// # Errors
///
/// Returns `DbError::Query` if the SQL statement fails.
pub fn get_chains_containing_prompt(
    conn: &rusqlite::Connection,
    prompt_id: i64,
) -> Result<Vec<Chain>, DbError> {
    let mut stmt = conn
        .prepare(
            "SELECT DISTINCT c.id, c.user_id, c.title, c.description, \
             c.notes, c.language, c.separator, c.is_favorite, c.is_archived, \
             c.created_at, c.updated_at \
             FROM chains c \
             INNER JOIN chain_steps cs ON cs.chain_id = c.id \
             WHERE cs.prompt_id = ?1 \
             ORDER BY c.title",
        )
        .map_err(|e| DbError::Query {
            operation: "get_chains_containing_prompt".to_owned(),
            source: e,
        })?;

    let rows = stmt
        .query_map(params![prompt_id], row_to_chain)
        .map_err(|e| DbError::Query {
            operation: "get_chains_containing_prompt".to_owned(),
            source: e,
        })?;

    let mut result = Vec::new();
    for row in rows {
        result.push(row.map_err(|e| DbError::Query {
            operation: "get_chains_containing_prompt".to_owned(),
            source: e,
        })?);
    }
    Ok(result)
}

/// Returns all chains owned by `user_id` that reference a given prompt ID.
/// Returns an empty vec if no matching chains exist.
///
/// # Errors
///
/// Returns `DbError::Query` if the SQL statement fails.
pub fn get_chains_containing_prompt_for_user(
    conn: &rusqlite::Connection,
    user_id: i64,
    prompt_id: i64,
) -> Result<Vec<Chain>, DbError> {
    let mut stmt = conn
        .prepare(
            "SELECT DISTINCT c.id, c.user_id, c.title, c.description, \
             c.notes, c.language, c.separator, c.is_favorite, c.is_archived, \
             c.created_at, c.updated_at \
             FROM chains c \
             INNER JOIN chain_steps cs ON cs.chain_id = c.id \
             WHERE cs.prompt_id = ?1 AND c.user_id = ?2 \
             ORDER BY c.title",
        )
        .map_err(|e| DbError::Query {
            operation: "get_chains_containing_prompt_for_user".to_owned(),
            source: e,
        })?;

    let rows = stmt
        .query_map(params![prompt_id, user_id], row_to_chain)
        .map_err(|e| DbError::Query {
            operation: "get_chains_containing_prompt_for_user".to_owned(),
            source: e,
        })?;

    let mut result = Vec::new();
    for row in rows {
        result.push(row.map_err(|e| DbError::Query {
            operation: "get_chains_containing_prompt_for_user".to_owned(),
            source: e,
        })?);
    }
    Ok(result)
}

/// Returns all chains that reference a given script ID.
///
/// # Errors
///
/// Returns `DbError::Query` if the SQL statement fails.
pub fn get_chains_containing_script(
    conn: &rusqlite::Connection,
    script_id: i64,
) -> Result<Vec<Chain>, DbError> {
    let mut stmt = conn
        .prepare(
            "SELECT DISTINCT c.id, c.user_id, c.title, c.description, \
             c.notes, c.language, c.separator, c.is_favorite, c.is_archived, \
             c.created_at, c.updated_at \
             FROM chains c \
             INNER JOIN chain_steps cs ON cs.chain_id = c.id \
             WHERE cs.script_id = ?1 \
             ORDER BY c.title",
        )
        .map_err(|e| DbError::Query {
            operation: "get_chains_containing_script".to_owned(),
            source: e,
        })?;

    let rows = stmt
        .query_map(params![script_id], row_to_chain)
        .map_err(|e| DbError::Query {
            operation: "get_chains_containing_script".to_owned(),
            source: e,
        })?;

    let mut result = Vec::new();
    for row in rows {
        result.push(row.map_err(|e| DbError::Query {
            operation: "get_chains_containing_script".to_owned(),
            source: e,
        })?);
    }
    Ok(result)
}

/// Counts how many chains reference a given script ID.
///
/// # Errors
///
/// Returns `DbError::Query` if the SQL statement fails.
pub fn count_chains_for_script(
    conn: &rusqlite::Connection,
    script_id: i64,
) -> Result<i64, DbError> {
    conn.query_row(
        "SELECT COUNT(DISTINCT chain_id) FROM chain_steps WHERE script_id = ?1",
        params![script_id],
        |row| row.get(0),
    )
    .map_err(|e| DbError::Query {
        operation: "count_chains_for_script".to_owned(),
        source: e,
    })
}

/// Counts how many chains reference a given prompt ID.
///
/// # Errors
///
/// Returns `DbError::Query` if the SQL statement fails.
pub fn count_chains_for_prompt(
    conn: &rusqlite::Connection,
    prompt_id: i64,
) -> Result<i64, DbError> {
    conn.query_row(
        "SELECT COUNT(DISTINCT chain_id) FROM chain_steps WHERE prompt_id = ?1",
        params![prompt_id],
        |row| row.get(0),
    )
    .map_err(|e| DbError::Query {
        operation: "count_chains_for_prompt".to_owned(),
        source: e,
    })
}

/// Returns the number of steps in a chain.
///
/// # Errors
///
/// Returns `DbError::Query` if the SQL statement fails.
pub fn count_steps(conn: &rusqlite::Connection, chain_id: i64) -> Result<i64, DbError> {
    conn.query_row(
        "SELECT COUNT(*) FROM chain_steps WHERE chain_id = ?1",
        params![chain_id],
        |row| row.get(0),
    )
    .map_err(|e| DbError::Query {
        operation: "count_steps".to_owned(),
        source: e,
    })
}

// ---------------------------------------------------------------------------
// FTS5 Search
// ---------------------------------------------------------------------------

/// Searches chains matching the query string with optional filters.
/// Searches chain metadata only (title, description, notes).
///
/// # Errors
///
/// Returns `DbError::Query` if the SQL statement fails.
pub fn search_chains(
    conn: &rusqlite::Connection,
    user_id: i64,
    query: &str,
    filter: &ChainFilter,
) -> Result<Vec<Chain>, DbError> {
    let fts_query = super::build_fts_query(query);
    if fts_query.is_empty() {
        return Ok(Vec::new());
    }

    let mut sql = String::from(
        "SELECT c.id, c.user_id, c.title, c.description, \
         c.notes, c.language, c.separator, c.is_favorite, c.is_archived, \
         c.created_at, c.updated_at \
         FROM chains c \
         INNER JOIN chains_fts fts ON fts.rowid = c.id \
         WHERE fts.chains_fts MATCH ?1 \
         AND c.user_id = ?2",
    );
    let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    param_values.push(Box::new(fts_query));
    param_values.push(Box::new(user_id));
    let mut param_idx: usize = 3;

    // Exclude archived chains unless explicitly requested.
    let is_archived = filter.is_archived.unwrap_or(false);
    let _ = write!(sql, " AND c.is_archived = ?{param_idx}");
    param_values.push(Box::new(is_archived));
    param_idx += 1;

    if let Some(fav) = filter.is_favorite {
        let _ = write!(sql, " AND c.is_favorite = ?{param_idx}");
        param_values.push(Box::new(fav));
        param_idx += 1;
    }

    if let Some(tag_id) = filter.tag_id {
        let _ = write!(
            sql,
            " AND c.id IN (SELECT chain_id FROM chain_tags WHERE tag_id = ?{param_idx})"
        );
        param_values.push(Box::new(tag_id));
        param_idx += 1;
    }

    if let Some(cat_id) = filter.category_id {
        let _ = write!(
            sql,
            " AND c.id IN (SELECT chain_id FROM chain_categories WHERE category_id = ?{param_idx})"
        );
        param_values.push(Box::new(cat_id));
        param_idx += 1;
    }

    if let Some(col_id) = filter.collection_id {
        let _ = write!(
            sql,
            " AND c.id IN (SELECT chain_id FROM chain_collections WHERE collection_id = ?{param_idx})"
        );
        param_values.push(Box::new(col_id));
        param_idx += 1;
    }

    let effective_limit = filter.limit.unwrap_or(200).clamp(1, 1000);
    let _ = write!(sql, " ORDER BY bm25(chains_fts) LIMIT ?{param_idx}");
    param_values.push(Box::new(effective_limit));
    param_idx += 1;

    let effective_offset = filter.offset.unwrap_or(0).max(0);
    if effective_offset > 0 {
        let _ = write!(sql, " OFFSET ?{param_idx}");
        param_values.push(Box::new(effective_offset));
    }

    let mut stmt = conn.prepare(&sql).map_err(|e| DbError::Query {
        operation: "search_chains".to_owned(),
        source: e,
    })?;

    let params_ref: Vec<&dyn rusqlite::types::ToSql> =
        param_values.iter().map(AsRef::as_ref).collect();
    let rows = stmt
        .query_map(params_ref.as_slice(), row_to_chain)
        .map_err(|e| DbError::Query {
            operation: "search_chains".to_owned(),
            source: e,
        })?;

    let mut result = Vec::new();
    for row in rows {
        result.push(row.map_err(|e| DbError::Query {
            operation: "search_chains".to_owned(),
            source: e,
        })?);
    }
    Ok(result)
}

// ---------------------------------------------------------------------------
// Chain Taxonomy Junction Table Operations
// ---------------------------------------------------------------------------

/// Returns tags associated with a chain via the `chain_tags` junction table.
///
/// # Errors
///
/// Returns `DbError::Query` if the SQL statement fails.
pub fn get_tags_for_chain(
    conn: &rusqlite::Connection,
    chain_id: i64,
) -> Result<Vec<neuronprompter_core::domain::tag::Tag>, DbError> {
    let mut stmt = conn
        .prepare(
            "SELECT t.id, t.user_id, t.name, t.created_at \
             FROM tags t \
             INNER JOIN chain_tags ct ON ct.tag_id = t.id \
             WHERE ct.chain_id = ?1 \
             ORDER BY t.name",
        )
        .map_err(|e| DbError::Query {
            operation: "get_tags_for_chain".to_owned(),
            source: e,
        })?;
    let rows = stmt
        .query_map(params![chain_id], |row| {
            Ok(neuronprompter_core::domain::tag::Tag {
                id: row.get(0)?,
                user_id: row.get(1)?,
                name: row.get(2)?,
                created_at: row.get(3)?,
            })
        })
        .map_err(|e| DbError::Query {
            operation: "get_tags_for_chain".to_owned(),
            source: e,
        })?;
    let mut result = Vec::new();
    for row in rows {
        result.push(row.map_err(|e| DbError::Query {
            operation: "get_tags_for_chain".to_owned(),
            source: e,
        })?);
    }
    Ok(result)
}

/// Returns categories associated with a chain.
///
/// # Errors
///
/// Returns `DbError::Query` if the SQL statement fails.
pub fn get_categories_for_chain(
    conn: &rusqlite::Connection,
    chain_id: i64,
) -> Result<Vec<neuronprompter_core::domain::category::Category>, DbError> {
    let mut stmt = conn
        .prepare(
            "SELECT c.id, c.user_id, c.name, c.created_at \
             FROM categories c \
             INNER JOIN chain_categories cc ON cc.category_id = c.id \
             WHERE cc.chain_id = ?1 \
             ORDER BY c.name",
        )
        .map_err(|e| DbError::Query {
            operation: "get_categories_for_chain".to_owned(),
            source: e,
        })?;
    let rows = stmt
        .query_map(params![chain_id], |row| {
            Ok(neuronprompter_core::domain::category::Category {
                id: row.get(0)?,
                user_id: row.get(1)?,
                name: row.get(2)?,
                created_at: row.get(3)?,
            })
        })
        .map_err(|e| DbError::Query {
            operation: "get_categories_for_chain".to_owned(),
            source: e,
        })?;
    let mut result = Vec::new();
    for row in rows {
        result.push(row.map_err(|e| DbError::Query {
            operation: "get_categories_for_chain".to_owned(),
            source: e,
        })?);
    }
    Ok(result)
}

/// Returns collections associated with a chain.
///
/// # Errors
///
/// Returns `DbError::Query` if the SQL statement fails.
pub fn get_collections_for_chain(
    conn: &rusqlite::Connection,
    chain_id: i64,
) -> Result<Vec<neuronprompter_core::domain::collection::Collection>, DbError> {
    let mut stmt = conn
        .prepare(
            "SELECT c.id, c.user_id, c.name, c.created_at \
             FROM collections c \
             INNER JOIN chain_collections cc ON cc.collection_id = c.id \
             WHERE cc.chain_id = ?1 \
             ORDER BY c.name",
        )
        .map_err(|e| DbError::Query {
            operation: "get_collections_for_chain".to_owned(),
            source: e,
        })?;
    let rows = stmt
        .query_map(params![chain_id], |row| {
            Ok(neuronprompter_core::domain::collection::Collection {
                id: row.get(0)?,
                user_id: row.get(1)?,
                name: row.get(2)?,
                created_at: row.get(3)?,
            })
        })
        .map_err(|e| DbError::Query {
            operation: "get_collections_for_chain".to_owned(),
            source: e,
        })?;
    let mut result = Vec::new();
    for row in rows {
        result.push(row.map_err(|e| DbError::Query {
            operation: "get_collections_for_chain".to_owned(),
            source: e,
        })?);
    }
    Ok(result)
}

/// Links a chain to a tag via junction table.
///
/// # Errors
///
/// Returns `DbError::Query` if the SQL statement fails.
pub fn link_chain_tag(
    conn: &rusqlite::Connection,
    chain_id: i64,
    tag_id: i64,
) -> Result<(), DbError> {
    conn.execute(
        "INSERT INTO chain_tags (chain_id, tag_id) VALUES (?1, ?2) ON CONFLICT (chain_id, tag_id) DO NOTHING",
        params![chain_id, tag_id],
    )
    .map_err(|e| DbError::Query {
        operation: "link_chain_tag".to_owned(),
        source: e,
    })?;
    Ok(())
}

/// Unlinks a chain from a tag.
///
/// # Errors
///
/// Returns `DbError::Query` if the SQL statement fails.
pub fn unlink_chain_tag(
    conn: &rusqlite::Connection,
    chain_id: i64,
    tag_id: i64,
) -> Result<(), DbError> {
    conn.execute(
        "DELETE FROM chain_tags WHERE chain_id = ?1 AND tag_id = ?2",
        params![chain_id, tag_id],
    )
    .map_err(|e| DbError::Query {
        operation: "unlink_chain_tag".to_owned(),
        source: e,
    })?;
    Ok(())
}

/// Links a chain to a category via junction table.
///
/// # Errors
///
/// Returns `DbError::Query` if the SQL statement fails.
pub fn link_chain_category(
    conn: &rusqlite::Connection,
    chain_id: i64,
    category_id: i64,
) -> Result<(), DbError> {
    conn.execute(
        "INSERT INTO chain_categories (chain_id, category_id) VALUES (?1, ?2) ON CONFLICT (chain_id, category_id) DO NOTHING",
        params![chain_id, category_id],
    )
    .map_err(|e| DbError::Query {
        operation: "link_chain_category".to_owned(),
        source: e,
    })?;
    Ok(())
}

/// Unlinks a chain from a category.
///
/// # Errors
///
/// Returns `DbError::Query` if the SQL statement fails.
pub fn unlink_chain_category(
    conn: &rusqlite::Connection,
    chain_id: i64,
    category_id: i64,
) -> Result<(), DbError> {
    conn.execute(
        "DELETE FROM chain_categories WHERE chain_id = ?1 AND category_id = ?2",
        params![chain_id, category_id],
    )
    .map_err(|e| DbError::Query {
        operation: "unlink_chain_category".to_owned(),
        source: e,
    })?;
    Ok(())
}

/// Links a chain to a collection via junction table.
///
/// # Errors
///
/// Returns `DbError::Query` if the SQL statement fails.
pub fn link_chain_collection(
    conn: &rusqlite::Connection,
    chain_id: i64,
    collection_id: i64,
) -> Result<(), DbError> {
    conn.execute(
        "INSERT INTO chain_collections (chain_id, collection_id) VALUES (?1, ?2) ON CONFLICT (chain_id, collection_id) DO NOTHING",
        params![chain_id, collection_id],
    )
    .map_err(|e| DbError::Query {
        operation: "link_chain_collection".to_owned(),
        source: e,
    })?;
    Ok(())
}

/// Unlinks a chain from a collection.
///
/// # Errors
///
/// Returns `DbError::Query` if the SQL statement fails.
pub fn unlink_chain_collection(
    conn: &rusqlite::Connection,
    chain_id: i64,
    collection_id: i64,
) -> Result<(), DbError> {
    conn.execute(
        "DELETE FROM chain_collections WHERE chain_id = ?1 AND collection_id = ?2",
        params![chain_id, collection_id],
    )
    .map_err(|e| DbError::Query {
        operation: "unlink_chain_collection".to_owned(),
        source: e,
    })?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Inserts a single polymorphic step linking a chain to a prompt or script at a position.
fn insert_step_polymorphic(
    conn: &rusqlite::Connection,
    chain_id: i64,
    step_type: &str,
    prompt_id: Option<i64>,
    script_id: Option<i64>,
    position: i32,
) -> Result<(), DbError> {
    conn.execute(
        "INSERT INTO chain_steps (chain_id, step_type, prompt_id, script_id, position) \
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![chain_id, step_type, prompt_id, script_id, position],
    )
    .map_err(|e| DbError::Query {
        operation: "insert_chain_step".to_owned(),
        source: e,
    })?;
    Ok(())
}

/// Resolves step inputs from a `NewChain`: if `steps` is non-empty it takes
/// precedence, otherwise `prompt_ids` is converted to step inputs.
fn resolve_step_inputs(new: &NewChain) -> Vec<ChainStepInput> {
    if new.steps.is_empty() {
        new.prompt_ids
            .iter()
            .map(|id| ChainStepInput {
                step_type: StepType::Prompt,
                item_id: *id,
            })
            .collect()
    } else {
        new.steps.clone()
    }
}

/// Retrieves ordered steps for a chain with resolved prompt and script data.
/// Uses LEFT JOINs on both prompts and scripts tables; exactly one will match
/// based on the step_type discriminant.
fn get_resolved_steps(
    conn: &rusqlite::Connection,
    chain_id: i64,
) -> Result<Vec<ResolvedChainStep>, DbError> {
    let mut stmt = conn
        .prepare(
            "SELECT cs.id, cs.chain_id, cs.step_type, cs.prompt_id, cs.script_id, cs.position, \
             p.id, p.user_id, p.title, p.content, p.description, \
             p.notes, p.language, p.is_favorite, p.is_archived, p.current_version, \
             p.created_at, p.updated_at, \
             s.id, s.user_id, s.title, s.content, s.description, \
             s.notes, s.script_language, s.language, s.is_favorite, s.is_archived, \
             s.current_version, s.created_at, s.updated_at, s.source_path, s.is_synced \
             FROM chain_steps cs \
             LEFT JOIN prompts p ON p.id = cs.prompt_id AND cs.step_type = 'prompt' \
             LEFT JOIN scripts s ON s.id = cs.script_id AND cs.step_type = 'script' \
             WHERE cs.chain_id = ?1 \
             ORDER BY cs.position",
        )
        .map_err(|e| DbError::Query {
            operation: "get_resolved_steps".to_owned(),
            source: e,
        })?;

    let rows = stmt
        .query_map(params![chain_id], |row| {
            let step_type_str: String = row.get(2)?;
            let step_type = StepType::from_str_opt(&step_type_str).ok_or_else(|| {
                rusqlite::Error::FromSqlConversionFailure(
                    2,
                    rusqlite::types::Type::Text,
                    format!("unknown step_type: {step_type_str}").into(),
                )
            })?;
            let prompt = if step_type == StepType::Prompt {
                Some(Prompt {
                    id: row.get(6)?,
                    user_id: row.get(7)?,
                    title: row.get(8)?,
                    content: row.get(9)?,
                    description: row.get(10)?,
                    notes: row.get(11)?,
                    language: row.get(12)?,
                    is_favorite: row.get(13)?,
                    is_archived: row.get(14)?,
                    current_version: row.get(15)?,
                    created_at: row.get(16)?,
                    updated_at: row.get(17)?,
                })
            } else {
                None
            };
            let script = if step_type == StepType::Script {
                Some(Script {
                    id: row.get(18)?,
                    user_id: row.get(19)?,
                    title: row.get(20)?,
                    content: row.get(21)?,
                    description: row.get(22)?,
                    notes: row.get(23)?,
                    script_language: row.get(24)?,
                    language: row.get(25)?,
                    is_favorite: row.get(26)?,
                    is_archived: row.get(27)?,
                    current_version: row.get(28)?,
                    created_at: row.get(29)?,
                    updated_at: row.get(30)?,
                    source_path: row.get(31)?,
                    is_synced: row.get(32)?,
                })
            } else {
                None
            };
            Ok(ResolvedChainStep {
                step: ChainStep {
                    id: row.get(0)?,
                    chain_id: row.get(1)?,
                    step_type,
                    prompt_id: row.get(3)?,
                    script_id: row.get(4)?,
                    position: row.get(5)?,
                },
                prompt,
                script,
            })
        })
        .map_err(|e| DbError::Query {
            operation: "get_resolved_steps".to_owned(),
            source: e,
        })?;

    let mut result = Vec::new();
    for row in rows {
        result.push(row.map_err(|e| DbError::Query {
            operation: "get_resolved_steps".to_owned(),
            source: e,
        })?);
    }
    Ok(result)
}

/// Maps a `rusqlite` row to a `Chain` struct. Column names must match the
/// SELECT statements used throughout this module.
fn row_to_chain(row: &rusqlite::Row<'_>) -> rusqlite::Result<Chain> {
    Ok(Chain {
        id: row.get("id")?,
        user_id: row.get("user_id")?,
        title: row.get("title")?,
        description: row.get("description")?,
        notes: row.get("notes")?,
        language: row.get("language")?,
        separator: row.get("separator")?,
        is_favorite: row.get("is_favorite")?,
        is_archived: row.get("is_archived")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
    })
}
