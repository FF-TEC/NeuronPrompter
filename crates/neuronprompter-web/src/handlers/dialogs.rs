// =============================================================================
// Native file dialog endpoints.
//
// Opens OS-native save/open file dialogs via the rfd crate and returns the
// selected path as JSON. Falls back gracefully when compiled without the
// gui feature (rfd unavailable) by returning null.
// =============================================================================

use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use serde::{Deserialize, Serialize};

use crate::WebState;

#[derive(Deserialize)]
pub struct DialogRequest {
    /// Dialog window title.
    pub title: Option<String>,
    /// File extension filters, e.g. `["json"]` or `["db", "sqlite"]`.
    pub filters: Option<Vec<DialogFilter>>,
    /// Default file name for save dialogs.
    pub default_name: Option<String>,
}

#[derive(Deserialize)]
pub struct DialogFilter {
    pub name: String,
    pub extensions: Vec<String>,
}

#[derive(Serialize)]
pub struct DialogResult {
    /// The selected file/directory path, or null if the user cancelled.
    pub path: Option<String>,
}

/// POST /api/v1/web/dialog/save
///
/// Opens a native save-file dialog and returns the chosen path.
pub async fn save_dialog(
    State(_state): State<Arc<WebState>>,
    Json(body): Json<DialogRequest>,
) -> Json<DialogResult> {
    Json(DialogResult {
        path: open_save_dialog(&body),
    })
}

/// POST /api/v1/web/dialog/open-file
///
/// Opens a native open-file dialog and returns the chosen path.
pub async fn open_file_dialog(
    State(_state): State<Arc<WebState>>,
    Json(body): Json<DialogRequest>,
) -> Json<DialogResult> {
    Json(DialogResult {
        path: open_file_dialog_impl(&body),
    })
}

/// POST /api/v1/web/dialog/open-dir
///
/// Opens a native directory picker and returns the chosen path.
pub async fn open_dir_dialog(State(_state): State<Arc<WebState>>) -> Json<DialogResult> {
    Json(DialogResult {
        path: open_dir_dialog_impl(),
    })
}

// ---------------------------------------------------------------------------
// rfd implementations (compiled only with gui feature)
// ---------------------------------------------------------------------------

#[cfg(feature = "gui")]
fn open_save_dialog(req: &DialogRequest) -> Option<String> {
    let mut dialog = rfd::FileDialog::new();
    if let Some(title) = &req.title {
        dialog = dialog.set_title(title);
    }
    if let Some(name) = &req.default_name {
        dialog = dialog.set_file_name(name);
    }
    if let Some(filters) = &req.filters {
        for f in filters {
            let exts: Vec<&str> = f.extensions.iter().map(String::as_str).collect();
            dialog = dialog.add_filter(&f.name, &exts);
        }
    }
    dialog.save_file().map(|p| p.display().to_string())
}

#[cfg(feature = "gui")]
fn open_file_dialog_impl(req: &DialogRequest) -> Option<String> {
    let mut dialog = rfd::FileDialog::new();
    if let Some(title) = &req.title {
        dialog = dialog.set_title(title);
    }
    if let Some(filters) = &req.filters {
        for f in filters {
            let exts: Vec<&str> = f.extensions.iter().map(String::as_str).collect();
            dialog = dialog.add_filter(&f.name, &exts);
        }
    }
    dialog.pick_file().map(|p| p.display().to_string())
}

#[cfg(feature = "gui")]
fn open_dir_dialog_impl() -> Option<String> {
    rfd::FileDialog::new()
        .set_title("Select Directory")
        .pick_folder()
        .map(|p| p.display().to_string())
}

// ---------------------------------------------------------------------------
// Fallbacks when gui feature is not compiled
// ---------------------------------------------------------------------------

#[cfg(not(feature = "gui"))]
fn open_save_dialog(_req: &DialogRequest) -> Option<String> {
    None
}

#[cfg(not(feature = "gui"))]
fn open_file_dialog_impl(_req: &DialogRequest) -> Option<String> {
    None
}

#[cfg(not(feature = "gui"))]
fn open_dir_dialog_impl() -> Option<String> {
    None
}
