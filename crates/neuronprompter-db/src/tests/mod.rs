// =============================================================================
// Integration tests for the neuronprompter-db crate.
//
// All tests use an in-memory SQLite database with migrations applied,
// verifying repository functions against a real (ephemeral) schema.
// =============================================================================

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod test_categories;
mod test_chains;
mod test_collections;
mod test_migrations;
mod test_mixed_chains;
mod test_prompts;
mod test_schema_constraints;
mod test_script_versions;
mod test_scripts;
mod test_search;
mod test_settings;
mod test_tags;
mod test_users;
mod test_versions;

use crate::Database;

/// Creates a fresh in-memory database with all migrations applied.
/// Each test calling this function receives an isolated database instance.
fn setup_db() -> Database {
    Database::open_in_memory().expect("in-memory database creation must succeed")
}
