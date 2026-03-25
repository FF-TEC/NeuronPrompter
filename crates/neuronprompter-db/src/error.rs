// =============================================================================
// Database error type.
//
// DbError wraps rusqlite errors with operation context and converts from
// CoreError for cases where validation failures originate in repository logic.
// =============================================================================

use neuronprompter_core::CoreError;

/// Error type for all database operations. Wraps the underlying `rusqlite` error
/// together with a description of the operation that failed.
#[derive(Debug, thiserror::Error)]
pub enum DbError {
    /// A SQL query or statement execution failed.
    #[error("Database {operation} failed: {source}")]
    Query {
        // The operation field stores the name of the database function that failed.
        // Using String instead of &'static str accommodates dynamically constructed
        // operation names (e.g. savepoint-related operations). A future refactor
        // could use Cow<'static, str> to avoid heap allocation for the common case
        // of static string literals.
        operation: String,
        source: rusqlite::Error,
    },

    /// A domain validation error propagated through the database layer.
    #[error(transparent)]
    Core(#[from] CoreError),

    /// Connection pool initialization or checkout failure.
    #[error("Connection pool error: {0}")]
    Pool(String),

    /// A Mutex or RwLock was poisoned (a thread panicked while holding the lock).
    #[error("Mutex poisoned: {0}")]
    MutexPoisoned(String),
}
