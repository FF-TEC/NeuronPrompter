// =============================================================================
// User repository operations.
//
// Provides CRUD functions for the users table. All functions accept a
// &rusqlite::Connection reference to operate within the caller's mutex-guarded
// scope and return Result<T, DbError>.
// =============================================================================

use neuronprompter_core::CoreError;
use neuronprompter_core::domain::user::{NewUser, User};
use rusqlite::params;

use crate::DbError;

/// Inserts a user record and returns the persisted entity with the
/// database-assigned id and timestamps.
///
/// # Errors
///
/// Returns `DbError::Core` with `CoreError::Duplicate` if a user with the
/// same username already exists.
/// Returns `DbError::Query` if the SQL statement fails.
pub fn create_user(conn: &rusqlite::Connection, new_user: &NewUser) -> Result<User, DbError> {
    conn.execute(
        "INSERT INTO users (username, display_name) VALUES (?1, ?2)",
        params![new_user.username, new_user.display_name],
    )
    .map_err(|e| {
        if let rusqlite::Error::SqliteFailure(ref err, _) = e
            && err.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE
        {
            return DbError::Core(CoreError::Duplicate {
                entity: "User".to_owned(),
                field: "username".to_owned(),
                value: new_user.username.clone(),
            });
        }
        DbError::Query {
            operation: "create_user".to_owned(),
            source: e,
        }
    })?;
    let id = conn.last_insert_rowid();
    get_user(conn, id)
}

/// Retrieves a user by primary key.
///
/// # Errors
///
/// Returns `DbError::Core` with `CoreError::NotFound` if no user exists with
/// the given id.
/// Returns `DbError::Query` if the SQL statement fails.
pub fn get_user(conn: &rusqlite::Connection, id: i64) -> Result<User, DbError> {
    conn.query_row(
        "SELECT id, username, display_name, created_at, updated_at FROM users WHERE id = ?1",
        params![id],
        |row| {
            Ok(User {
                id: row.get("id")?,
                username: row.get("username")?,
                display_name: row.get("display_name")?,
                created_at: row.get("created_at")?,
                updated_at: row.get("updated_at")?,
            })
        },
    )
    .map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => DbError::Core(CoreError::NotFound {
            entity: "User".to_owned(),
            id,
        }),
        other => DbError::Query {
            operation: "get_user".to_owned(),
            source: other,
        },
    })
}

/// Returns all user records ordered by username, capped at 1000 rows.
/// The LIMIT provides a defensive upper bound consistent with other list
/// functions in the repository layer. In practice, the user count for a
/// LAN-deployed desktop application is far below this threshold.
///
/// # Errors
///
/// Returns `DbError::Query` if the SQL statement fails.
pub fn list_users(conn: &rusqlite::Connection) -> Result<Vec<User>, DbError> {
    let mut stmt = conn
        .prepare(
            "SELECT id, username, display_name, created_at, updated_at \
             FROM users ORDER BY username LIMIT 1000",
        )
        .map_err(|e| DbError::Query {
            operation: "list_users".to_owned(),
            source: e,
        })?;
    let rows = stmt
        .query_map([], |row| {
            Ok(User {
                id: row.get("id")?,
                username: row.get("username")?,
                display_name: row.get("display_name")?,
                created_at: row.get("created_at")?,
                updated_at: row.get("updated_at")?,
            })
        })
        .map_err(|e| DbError::Query {
            operation: "list_users".to_owned(),
            source: e,
        })?;
    let mut users = Vec::new();
    for row in rows {
        users.push(row.map_err(|e| DbError::Query {
            operation: "list_users".to_owned(),
            source: e,
        })?);
    }
    Ok(users)
}

/// Deletes a user by primary key. Associated data (prompts, tags, categories,
/// collections, settings) is removed via foreign key CASCADE rules.
///
/// # Errors
///
/// Returns `DbError::Core` with `CoreError::NotFound` if no user exists with
/// the given id.
/// Returns `DbError::Query` if the SQL statement fails.
pub fn delete_user(conn: &rusqlite::Connection, id: i64) -> Result<(), DbError> {
    let affected = conn
        .execute("DELETE FROM users WHERE id = ?1", params![id])
        .map_err(|e| DbError::Query {
            operation: "delete_user".to_owned(),
            source: e,
        })?;
    if affected == 0 {
        return Err(DbError::Core(CoreError::NotFound {
            entity: "User".to_owned(),
            id,
        }));
    }
    Ok(())
}

/// Updates a user's display_name and username.
///
/// # Errors
///
/// Returns `DbError::Core` with `CoreError::Duplicate` if the new username
/// conflicts with an existing user.
/// Returns `DbError::Core` with `CoreError::NotFound` if no user exists with
/// the given id.
/// Returns `DbError::Query` if the SQL statement fails.
pub fn update_user(
    conn: &rusqlite::Connection,
    id: i64,
    display_name: &str,
    username: &str,
) -> Result<User, DbError> {
    let affected = conn
        .execute(
            "UPDATE users SET display_name = ?1, username = ?2, \
             updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now') WHERE id = ?3",
            params![display_name, username, id],
        )
        .map_err(|e| {
            if let rusqlite::Error::SqliteFailure(ref err, _) = e
                && err.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE
            {
                return DbError::Core(CoreError::Duplicate {
                    entity: "User".to_owned(),
                    field: "username".to_owned(),
                    value: username.to_owned(),
                });
            }
            DbError::Query {
                operation: "update_user".to_owned(),
                source: e,
            }
        })?;
    if affected == 0 {
        return Err(DbError::Core(CoreError::NotFound {
            entity: "User".to_owned(),
            id,
        }));
    }
    get_user(conn, id)
}

/// Looks up a user by exact username match. Returns None if no user exists
/// with the given username.
///
/// # Errors
///
/// Returns `DbError::Query` if the SQL statement fails.
pub fn find_user_by_username(
    conn: &rusqlite::Connection,
    username: &str,
) -> Result<Option<User>, DbError> {
    let result = conn.query_row(
        "SELECT id, username, display_name, created_at, updated_at FROM users WHERE username = ?1",
        params![username],
        |row| {
            Ok(User {
                id: row.get("id")?,
                username: row.get("username")?,
                display_name: row.get("display_name")?,
                created_at: row.get("created_at")?,
                updated_at: row.get("updated_at")?,
            })
        },
    );
    match result {
        Ok(user) => Ok(Some(user)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(DbError::Query {
            operation: "find_user_by_username".to_owned(),
            source: e,
        }),
    }
}
