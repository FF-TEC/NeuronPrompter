// =============================================================================
// Collection domain entity.
//
// Collections group prompts into user-defined sets, analogous to folders or
// playlists. A collection name must be unique within a single user's namespace.
// Collections are linked to prompts through the prompt_collections junction table.
// =============================================================================

use serde::{Deserialize, Serialize};

/// Persisted collection record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Collection {
    pub id: i64,
    pub user_id: i64,
    pub name: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Payload for creating a collection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewCollection {
    pub user_id: i64,
    pub name: String,
}
