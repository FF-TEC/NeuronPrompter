// =============================================================================
// REST API handler modules.
//
// Each module defines axum handler functions for one domain area, replacing
// the former Tauri IPC command handlers with standard HTTP endpoints.
// =============================================================================

pub mod categories;
pub mod chains;
pub mod clipboard;
pub mod collections;
pub mod common;
pub mod copy;
pub mod health;
pub mod io;
pub mod ollama;
pub mod prompts;
pub mod script_versions;
pub mod scripts;
pub mod search;
pub mod sessions;
pub mod settings;
pub mod shutdown;
pub mod tags;
pub mod users;
pub mod versions;

/// Shared payload for boolean toggle endpoints (favorite, archive).
/// When `value` is omitted, the handler reads the current state and inverts it.
#[derive(serde::Deserialize, Default)]
pub struct TogglePayload {
    pub value: Option<bool>,
}

/// Maximum number of IDs allowed in a single bulk-update request.
pub const MAX_BULK_IDS: usize = 1000;

/// Shared payload for bulk-update endpoints across prompts, scripts, and chains.
/// Only set fields are applied; all operations run in a single transaction.
#[derive(serde::Deserialize)]
pub struct BulkUpdatePayload {
    pub ids: Vec<i64>,
    pub set_favorite: Option<bool>,
    pub set_archived: Option<bool>,
    #[serde(default)]
    pub add_tag_ids: Vec<i64>,
    #[serde(default)]
    pub remove_tag_ids: Vec<i64>,
    #[serde(default)]
    pub add_category_ids: Vec<i64>,
    #[serde(default)]
    pub remove_category_ids: Vec<i64>,
    #[serde(default)]
    pub add_collection_ids: Vec<i64>,
    #[serde(default)]
    pub remove_collection_ids: Vec<i64>,
}
