// =============================================================================
// Script version domain entity.
//
// Each row in script_versions captures an immutable snapshot of a script's
// content and metadata at the time of a modification. Versions are
// sequentially numbered per script and created automatically by the
// application service layer before applying updates.
// =============================================================================

use serde::{Deserialize, Serialize};

/// Immutable snapshot of a script's state at a specific version number.
/// The content and metadata fields reflect the script's values immediately
/// before the update that incremented the version counter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScriptVersion {
    pub id: i64,
    pub script_id: i64,
    pub version_number: i64,
    pub title: String,
    pub content: String,
    pub description: Option<String>,
    pub notes: Option<String>,
    pub script_language: String,
    pub language: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}
