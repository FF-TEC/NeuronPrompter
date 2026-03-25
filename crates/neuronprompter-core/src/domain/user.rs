// =============================================================================
// User domain entity.
//
// Represents an application user who owns prompts, tags, categories, and
// collections. The username field is unique and validated for lowercase
// alphanumeric characters plus underscores.
// =============================================================================

use serde::{Deserialize, Serialize};

/// Persisted user record with database-assigned identifier and timestamps.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: i64,
    pub username: String,
    pub display_name: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Payload for creating a user. Contains only the fields supplied by the caller;
/// id and timestamps are assigned by the database layer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewUser {
    pub username: String,
    pub display_name: String,
}
