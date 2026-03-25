// =============================================================================
// Collection repository operations.
//
// Provides CRUD functions for the collections table and the prompt_collections
// junction table. Collections are scoped per user with a (user_id, name)
// uniqueness constraint.
// =============================================================================

use neuronprompter_core::CoreError;
use neuronprompter_core::domain::collection::Collection;
use rusqlite::params;

use crate::DbError;

/// Creates a collection under the specified user.
///
/// # Errors
///
/// Returns `DbError::Core` with `CoreError::Duplicate` if a collection with
/// the same name already exists for the user.
/// Returns `DbError::Query` if the SQL statement fails.
pub fn create_collection(
    conn: &rusqlite::Connection,
    user_id: i64,
    name: &str,
) -> Result<Collection, DbError> {
    conn.execute(
        "INSERT INTO collections (user_id, name) VALUES (?1, ?2)",
        params![user_id, name],
    )
    .map_err(|e| {
        if let rusqlite::Error::SqliteFailure(ref err, _) = e
            && err.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE
        {
            return DbError::Core(CoreError::Duplicate {
                entity: "Collection".to_owned(),
                field: "name".to_owned(),
                value: name.to_owned(),
            });
        }
        DbError::Query {
            operation: "create_collection".to_owned(),
            source: e,
        }
    })?;
    let id = conn.last_insert_rowid();
    get_collection(conn, id)
}

/// Retrieves a collection by primary key.
///
/// # Errors
///
/// Returns `DbError::Core` with `CoreError::NotFound` if no collection exists
/// with the given id.
/// Returns `DbError::Query` if the SQL statement fails.
pub fn get_collection(conn: &rusqlite::Connection, id: i64) -> Result<Collection, DbError> {
    conn.query_row(
        "SELECT id, user_id, name, created_at FROM collections WHERE id = ?1",
        params![id],
        row_to_collection,
    )
    .map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => DbError::Core(CoreError::NotFound {
            entity: "Collection".to_owned(),
            id,
        }),
        other => DbError::Query {
            operation: "get_collection".to_owned(),
            source: other,
        },
    })
}

/// Returns all collections owned by a user, ordered by name.
///
/// # Errors
///
/// Returns `DbError::Query` if the SQL statement fails.
pub fn list_collections_for_user(
    conn: &rusqlite::Connection,
    user_id: i64,
) -> Result<Vec<Collection>, DbError> {
    let mut stmt = conn
        .prepare(
            "SELECT id, user_id, name, created_at FROM collections \
             WHERE user_id = ?1 ORDER BY name LIMIT 10000",
        )
        .map_err(|e| DbError::Query {
            operation: "list_collections_for_user".to_owned(),
            source: e,
        })?;
    let rows = stmt
        .query_map(params![user_id], row_to_collection)
        .map_err(|e| DbError::Query {
            operation: "list_collections_for_user".to_owned(),
            source: e,
        })?;
    collect_rows(rows, "list_collections_for_user")
}

/// Renames an existing collection.
///
/// # Errors
///
/// Returns `DbError::Core` with `CoreError::Duplicate` if the new name
/// conflicts with an existing collection for the same user.
/// Returns `DbError::Core` with `CoreError::NotFound` if no collection exists
/// with the given id.
/// Returns `DbError::Query` if the SQL statement fails.
pub fn rename_collection(
    conn: &rusqlite::Connection,
    id: i64,
    new_name: &str,
) -> Result<(), DbError> {
    let affected = conn
        .execute(
            "UPDATE collections SET name = ?1 WHERE id = ?2",
            params![new_name, id],
        )
        .map_err(|e| {
            if let rusqlite::Error::SqliteFailure(ref err, _) = e
                && err.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE
            {
                return DbError::Core(CoreError::Duplicate {
                    entity: "Collection".to_owned(),
                    field: "name".to_owned(),
                    value: new_name.to_owned(),
                });
            }
            DbError::Query {
                operation: "rename_collection".to_owned(),
                source: e,
            }
        })?;
    if affected == 0 {
        return Err(DbError::Core(CoreError::NotFound {
            entity: "Collection".to_owned(),
            id,
        }));
    }
    Ok(())
}

/// Deletes a collection and all its junction-table links via CASCADE.
///
/// # Errors
///
/// Returns `DbError::Core` with `CoreError::NotFound` if no collection exists
/// with the given id.
/// Returns `DbError::Query` if the SQL statement fails.
pub fn delete_collection(conn: &rusqlite::Connection, id: i64) -> Result<(), DbError> {
    let affected = conn
        .execute("DELETE FROM collections WHERE id = ?1", params![id])
        .map_err(|e| DbError::Query {
            operation: "delete_collection".to_owned(),
            source: e,
        })?;
    if affected == 0 {
        return Err(DbError::Core(CoreError::NotFound {
            entity: "Collection".to_owned(),
            id,
        }));
    }
    Ok(())
}

/// Returns collections associated with a specific prompt.
///
/// # Errors
///
/// Returns `DbError::Query` if the SQL statement fails.
pub fn get_collections_for_prompt(
    conn: &rusqlite::Connection,
    prompt_id: i64,
) -> Result<Vec<Collection>, DbError> {
    let mut stmt = conn
        .prepare(
            "SELECT c.id, c.user_id, c.name, c.created_at \
             FROM collections c \
             INNER JOIN prompt_collections pc ON pc.collection_id = c.id \
             WHERE pc.prompt_id = ?1 \
             ORDER BY c.name",
        )
        .map_err(|e| DbError::Query {
            operation: "get_collections_for_prompt".to_owned(),
            source: e,
        })?;
    let rows = stmt
        .query_map(params![prompt_id], row_to_collection)
        .map_err(|e| DbError::Query {
            operation: "get_collections_for_prompt".to_owned(),
            source: e,
        })?;
    collect_rows(rows, "get_collections_for_prompt")
}

/// Creates a junction-table link between a prompt and a collection.
///
/// # Errors
///
/// Returns `DbError::Query` if the SQL statement fails.
pub fn link_prompt_collection(
    conn: &rusqlite::Connection,
    prompt_id: i64,
    collection_id: i64,
) -> Result<(), DbError> {
    conn.execute(
        "INSERT INTO prompt_collections (prompt_id, collection_id) VALUES (?1, ?2) ON CONFLICT (prompt_id, collection_id) DO NOTHING",
        params![prompt_id, collection_id],
    )
    .map_err(|e| DbError::Query {
        operation: "link_prompt_collection".to_owned(),
        source: e,
    })?;
    Ok(())
}

/// Removes the junction-table link between a prompt and a collection.
///
/// # Errors
///
/// Returns `DbError::Query` if the SQL statement fails.
pub fn unlink_prompt_collection(
    conn: &rusqlite::Connection,
    prompt_id: i64,
    collection_id: i64,
) -> Result<(), DbError> {
    conn.execute(
        "DELETE FROM prompt_collections WHERE prompt_id = ?1 AND collection_id = ?2",
        params![prompt_id, collection_id],
    )
    .map_err(|e| DbError::Query {
        operation: "unlink_prompt_collection".to_owned(),
        source: e,
    })?;
    Ok(())
}

/// Looks up a collection by exact name within a user's namespace.
///
/// # Errors
///
/// Returns `DbError::Query` if the SQL statement fails.
pub fn find_collection_by_name(
    conn: &rusqlite::Connection,
    user_id: i64,
    name: &str,
) -> Result<Option<Collection>, DbError> {
    let result = conn.query_row(
        "SELECT id, user_id, name, created_at FROM collections WHERE user_id = ?1 AND name = ?2",
        params![user_id, name],
        row_to_collection,
    );
    match result {
        Ok(col) => Ok(Some(col)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(DbError::Query {
            operation: "find_collection_by_name".to_owned(),
            source: e,
        }),
    }
}

/// Maps a `rusqlite` row to a `Collection` struct.
fn row_to_collection(row: &rusqlite::Row<'_>) -> rusqlite::Result<Collection> {
    Ok(Collection {
        id: row.get("id")?,
        user_id: row.get("user_id")?,
        name: row.get("name")?,
        created_at: row.get("created_at")?,
    })
}

/// Collects `query_map` rows into a Vec, wrapping iteration errors in `DbError`.
fn collect_rows(
    rows: rusqlite::MappedRows<'_, impl FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<Collection>>,
    operation: &str,
) -> Result<Vec<Collection>, DbError> {
    let mut result = Vec::new();
    for row in rows {
        result.push(row.map_err(|e| DbError::Query {
            operation: operation.to_owned(),
            source: e,
        })?);
    }
    Ok(result)
}
