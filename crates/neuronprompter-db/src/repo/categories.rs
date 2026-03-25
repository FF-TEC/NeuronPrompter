// =============================================================================
// Category repository operations.
//
// Provides CRUD functions for the categories table and the prompt_categories
// junction table. Categories are scoped per user with a (user_id, name)
// uniqueness constraint.
// =============================================================================

use neuronprompter_core::CoreError;
use neuronprompter_core::domain::category::Category;
use rusqlite::params;

use crate::DbError;

/// Creates a category under the specified user.
///
/// # Errors
///
/// Returns `DbError::Core` with `CoreError::Duplicate` if a category with the
/// same name already exists for the user.
/// Returns `DbError::Query` if the SQL statement fails.
pub fn create_category(
    conn: &rusqlite::Connection,
    user_id: i64,
    name: &str,
) -> Result<Category, DbError> {
    conn.execute(
        "INSERT INTO categories (user_id, name) VALUES (?1, ?2)",
        params![user_id, name],
    )
    .map_err(|e| {
        if let rusqlite::Error::SqliteFailure(ref err, _) = e
            && err.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE
        {
            return DbError::Core(CoreError::Duplicate {
                entity: "Category".to_owned(),
                field: "name".to_owned(),
                value: name.to_owned(),
            });
        }
        DbError::Query {
            operation: "create_category".to_owned(),
            source: e,
        }
    })?;
    let id = conn.last_insert_rowid();
    get_category(conn, id)
}

/// Retrieves a category by primary key.
///
/// # Errors
///
/// Returns `DbError::Core` with `CoreError::NotFound` if no category exists
/// with the given id.
/// Returns `DbError::Query` if the SQL statement fails.
pub fn get_category(conn: &rusqlite::Connection, id: i64) -> Result<Category, DbError> {
    conn.query_row(
        "SELECT id, user_id, name, created_at FROM categories WHERE id = ?1",
        params![id],
        row_to_category,
    )
    .map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => DbError::Core(CoreError::NotFound {
            entity: "Category".to_owned(),
            id,
        }),
        other => DbError::Query {
            operation: "get_category".to_owned(),
            source: other,
        },
    })
}

/// Returns all categories owned by a user, ordered by name.
///
/// # Errors
///
/// Returns `DbError::Query` if the SQL statement fails.
pub fn list_categories_for_user(
    conn: &rusqlite::Connection,
    user_id: i64,
) -> Result<Vec<Category>, DbError> {
    let mut stmt = conn
        .prepare(
            "SELECT id, user_id, name, created_at FROM categories \
             WHERE user_id = ?1 ORDER BY name LIMIT 10000",
        )
        .map_err(|e| DbError::Query {
            operation: "list_categories_for_user".to_owned(),
            source: e,
        })?;
    let rows = stmt
        .query_map(params![user_id], row_to_category)
        .map_err(|e| DbError::Query {
            operation: "list_categories_for_user".to_owned(),
            source: e,
        })?;
    collect_rows(rows, "list_categories_for_user")
}

/// Renames an existing category.
///
/// # Errors
///
/// Returns `DbError::Core` with `CoreError::Duplicate` if the new name
/// conflicts with an existing category for the same user.
/// Returns `DbError::Core` with `CoreError::NotFound` if no category exists
/// with the given id.
/// Returns `DbError::Query` if the SQL statement fails.
pub fn rename_category(
    conn: &rusqlite::Connection,
    id: i64,
    new_name: &str,
) -> Result<(), DbError> {
    let affected = conn
        .execute(
            "UPDATE categories SET name = ?1 WHERE id = ?2",
            params![new_name, id],
        )
        .map_err(|e| {
            if let rusqlite::Error::SqliteFailure(ref err, _) = e
                && err.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE
            {
                return DbError::Core(CoreError::Duplicate {
                    entity: "Category".to_owned(),
                    field: "name".to_owned(),
                    value: new_name.to_owned(),
                });
            }
            DbError::Query {
                operation: "rename_category".to_owned(),
                source: e,
            }
        })?;
    if affected == 0 {
        return Err(DbError::Core(CoreError::NotFound {
            entity: "Category".to_owned(),
            id,
        }));
    }
    Ok(())
}

/// Deletes a category and all its junction-table links via CASCADE.
///
/// # Errors
///
/// Returns `DbError::Core` with `CoreError::NotFound` if no category exists
/// with the given id.
/// Returns `DbError::Query` if the SQL statement fails.
pub fn delete_category(conn: &rusqlite::Connection, id: i64) -> Result<(), DbError> {
    let affected = conn
        .execute("DELETE FROM categories WHERE id = ?1", params![id])
        .map_err(|e| DbError::Query {
            operation: "delete_category".to_owned(),
            source: e,
        })?;
    if affected == 0 {
        return Err(DbError::Core(CoreError::NotFound {
            entity: "Category".to_owned(),
            id,
        }));
    }
    Ok(())
}

/// Returns categories associated with a specific prompt.
///
/// # Errors
///
/// Returns `DbError::Query` if the SQL statement fails.
pub fn get_categories_for_prompt(
    conn: &rusqlite::Connection,
    prompt_id: i64,
) -> Result<Vec<Category>, DbError> {
    let mut stmt = conn
        .prepare(
            "SELECT c.id, c.user_id, c.name, c.created_at \
             FROM categories c \
             INNER JOIN prompt_categories pc ON pc.category_id = c.id \
             WHERE pc.prompt_id = ?1 \
             ORDER BY c.name",
        )
        .map_err(|e| DbError::Query {
            operation: "get_categories_for_prompt".to_owned(),
            source: e,
        })?;
    let rows = stmt
        .query_map(params![prompt_id], row_to_category)
        .map_err(|e| DbError::Query {
            operation: "get_categories_for_prompt".to_owned(),
            source: e,
        })?;
    collect_rows(rows, "get_categories_for_prompt")
}

/// Creates a junction-table link between a prompt and a category.
///
/// # Errors
///
/// Returns `DbError::Query` if the SQL statement fails.
pub fn link_prompt_category(
    conn: &rusqlite::Connection,
    prompt_id: i64,
    category_id: i64,
) -> Result<(), DbError> {
    conn.execute(
        "INSERT INTO prompt_categories (prompt_id, category_id) VALUES (?1, ?2) ON CONFLICT (prompt_id, category_id) DO NOTHING",
        params![prompt_id, category_id],
    )
    .map_err(|e| DbError::Query {
        operation: "link_prompt_category".to_owned(),
        source: e,
    })?;
    Ok(())
}

/// Removes the junction-table link between a prompt and a category.
///
/// # Errors
///
/// Returns `DbError::Query` if the SQL statement fails.
pub fn unlink_prompt_category(
    conn: &rusqlite::Connection,
    prompt_id: i64,
    category_id: i64,
) -> Result<(), DbError> {
    conn.execute(
        "DELETE FROM prompt_categories WHERE prompt_id = ?1 AND category_id = ?2",
        params![prompt_id, category_id],
    )
    .map_err(|e| DbError::Query {
        operation: "unlink_prompt_category".to_owned(),
        source: e,
    })?;
    Ok(())
}

/// Looks up a category by exact name within a user's namespace.
///
/// # Errors
///
/// Returns `DbError::Query` if the SQL statement fails.
pub fn find_category_by_name(
    conn: &rusqlite::Connection,
    user_id: i64,
    name: &str,
) -> Result<Option<Category>, DbError> {
    let result = conn.query_row(
        "SELECT id, user_id, name, created_at FROM categories WHERE user_id = ?1 AND name = ?2",
        params![user_id, name],
        row_to_category,
    );
    match result {
        Ok(cat) => Ok(Some(cat)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(DbError::Query {
            operation: "find_category_by_name".to_owned(),
            source: e,
        }),
    }
}

/// Maps a `rusqlite` row to a `Category` struct.
fn row_to_category(row: &rusqlite::Row<'_>) -> rusqlite::Result<Category> {
    Ok(Category {
        id: row.get("id")?,
        user_id: row.get("user_id")?,
        name: row.get("name")?,
        created_at: row.get("created_at")?,
    })
}

/// Collects `query_map` rows into a Vec, wrapping iteration errors in `DbError`.
fn collect_rows(
    rows: rusqlite::MappedRows<'_, impl FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<Category>>,
    operation: &str,
) -> Result<Vec<Category>, DbError> {
    let mut result = Vec::new();
    for row in rows {
        result.push(row.map_err(|e| DbError::Query {
            operation: operation.to_owned(),
            source: e,
        })?);
    }
    Ok(result)
}
