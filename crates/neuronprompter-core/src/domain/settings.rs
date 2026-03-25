// =============================================================================
// Application and user settings domain types.
//
// AppSetting stores global key-value pairs (e.g., last_user_id).
// UserSettings stores per-user preferences such as theme, default model,
// Ollama base URL, and sorting preferences. Theme, SortField, and
// SortDirection are serialized as lowercase strings for SQLite storage.
// =============================================================================

use serde::{Deserialize, Serialize};

/// A single application-wide setting stored as a key-value pair in the
/// `app_settings` table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSetting {
    pub key: String,
    pub value: String,
}

/// Per-user preferences loaded at user switch and applied to the UI and
/// service layer behavior. The `user_id` column is the primary key (1:1
/// relationship with the `users` table).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserSettings {
    pub user_id: i64,
    pub theme: Theme,
    pub last_collection_id: Option<i64>,
    pub sidebar_collapsed: bool,
    pub sort_field: SortField,
    pub sort_direction: SortDirection,
    pub ollama_base_url: String,
    pub ollama_model: Option<String>,
    pub extra: String,
}

/// Color scheme selection. System delegates to the operating system's
/// current light/dark preference.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    Light,
    Dark,
    System,
}

/// Column used for ordering prompt lists in the UI.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SortField {
    UpdatedAt,
    CreatedAt,
    Title,
}

/// Ascending or descending order for prompt list sorting.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SortDirection {
    Asc,
    Desc,
}
