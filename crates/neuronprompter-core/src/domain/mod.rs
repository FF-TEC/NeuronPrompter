// =============================================================================
// Domain model sub-modules.
//
// Each module defines the structs representing a single aggregate or entity
// from the NeuronPrompter data model. All types derive Serialize, Deserialize,
// Debug, and Clone for IPC transport and persistence.
// =============================================================================

pub mod category;
pub mod chain;
pub mod collection;
pub mod prompt;
pub mod script;
pub mod script_version;
pub mod settings;
pub mod tag;
pub mod user;
pub mod version;

use serde::{Deserialize, Serialize};

/// Generic wrapper adding pagination metadata to list responses.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginatedList<T> {
    pub items: Vec<T>,
    pub total: i64,
    pub has_more: bool,
}
