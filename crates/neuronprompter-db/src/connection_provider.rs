// =============================================================================
// ConnectionProvider trait: unified abstraction over Database and DbPool.
//
// This trait allows service-layer code to be written once against a generic
// connection source, eliminating the need for duplicate `_with_conn` variants.
// Both the single-connection `Database` wrapper (used by MCP) and the r2d2
// `DbPool` (used by the API server) implement this trait.
// =============================================================================

use crate::error::DbError;

/// Unified abstraction over different SQLite connection sources.
///
/// Implementors provide access to a `rusqlite::Connection` either through a
/// `Mutex`-guarded single connection (`Database`) or through an `r2d2`
/// connection pool (`DbPool`).
pub trait ConnectionProvider {
    /// Executes a closure with a borrowed connection.
    ///
    /// For `Database`, this acquires the mutex lock.
    /// For `DbPool`, this checks out a connection from the pool.
    ///
    /// # Errors
    ///
    /// Returns `DbError::MutexPoisoned` or `DbError::Pool` if the connection
    /// cannot be acquired, or propagates any error returned by the closure.
    fn with_connection<F, T>(&self, f: F) -> Result<T, DbError>
    where
        F: FnOnce(&rusqlite::Connection) -> Result<T, DbError>;

    /// Executes a closure inside a transaction (`BEGIN IMMEDIATE` / `COMMIT` / `ROLLBACK`).
    ///
    /// On success the transaction is committed. On error the transaction is
    /// rolled back and the original error is returned. Rollback failures are
    /// logged but do not replace the original error.
    ///
    /// # Errors
    ///
    /// Returns `DbError::MutexPoisoned` or `DbError::Pool` if the connection
    /// cannot be acquired, `DbError::Query` if the transaction control
    /// statements fail, or propagates any error returned by the closure.
    fn with_transaction<F, T>(&self, f: F) -> Result<T, DbError>
    where
        F: FnOnce(&rusqlite::Connection) -> Result<T, DbError>;
}
