// =============================================================================
// Tag domain entity.
//
// Tags provide free-form, user-scoped labels for organizing prompts. A tag
// name must be unique within a single user's namespace. Tags are linked to
// prompts through the prompt_tags junction table.
// =============================================================================

use serde::{Deserialize, Serialize};

/// Persisted tag record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tag {
    pub id: i64,
    pub user_id: i64,
    pub name: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Payload for creating a tag. The name is validated for uniqueness per user
/// in the service layer before insertion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewTag {
    pub user_id: i64,
    pub name: String,
}
