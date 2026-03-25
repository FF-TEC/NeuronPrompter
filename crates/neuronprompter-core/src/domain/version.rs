// =============================================================================
// Prompt version domain entity.
//
// Each row in prompt_versions captures an immutable snapshot of a prompt's
// content and metadata at the time of a modification. Versions are
// sequentially numbered per prompt and created automatically by the
// application service layer before applying updates.
// =============================================================================

use serde::{Deserialize, Serialize};

/// Immutable snapshot of a prompt's state at a specific version number.
/// The content and metadata fields reflect the prompt's values immediately
/// before the update that incremented the version counter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptVersion {
    pub id: i64,
    pub prompt_id: i64,
    pub version_number: i64,
    pub title: String,
    pub content: String,
    pub description: Option<String>,
    pub notes: Option<String>,
    pub language: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}
