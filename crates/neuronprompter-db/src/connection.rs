// =============================================================================
// `SQLite` connection management.
//
// The Database struct holds a Mutex-wrapped rusqlite::Connection and exposes
// methods for mutex-guarded access and transaction-wrapped operations. On
// construction, it applies performance pragmas (WAL journal mode, foreign
// keys, synchronous NORMAL, 5-second busy timeout) and runs pending schema
// migrations.
// =============================================================================

use std::path::Path;
use std::sync::{Arc, Mutex};

use crate::connection_provider::ConnectionProvider;
use crate::error::DbError;
use crate::migrations;

/// Thread-safe wrapper around a single `SQLite` connection. All access goes
/// through the internal `Mutex` to serialize database operations.
pub struct Database {
    conn: Mutex<rusqlite::Connection>,
}

impl Database {
    /// Opens a file-backed `SQLite` database at the given path, applies pragmas,
    /// and runs any pending migrations.
    ///
    /// # Errors
    ///
    /// Returns `DbError::Query` if the connection cannot be opened or pragmas
    /// fail to apply.
    pub fn open(path: &Path) -> Result<Self, DbError> {
        let conn = rusqlite::Connection::open(path).map_err(|e| DbError::Query {
            operation: "open".to_owned(),
            source: e,
        })?;
        Self::initialize(conn)
    }

    /// Opens an in-memory `SQLite` database for testing. Applies the same
    /// pragmas and migrations as a file-backed database.
    ///
    /// # Errors
    ///
    /// Returns `DbError::Query` if pragmas fail to apply.
    pub fn open_in_memory() -> Result<Self, DbError> {
        let conn = rusqlite::Connection::open_in_memory().map_err(|e| DbError::Query {
            operation: "open_in_memory".to_owned(),
            source: e,
        })?;
        Self::initialize(conn)
    }

    /// Executes a closure with a reference to the underlying connection while
    /// holding the mutex lock. This is the primary access method for read-only
    /// database operations that do not require transactional guarantees.
    ///
    /// # Errors
    ///
    /// Returns `DbError::MutexPoisoned` if the mutex is poisoned, or propagates
    /// any error returned by the closure.
    pub fn with_connection_raw<F, T>(&self, f: F) -> Result<T, DbError>
    where
        F: FnOnce(&rusqlite::Connection) -> Result<T, DbError>,
    {
        let conn = self
            .conn
            .lock()
            .map_err(|e| DbError::MutexPoisoned(e.to_string()))?;
        f(&conn)
    }

    /// Applies connection pragmas and runs schema migrations. Called once by
    /// both `open` and `open_in_memory`.
    fn initialize(conn: rusqlite::Connection) -> Result<Self, DbError> {
        conn.execute_batch(
            "PRAGMA encoding = 'UTF-8';
             PRAGMA journal_mode = WAL;
             PRAGMA foreign_keys = ON;
             PRAGMA synchronous = NORMAL;
             PRAGMA busy_timeout = 5000;",
        )
        .map_err(|e| DbError::Query {
            operation: "set_pragmas".to_owned(),
            source: e,
        })?;

        migrations::run_migrations(&conn)?;

        Ok(Self {
            conn: Mutex::new(conn),
        })
    }
}

impl ConnectionProvider for Database {
    fn with_connection<F, T>(&self, f: F) -> Result<T, DbError>
    where
        F: FnOnce(&rusqlite::Connection) -> Result<T, DbError>,
    {
        self.with_connection_raw(f)
    }

    fn with_transaction<F, T>(&self, f: F) -> Result<T, DbError>
    where
        F: FnOnce(&rusqlite::Connection) -> Result<T, DbError>,
    {
        let conn = self
            .conn
            .lock()
            .map_err(|e| DbError::MutexPoisoned(e.to_string()))?;
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

/// Blanket implementation for `Arc<Database>` so the MCP server can pass
/// `Arc<Database>` directly to service functions.
impl ConnectionProvider for Arc<Database> {
    fn with_connection<F, T>(&self, f: F) -> Result<T, DbError>
    where
        F: FnOnce(&rusqlite::Connection) -> Result<T, DbError>,
    {
        (**self).with_connection(f)
    }

    fn with_transaction<F, T>(&self, f: F) -> Result<T, DbError>
    where
        F: FnOnce(&rusqlite::Connection) -> Result<T, DbError>,
    {
        (**self).with_transaction(f)
    }
}
