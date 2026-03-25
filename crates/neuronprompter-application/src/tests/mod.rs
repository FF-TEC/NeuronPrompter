// =============================================================================
// Integration tests for the neuronprompter-application crate.
//
// All tests use an in-memory SQLite database with migrations applied,
// verifying service-layer workflows (versioning, association sync,
// import/export, error propagation) against a real ephemeral schema.
// =============================================================================

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

mod test_chain_service;
mod test_io_service;
mod test_ollama;
mod test_prompt_service;
mod test_script_service;
mod test_script_version_service;
mod test_search_service;
mod test_user_service;
mod test_version_service;

use neuronprompter_core::domain::prompt::NewPrompt;
use neuronprompter_core::domain::script::NewScript;
use neuronprompter_core::domain::user::NewUser;
use neuronprompter_db::Database;

use crate::user_service;

/// Creates a fresh in-memory database with all migrations applied.
fn setup_db() -> Database {
    Database::open_in_memory().expect("in-memory database creation must succeed")
}

/// Creates a user through the service layer (including default settings).
fn create_test_user(db: &Database, username: &str) -> i64 {
    let new = NewUser {
        username: username.to_owned(),
        display_name: format!("Display {username}"),
    };
    user_service::create_user(db, &new)
        .expect("test user creation")
        .id
}

/// Builds a minimal `NewPrompt` for testing.
fn make_prompt(user_id: i64, title: &str) -> NewPrompt {
    NewPrompt {
        user_id,
        title: title.to_owned(),
        content: format!("Content for {title}"),
        description: None,
        notes: None,
        language: None,
        tag_ids: Vec::new(),
        category_ids: Vec::new(),
        collection_ids: Vec::new(),
    }
}

/// Builds a minimal `NewScript` for testing.
fn make_script(user_id: i64, title: &str, lang: &str) -> NewScript {
    NewScript {
        user_id,
        title: title.to_owned(),
        content: format!("# {title}"),
        script_language: lang.to_owned(),
        description: None,
        notes: None,
        language: None,
        source_path: None,
        is_synced: false,
        tag_ids: Vec::new(),
        category_ids: Vec::new(),
        collection_ids: Vec::new(),
    }
}
