// =============================================================================
// Tag repository operations.
//
// Provides CRUD functions for the tags table and the prompt_tags junction
// table. Tags are scoped per user with a (user_id, name) uniqueness constraint.
// =============================================================================

use neuronprompter_core::CoreError;
use neuronprompter_core::domain::tag::Tag;
use rusqlite::params;

use crate::DbError;

/// Creates a tag under the specified user and returns the persisted entity.
///
/// # Errors
///
/// Returns `DbError::Core` with `CoreError::Duplicate` if a tag with the same
/// name already exists for the user.
/// Returns `DbError::Query` if the SQL statement fails.
pub fn create_tag(conn: &rusqlite::Connection, user_id: i64, name: &str) -> Result<Tag, DbError> {
    conn.execute(
        "INSERT INTO tags (user_id, name) VALUES (?1, ?2)",
        params![user_id, name],
    )
    .map_err(|e| {
        if let rusqlite::Error::SqliteFailure(ref err, _) = e
            && err.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE
        {
            return DbError::Core(CoreError::Duplicate {
                entity: "Tag".to_owned(),
                field: "name".to_owned(),
                value: name.to_owned(),
            });
        }
        DbError::Query {
            operation: "create_tag".to_owned(),
            source: e,
        }
    })?;
    let id = conn.last_insert_rowid();
    get_tag(conn, id)
}

/// Retrieves a tag by primary key.
///
/// # Errors
///
/// Returns `DbError::Core` with `CoreError::NotFound` if no tag exists with
/// the given id.
/// Returns `DbError::Query` if the SQL statement fails.
pub fn get_tag(conn: &rusqlite::Connection, id: i64) -> Result<Tag, DbError> {
    conn.query_row(
        "SELECT id, user_id, name, created_at FROM tags WHERE id = ?1",
        params![id],
        row_to_tag,
    )
    .map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => DbError::Core(CoreError::NotFound {
            entity: "Tag".to_owned(),
            id,
        }),
        other => DbError::Query {
            operation: "get_tag".to_owned(),
            source: other,
        },
    })
}

/// Returns all tags owned by a user, ordered by name.
///
/// # Errors
///
/// Returns `DbError::Query` if the SQL statement fails.
pub fn list_tags_for_user(conn: &rusqlite::Connection, user_id: i64) -> Result<Vec<Tag>, DbError> {
    let mut stmt = conn
        .prepare("SELECT id, user_id, name, created_at FROM tags WHERE user_id = ?1 ORDER BY name LIMIT 10000")
        .map_err(|e| DbError::Query {
            operation: "list_tags_for_user".to_owned(),
            source: e,
        })?;
    let rows = stmt
        .query_map(params![user_id], row_to_tag)
        .map_err(|e| DbError::Query {
            operation: "list_tags_for_user".to_owned(),
            source: e,
        })?;
    collect_rows(rows, "list_tags_for_user")
}

/// Renames an existing tag.
///
/// # Errors
///
/// Returns `DbError::Core` with `CoreError::Duplicate` if the new name
/// conflicts with an existing tag for the same user.
/// Returns `DbError::Core` with `CoreError::NotFound` if no tag exists with
/// the given id.
/// Returns `DbError::Query` if the SQL statement fails.
pub fn rename_tag(conn: &rusqlite::Connection, id: i64, new_name: &str) -> Result<(), DbError> {
    let affected = conn
        .execute(
            "UPDATE tags SET name = ?1 WHERE id = ?2",
            params![new_name, id],
        )
        .map_err(|e| {
            if let rusqlite::Error::SqliteFailure(ref err, _) = e
                && err.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE
            {
                return DbError::Core(CoreError::Duplicate {
                    entity: "Tag".to_owned(),
                    field: "name".to_owned(),
                    value: new_name.to_owned(),
                });
            }
            DbError::Query {
                operation: "rename_tag".to_owned(),
                source: e,
            }
        })?;
    if affected == 0 {
        return Err(DbError::Core(CoreError::NotFound {
            entity: "Tag".to_owned(),
            id,
        }));
    }
    Ok(())
}

/// Deletes a tag and all its junction-table links via CASCADE.
///
/// # Errors
///
/// Returns `DbError::Core` with `CoreError::NotFound` if no tag exists with
/// the given id.
/// Returns `DbError::Query` if the SQL statement fails.
pub fn delete_tag(conn: &rusqlite::Connection, id: i64) -> Result<(), DbError> {
    let affected = conn
        .execute("DELETE FROM tags WHERE id = ?1", params![id])
        .map_err(|e| DbError::Query {
            operation: "delete_tag".to_owned(),
            source: e,
        })?;
    if affected == 0 {
        return Err(DbError::Core(CoreError::NotFound {
            entity: "Tag".to_owned(),
            id,
        }));
    }
    Ok(())
}

/// Returns tags associated with a specific prompt via the `prompt_tags`
/// junction table.
///
/// # Errors
///
/// Returns `DbError::Query` if the SQL statement fails.
pub fn get_tags_for_prompt(
    conn: &rusqlite::Connection,
    prompt_id: i64,
) -> Result<Vec<Tag>, DbError> {
    let mut stmt = conn
        .prepare(
            "SELECT t.id, t.user_id, t.name, t.created_at \
             FROM tags t \
             INNER JOIN prompt_tags pt ON pt.tag_id = t.id \
             WHERE pt.prompt_id = ?1 \
             ORDER BY t.name",
        )
        .map_err(|e| DbError::Query {
            operation: "get_tags_for_prompt".to_owned(),
            source: e,
        })?;
    let rows = stmt
        .query_map(params![prompt_id], row_to_tag)
        .map_err(|e| DbError::Query {
            operation: "get_tags_for_prompt".to_owned(),
            source: e,
        })?;
    collect_rows(rows, "get_tags_for_prompt")
}

/// Creates a junction-table link between a prompt and a tag. Silently
/// succeeds if the link already exists (ON CONFLICT DO NOTHING).
///
/// # Errors
///
/// Returns `DbError::Query` if the SQL statement fails.
pub fn link_prompt_tag(
    conn: &rusqlite::Connection,
    prompt_id: i64,
    tag_id: i64,
) -> Result<(), DbError> {
    conn.execute(
        "INSERT INTO prompt_tags (prompt_id, tag_id) VALUES (?1, ?2) ON CONFLICT (prompt_id, tag_id) DO NOTHING",
        params![prompt_id, tag_id],
    )
    .map_err(|e| DbError::Query {
        operation: "link_prompt_tag".to_owned(),
        source: e,
    })?;
    Ok(())
}

/// Removes the junction-table link between a prompt and a tag.
///
/// # Errors
///
/// Returns `DbError::Query` if the SQL statement fails.
pub fn unlink_prompt_tag(
    conn: &rusqlite::Connection,
    prompt_id: i64,
    tag_id: i64,
) -> Result<(), DbError> {
    conn.execute(
        "DELETE FROM prompt_tags WHERE prompt_id = ?1 AND tag_id = ?2",
        params![prompt_id, tag_id],
    )
    .map_err(|e| DbError::Query {
        operation: "unlink_prompt_tag".to_owned(),
        source: e,
    })?;
    Ok(())
}

/// Looks up a tag by exact name within a user's namespace.
///
/// # Errors
///
/// Returns `DbError::Query` if the SQL statement fails.
pub fn find_tag_by_name(
    conn: &rusqlite::Connection,
    user_id: i64,
    name: &str,
) -> Result<Option<Tag>, DbError> {
    let result = conn.query_row(
        "SELECT id, user_id, name, created_at FROM tags WHERE user_id = ?1 AND name = ?2",
        params![user_id, name],
        row_to_tag,
    );
    match result {
        Ok(tag) => Ok(Some(tag)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(DbError::Query {
            operation: "find_tag_by_name".to_owned(),
            source: e,
        }),
    }
}

/// Maps a `rusqlite` row to a `Tag` struct. Used by all query functions.
fn row_to_tag(row: &rusqlite::Row<'_>) -> rusqlite::Result<Tag> {
    Ok(Tag {
        id: row.get("id")?,
        user_id: row.get("user_id")?,
        name: row.get("name")?,
        created_at: row.get("created_at")?,
    })
}

/// Collects `query_map` rows into a Vec, wrapping iteration errors in `DbError`.
fn collect_rows(
    rows: rusqlite::MappedRows<'_, impl FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<Tag>>,
    operation: &str,
) -> Result<Vec<Tag>, DbError> {
    let mut result = Vec::new();
    for row in rows {
        result.push(row.map_err(|e| DbError::Query {
            operation: operation.to_owned(),
            source: e,
        })?);
    }
    Ok(result)
}
