// =============================================================================
// Category domain entity.
//
// Categories classify prompts into semantic groups (e.g., "Translation",
// "Summarization"). A category name must be unique within a single user's
// namespace. Categories are linked to prompts through the prompt_categories
// junction table.
// =============================================================================

use serde::{Deserialize, Serialize};

/// Persisted category record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Category {
    pub id: i64,
    pub user_id: i64,
    pub name: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Payload for creating a category.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewCategory {
    pub user_id: i64,
    pub name: String,
}
