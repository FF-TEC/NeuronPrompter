// =============================================================================
// Core error types.
//
// CoreError defines the domain-level error variants shared across all crates.
// These errors carry structured context (field names, entity types, IDs) to
// enable precise error reporting and mapping to IPC error codes in the app crate.
// =============================================================================

/// Domain-level error type used throughout the `NeuronPrompter` crate hierarchy.
/// Each variant carries enough context for the app layer to map it to a
/// specific IPC error code and user-facing message.
#[derive(Debug, Clone, thiserror::Error)]
pub enum CoreError {
    /// A field value failed validation rules (e.g., empty title, username
    /// with invalid characters).
    #[error("Validation error on field '{field}': {message}")]
    Validation {
        // Entity and field names use String to support arbitrary domain types
        // without requiring a shared enum. A future improvement could introduce
        // a DomainEntity enum for compile-time safety.
        field: String,
        message: String,
    },

    /// The requested entity does not exist in the database or is not
    /// accessible to the current user.
    #[error("{entity} with id {id} not found")]
    NotFound {
        // Entity and field names use String to support arbitrary domain types
        // without requiring a shared enum. A future improvement could introduce
        // a DomainEntity enum for compile-time safety.
        entity: String,
        id: i64,
    },

    /// An insert or update would violate a uniqueness constraint (e.g.,
    /// duplicate tag name within the same user namespace).
    #[error("Duplicate {entity}: {field} '{value}' already exists")]
    Duplicate {
        entity: String,
        field: String,
        value: String,
    },

    /// An entity cannot be deleted because it is referenced by one or more
    /// other entities (e.g., a prompt or script used in chains).
    #[error("{entity_type} {entity_id} is in use by: {}", referencing_titles.join(", "))]
    EntityInUse {
        entity_type: String,
        entity_id: i64,
        referencing_titles: Vec<String>,
    },

    /// The caller does not have permission to perform the requested operation.
    #[error("Authorization: {message}")]
    Authorization { message: String },

    /// A filesystem path failed sandboxing validation (directory traversal,
    /// symlink escape, or access outside the allowed directory).
    #[error("Path access denied: {path}")]
    PathTraversal { path: String },

    /// An update failed because the entity was modified concurrently.
    /// The caller should reload and retry.
    #[error("Conflict: {entity} {id} was modified (expected version {expected}, actual {actual})")]
    Conflict {
        entity: String,
        id: i64,
        expected: i64,
        actual: i64,
    },
}
