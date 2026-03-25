// =============================================================================
// Prompt domain entity.
//
// The central aggregate of NeuronPrompter. A prompt stores the user-authored
// text content along with metadata (title, description, language, etc.) and
// maintains a version counter incremented on every content or metadata change.
// =============================================================================

use serde::{Deserialize, Serialize};

use super::category::Category;
use super::collection::Collection;
use super::tag::Tag;

/// Persisted prompt record containing all columns from the prompts table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Prompt {
    pub id: i64,
    pub user_id: i64,
    pub title: String,
    pub content: String,
    pub description: Option<String>,
    pub notes: Option<String>,
    pub language: Option<String>,
    pub is_favorite: bool,
    pub is_archived: bool,
    pub current_version: i64,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Payload for creating a prompt. Optional fields default to NULL in the
/// database. Association ID vectors specify which tags, categories, and
/// collections to link via junction tables.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewPrompt {
    pub user_id: i64,
    pub title: String,
    pub content: String,
    pub description: Option<String>,
    pub notes: Option<String>,
    pub language: Option<String>,
    #[serde(default)]
    pub tag_ids: Vec<i64>,
    #[serde(default)]
    pub category_ids: Vec<i64>,
    #[serde(default)]
    pub collection_ids: Vec<i64>,
}

/// Payload for updating a prompt. All fields are optional; only supplied fields
/// are modified. Association ID vectors replace the full set of linked entities
/// when provided.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdatePrompt {
    pub prompt_id: i64,
    pub title: Option<String>,
    pub content: Option<String>,
    #[serde(
        default,
        deserialize_with = "crate::serde_helpers::deserialize_optional_field"
    )]
    pub description: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "crate::serde_helpers::deserialize_optional_field"
    )]
    pub notes: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "crate::serde_helpers::deserialize_optional_field"
    )]
    pub language: Option<Option<String>>,
    pub tag_ids: Option<Vec<i64>>,
    pub category_ids: Option<Vec<i64>>,
    pub collection_ids: Option<Vec<i64>>,
    /// F10: Optional expected version for optimistic concurrency control.
    /// When set, the update only succeeds if the current version matches.
    #[serde(default)]
    pub expected_version: Option<i64>,
}

/// A prompt together with its resolved associations. Used for detail views
/// and export operations where the full entity graph is needed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptWithAssociations {
    pub prompt: Prompt,
    pub tags: Vec<Tag>,
    pub categories: Vec<Category>,
    pub collections: Vec<Collection>,
}

/// Filter criteria for listing prompts. All fields are optional and combined
/// with AND semantics. Unset fields impose no restriction.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PromptFilter {
    pub user_id: Option<i64>,
    pub is_favorite: Option<bool>,
    pub is_archived: Option<bool>,
    pub collection_id: Option<i64>,
    pub category_id: Option<i64>,
    pub tag_id: Option<i64>,
    /// When `true`, only return prompts whose content contains template
    /// variables (`{{...}}`). When `false`, only prompts without variables.
    #[serde(default)]
    pub has_variables: Option<bool>,
    /// Filter to prompts whose content contains a specific template variable
    /// (e.g. `"topic"` matches prompts containing `{{topic}}`).
    #[serde(default)]
    pub variable_name: Option<String>,
    /// Maximum number of results to return (default 200, max 1000).
    #[serde(default)]
    pub limit: Option<i64>,
    /// Number of results to skip for pagination.
    #[serde(default)]
    pub offset: Option<i64>,
}
