// =============================================================================
// Service layer error type.
//
// ServiceError aggregates all error sources that can occur during service
// operations: domain validation (CoreError), database failures (DbError),
// Ollama connectivity/response issues, file I/O, and serialization problems.
// =============================================================================

use neuronprompter_core::CoreError;
use neuronprompter_db::DbError;

/// Error type for all service-layer operations. Each variant maps to a
/// specific IPC error code in the app crate.
#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    /// A domain validation or not-found error from the core crate.
    #[error(transparent)]
    Core(#[from] CoreError),

    /// A database operation failed.
    #[error(transparent)]
    Database(#[from] DbError),

    /// The Ollama HTTP server is not reachable or returned a non-200 status.
    #[error("Ollama unavailable: {0}")]
    OllamaUnavailable(String),

    /// The Ollama server returned a response that could not be parsed.
    #[error("Ollama error: {0}")]
    OllamaError(String),

    /// A filesystem read, write, or copy operation failed.
    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),

    /// JSON or YAML serialization/deserialization failed.
    #[error("Serialization error: {0}")]
    SerializationError(String),
}
