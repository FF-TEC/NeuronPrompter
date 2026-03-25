// =============================================================================
// r2d2 connection pool for SQLite.
//
// Provides a thread-safe connection pool that applies the same pragmas as
// the single-connection `Database` wrapper (WAL mode, foreign keys,
// synchronous NORMAL, 5-second busy timeout). Each pooled connection runs
// through a `ConnectionCustomizer` that sets pragmas on checkout.
// =============================================================================

use r2d2::{Pool, PooledConnection};
use r2d2_sqlite::SqliteConnectionManager;

use crate::connection_provider::ConnectionProvider;
use crate::error::DbError;
use crate::migrations;

/// Type alias for the r2d2 SQLite connection pool.
pub type DbPool = Pool<SqliteConnectionManager>;

/// Type alias for a pooled connection obtained from the pool.
pub type PooledConn = PooledConnection<SqliteConnectionManager>;

/// Default number of connections in the pool. SQLite WAL mode permits
/// concurrent readers but only a single writer at any given time. Eight
/// connections provide one writer plus seven concurrent readers, which
/// reduces contention under concurrent HTTP load without excessive
/// write-lock pressure.
const DEFAULT_POOL_SIZE: u32 = 8;

/// Custom r2d2 connection initializer that applies SQLite pragmas
/// to each connection created by the pool.
#[derive(Debug)]
struct PragmaCustomizer;

impl r2d2::CustomizeConnection<rusqlite::Connection, rusqlite::Error> for PragmaCustomizer {
    fn on_acquire(&self, conn: &mut rusqlite::Connection) -> Result<(), rusqlite::Error> {
        conn.execute_batch(
            "PRAGMA encoding = 'UTF-8';
             PRAGMA journal_mode = WAL;
             PRAGMA foreign_keys = ON;
             PRAGMA synchronous = NORMAL;
             PRAGMA busy_timeout = 5000;
             PRAGMA cache_size = -8000;",
        )?;
        // cache_size = -8000 sets the per-connection page cache to 8 MB
        // (negative value denotes KiB). This larger cache improves
        // performance for FTS5 full-text queries and multi-table JOINs
        // that touch many pages in a single statement.
        Ok(())
    }
}

/// Creates a connection pool with the default pool size.
///
/// # Errors
///
/// Returns `DbError::Pool` if the pool cannot be created or if migrations fail.
pub fn create_pool(path: &std::path::Path) -> Result<DbPool, DbError> {
    create_pool_with_size(path, DEFAULT_POOL_SIZE)
}

/// Creates a connection pool with a custom pool size.
///
/// The first connection from the pool runs schema migrations before the pool
/// is returned to the caller. Subsequent connections skip migrations because
/// the schema is already up to date.
///
/// # Errors
///
/// Returns `DbError::Pool` if the pool cannot be created or if migrations fail.
pub fn create_pool_with_size(path: &std::path::Path, size: u32) -> Result<DbPool, DbError> {
    let manager = SqliteConnectionManager::file(path);
    let pool = Pool::builder()
        .max_size(size)
        .connection_timeout(std::time::Duration::from_secs(10))
        .connection_customizer(Box::new(PragmaCustomizer))
        .build(manager)
        .map_err(|e| DbError::Pool(e.to_string()))?;

    // Run migrations on the first connection.
    {
        let conn = pool.get().map_err(|e| DbError::Pool(e.to_string()))?;
        migrations::run_migrations(&conn)?;
    }

    Ok(pool)
}

impl ConnectionProvider for DbPool {
    fn with_connection<F, T>(&self, f: F) -> Result<T, DbError>
    where
        F: FnOnce(&rusqlite::Connection) -> Result<T, DbError>,
    {
        let conn = self.get().map_err(|e| DbError::Pool(e.to_string()))?;
        f(&conn)
    }

    fn with_transaction<F, T>(&self, f: F) -> Result<T, DbError>
    where
        F: FnOnce(&rusqlite::Connection) -> Result<T, DbError>,
    {
        let conn = self.get().map_err(|e| DbError::Pool(e.to_string()))?;
        conn.execute_batch("BEGIN IMMEDIATE")
            .map_err(|e| DbError::Query {
                operation: "begin_transaction".to_owned(),
                source: e,
            })?;
        match f(&conn) {
            Ok(result) => {
                conn.execute_batch("COMMIT").map_err(|e| DbError::Query {
                    operation: "commit_transaction".to_owned(),
                    source: e,
                })?;
                Ok(result)
            }
            Err(err) => {
                if let Err(rb_err) = conn.execute_batch("ROLLBACK") {
                    tracing::error!("transaction rollback failed: {rb_err}");
                }
                Err(err)
            }
        }
    }
}

/// Implementation for individual pooled connections, used in handlers that
/// need to perform multiple operations on the same connection (e.g., bulk updates
/// with manual transaction management).
impl ConnectionProvider for PooledConn {
    fn with_connection<F, T>(&self, f: F) -> Result<T, DbError>
    where
        F: FnOnce(&rusqlite::Connection) -> Result<T, DbError>,
    {
        f(self)
    }

    fn with_transaction<F, T>(&self, f: F) -> Result<T, DbError>
    where
        F: FnOnce(&rusqlite::Connection) -> Result<T, DbError>,
    {
        self.execute_batch("BEGIN IMMEDIATE")
            .map_err(|e| DbError::Query {
                operation: "begin_transaction".to_owned(),
                source: e,
            })?;
        match f(self) {
            Ok(result) => {
                self.execute_batch("COMMIT").map_err(|e| DbError::Query {
                    operation: "commit_transaction".to_owned(),
                    source: e,
                })?;
                Ok(result)
            }
            Err(err) => {
                if let Err(rb_err) = self.execute_batch("ROLLBACK") {
                    tracing::error!("transaction rollback failed: {rb_err}");
                }
                Err(err)
            }
        }
    }
}

/// Creates an in-memory connection pool for testing. Uses a single connection
/// to preserve the in-memory database state across calls.
///
/// **Important:** `max_size` is set to 1 because each `SqliteConnectionManager::memory()`
/// connection creates an independent in-memory database. With pool size > 1,
/// connections would not share state.
///
/// # Errors
///
/// Returns `DbError::Pool` if the pool cannot be created or if migrations fail.
pub fn create_in_memory_pool() -> Result<DbPool, DbError> {
    let manager = SqliteConnectionManager::memory();
    let pool = Pool::builder()
        .max_size(1)
        .connection_customizer(Box::new(PragmaCustomizer))
        .build(manager)
        .map_err(|e| DbError::Pool(e.to_string()))?;

    {
        let conn = pool.get().map_err(|e| DbError::Pool(e.to_string()))?;
        migrations::run_migrations(&conn)?;
    }

    Ok(pool)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

    use super::*;

    #[test]
    fn in_memory_pool_applies_pragmas_and_migrations() {
        let pool = create_in_memory_pool().expect("pool creation should succeed");
        let conn = pool.get().expect("connection should be available");

        // Verify WAL mode is set.
        let journal_mode: String = conn
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .unwrap();
        // In-memory databases report "memory" for journal_mode regardless of
        // the WAL pragma. This is expected behavior.
        assert!(
            journal_mode == "wal" || journal_mode == "memory",
            "unexpected journal mode: {journal_mode}"
        );

        // Verify foreign keys are enabled.
        let fk_enabled: i64 = conn
            .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
            .unwrap();
        assert_eq!(fk_enabled, 1);

        // Verify schema_version table exists (proof migrations ran).
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='schema_version'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }
}
