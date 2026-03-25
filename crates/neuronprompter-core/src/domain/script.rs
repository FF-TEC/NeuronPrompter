// =============================================================================
// Script domain entity.
//
// A script stores a code file of any programming language as a first-class
// entity alongside prompts. Each script has a required programming language
// identifier (script_language), full metadata, taxonomy associations, and
// maintains a version counter incremented on every content or metadata change.
// =============================================================================

use serde::{Deserialize, Serialize};

use super::category::Category;
use super::collection::Collection;
use super::tag::Tag;

/// Persisted script record containing all columns from the scripts table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Script {
    pub id: i64,
    pub user_id: i64,
    pub title: String,
    pub content: String,
    pub description: Option<String>,
    pub notes: Option<String>,
    pub script_language: String,
    pub language: Option<String>,
    pub is_favorite: bool,
    pub is_archived: bool,
    pub current_version: i64,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub source_path: Option<String>,
    pub is_synced: bool,
}

/// Payload for creating a script. The `script_language` field is required and
/// identifies the programming language (e.g., "python", "bash", "javascript").
/// Association ID vectors specify which tags, categories, and collections to
/// link via junction tables.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewScript {
    pub user_id: i64,
    pub title: String,
    pub content: String,
    pub script_language: String,
    pub description: Option<String>,
    pub notes: Option<String>,
    pub language: Option<String>,
    #[serde(default)]
    pub source_path: Option<String>,
    #[serde(default)]
    pub is_synced: bool,
    #[serde(default)]
    pub tag_ids: Vec<i64>,
    #[serde(default)]
    pub category_ids: Vec<i64>,
    #[serde(default)]
    pub collection_ids: Vec<i64>,
}

/// Payload for updating a script. All fields are optional; only supplied fields
/// are modified. Association ID vectors replace the full set of linked entities
/// when provided.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateScript {
    pub script_id: i64,
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
    pub script_language: Option<String>,
    #[serde(
        default,
        deserialize_with = "crate::serde_helpers::deserialize_optional_field"
    )]
    pub language: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "crate::serde_helpers::deserialize_optional_field"
    )]
    pub source_path: Option<Option<String>>,
    pub is_synced: Option<bool>,
    pub tag_ids: Option<Vec<i64>>,
    pub category_ids: Option<Vec<i64>>,
    pub collection_ids: Option<Vec<i64>>,
    /// F10: Optional expected version for optimistic concurrency control.
    #[serde(default)]
    pub expected_version: Option<i64>,
}

/// A script together with its resolved associations. Used for detail views
/// and export operations where the full entity graph is needed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScriptWithAssociations {
    pub script: Script,
    pub tags: Vec<Tag>,
    pub categories: Vec<Category>,
    pub collections: Vec<Collection>,
}

/// Filter criteria for listing scripts. All fields are optional and combined
/// with AND semantics. Unset fields impose no restriction.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ScriptFilter {
    pub user_id: Option<i64>,
    pub is_favorite: Option<bool>,
    pub is_archived: Option<bool>,
    pub collection_id: Option<i64>,
    pub category_id: Option<i64>,
    pub tag_id: Option<i64>,
    pub is_synced: Option<bool>,
    /// Maximum number of results to return (default 200, max 1000).
    #[serde(default)]
    pub limit: Option<i64>,
    /// Number of results to skip for pagination.
    #[serde(default)]
    pub offset: Option<i64>,
}
