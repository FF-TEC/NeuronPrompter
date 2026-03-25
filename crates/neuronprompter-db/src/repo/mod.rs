// =============================================================================
// Repository modules.
//
// Each module implements CRUD operations for one domain aggregate. All
// functions accept a &rusqlite::Connection and return Result<T, DbError>.
// =============================================================================

// Architecture note: Repository functions are free-standing functions accepting
// &rusqlite::Connection. A future improvement would define repository traits in
// neuronprompter-core and implement them here, enabling unit testing of the
// application layer with mock repositories.

use crate::DbError;

pub mod categories;
pub mod chains;
pub mod collections;
pub mod prompts;
pub mod script_versions;
pub mod scripts;
pub mod search;
pub mod settings;
pub mod tags;
pub mod users;
pub mod versions;

/// Executes a closure within a SQLite SAVEPOINT for atomic multi-step operations.
/// On success, releases the savepoint. On error, rolls back and returns the error.
///
/// The savepoint name is validated at runtime to contain only ASCII alphanumeric
/// characters and underscores. This prevents SQL injection through savepoint names,
/// which are interpolated into SQL strings via `format!`.
///
/// # Errors
///
/// Returns `DbError::Query` if the savepoint name contains invalid characters,
/// if the SAVEPOINT or RELEASE statement fails, or propagates any error
/// returned by the closure.
pub fn with_savepoint<F, T>(conn: &rusqlite::Connection, name: &str, f: F) -> Result<T, DbError>
where
    F: FnOnce(&rusqlite::Connection) -> Result<T, DbError>,
{
    // Runtime validation of savepoint name to prevent SQL injection.
    // The name is interpolated directly into SQL statements, so it must
    // consist only of safe identifier characters.
    if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(DbError::Query {
            operation: "savepoint_name_validation".to_owned(),
            source: rusqlite::Error::InvalidParameterName(format!(
                "savepoint name must contain only alphanumeric characters and underscores, got: {name}"
            )),
        });
    }

    conn.execute_batch(&format!("SAVEPOINT {name}"))
        .map_err(|e| DbError::Query {
            operation: format!("savepoint_begin_{name}"),
            source: e,
        })?;
    match f(conn) {
        Ok(val) => {
            conn.execute_batch(&format!("RELEASE {name}"))
                .map_err(|e| DbError::Query {
                    operation: format!("savepoint_release_{name}"),
                    source: e,
                })?;
            Ok(val)
        }
        Err(e) => {
            let _ = conn.execute_batch(&format!("ROLLBACK TO {name}"));
            let _ = conn.execute_batch(&format!("RELEASE {name}"));
            Err(e)
        }
    }
}

/// Tokenizes a user's query string and appends a `*` wildcard to each
/// term for FTS5 prefix matching. Special FTS5 syntax characters are
/// escaped by quoting each term.
pub(crate) fn build_fts_query(query: &str) -> String {
    let terms: Vec<String> = query
        .split_whitespace()
        .filter(|t| !t.is_empty())
        .map(|term| {
            // Quote each term to escape FTS5 special characters (*, :, ^, etc.).
            // The `*` suffix enables prefix matching.
            let escaped = term.replace('"', "\"\"");
            format!("\"{escaped}\"*")
        })
        .collect();
    terms.join(" ")
}
