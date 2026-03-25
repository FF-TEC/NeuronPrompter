// =============================================================================
// Schema migration runner.
//
// Embeds the SQL migration file at compile time and applies it on first run.
// A schema_version table tracks whether the migration has been applied.
// =============================================================================

use crate::error::DbError;

/// Complete schema SQL. Embedded from the consolidated migration file at
/// compile time to ensure the binary is self-contained.
const MIGRATION_0001: &str = include_str!("../../../migrations/0001_initial.sql");

/// All migrations in order. Each entry is a (version, sql) tuple.
const MIGRATIONS: &[(i64, &str)] = &[(1, MIGRATION_0001)];

/// Applies all pending migrations to the given connection. Creates the
/// `schema_version` tracking table if it does not exist, then executes each
/// migration whose version exceeds the current schema version.
///
/// # Errors
///
/// Returns `DbError::Query` if any migration SQL fails to execute.
pub fn run_migrations(conn: &rusqlite::Connection) -> Result<(), DbError> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_version (
            version INTEGER PRIMARY KEY,
            applied_at TEXT NOT NULL DEFAULT (datetime('now'))
        );",
    )
    .map_err(|e| DbError::Query {
        operation: "create_schema_version_table".to_owned(),
        source: e,
    })?;

    let current_version: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_version",
            [],
            |row| row.get(0),
        )
        .map_err(|e| DbError::Query {
            operation: "read_schema_version".to_owned(),
            source: e,
        })?;

    for &(version, sql) in MIGRATIONS {
        if version > current_version {
            conn.execute_batch("BEGIN IMMEDIATE;")
                .map_err(|e| DbError::Query {
                    operation: format!("begin_migration_{version:04}"),
                    source: e,
                })?;

            conn.execute_batch(sql).map_err(|e| {
                let _ = conn.execute_batch("ROLLBACK;");
                DbError::Query {
                    operation: format!("migration_{version:04}"),
                    source: e,
                }
            })?;

            conn.execute(
                "INSERT INTO schema_version (version) VALUES (?1)",
                [version],
            )
            .map_err(|e| {
                let _ = conn.execute_batch("ROLLBACK;");
                DbError::Query {
                    operation: format!("record_migration_{version:04}"),
                    source: e,
                }
            })?;

            conn.execute_batch("COMMIT;").map_err(|e| DbError::Query {
                operation: format!("commit_migration_{version:04}"),
                source: e,
            })?;
        }
    }

    Ok(())
}
