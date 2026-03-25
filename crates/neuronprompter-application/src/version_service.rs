// =============================================================================
// Version service: snapshot creation, history listing, and version restore.
//
// Manages the lifecycle of prompt version snapshots. Before each prompt update,
// the service captures the pre-edit state as a version row. The restore
// operation overwrites the current prompt with a historical version's data
// while preserving the ability to undo the restore (by snapshotting first).
// =============================================================================

use neuronprompter_core::domain::prompt::Prompt;
use neuronprompter_core::domain::version::PromptVersion;
use neuronprompter_db::ConnectionProvider;
use neuronprompter_db::DbError;
use neuronprompter_db::repo::{prompts, versions};

use crate::ServiceError;

/// Captures the current state of a prompt as an immutable version snapshot.
/// Called by the prompt service before applying field updates.
///
/// The snapshot records the prompt's content and metadata at the specified
/// `version_number`, which corresponds to the prompt's `current_version`
/// before the update increments it.
///
/// # Errors
///
/// Returns `DbError` if the version insertion fails in the persistence layer.
pub fn create_version_snapshot(
    conn: &rusqlite::Connection,
    prompt: &Prompt,
) -> Result<PromptVersion, DbError> {
    versions::insert_version(
        conn,
        prompt.id,
        prompt.current_version,
        &prompt.title,
        &prompt.content,
        prompt.description.as_deref(),
        prompt.notes.as_deref(),
        prompt.language.as_deref(),
    )
}

/// Returns all version snapshots for a prompt, ordered by version number
/// ascending.
///
/// # Errors
///
/// Returns `ServiceError::Database` if the persistence layer fails.
pub fn list_versions(
    cp: &impl ConnectionProvider,
    prompt_id: i64,
) -> Result<Vec<PromptVersion>, ServiceError> {
    Ok(cp.with_connection(|conn| versions::list_versions_for_prompt(conn, prompt_id))?)
}

/// Retrieves a single version snapshot by its database ID.
///
/// # Errors
///
/// Returns `ServiceError::Database` if no version with the given ID exists or
/// the persistence layer fails.
pub fn get_version(
    cp: &impl ConnectionProvider,
    version_id: i64,
) -> Result<PromptVersion, ServiceError> {
    Ok(cp.with_connection(|conn| versions::get_version_by_id(conn, version_id))?)
}

/// Restores a prompt to a historical version's content and metadata.
///
/// Before overwriting, the current state is saved as a version snapshot
/// so the restore operation itself is reversible. The prompt's content fields
/// are replaced with the historical version's values and the version counter
/// is incremented.
///
/// If a snapshot with the current version number already exists (e.g. from a
/// previous restore attempt), the duplicate snapshot creation is skipped to
/// prevent duplicate version rows.
///
/// # Errors
///
/// Returns `ServiceError::Database` if the prompt does not exist, the
/// requested version number does not exist, or the persistence layer fails.
pub fn restore_version(
    cp: &impl ConnectionProvider,
    prompt_id: i64,
    version_number: i64,
) -> Result<Prompt, ServiceError> {
    Ok(cp.with_transaction(|conn| {
        // Load the current prompt state.
        let current = prompts::get_prompt(conn, prompt_id)?;

        // Check whether a snapshot with the current version number already exists.
        // If it does, skip snapshot creation to avoid duplicate version rows.
        let existing_snapshot =
            versions::get_version_by_number(conn, prompt_id, current.current_version);
        if existing_snapshot.is_err() {
            // No snapshot exists for this version number, so create one.
            create_version_snapshot(conn, &current)?;
        }

        // Load the historical version to restore from.
        let historical = versions::get_version_by_number(conn, prompt_id, version_number)?;

        // Overwrite the prompt's content fields with the historical version.
        prompts::update_prompt_fields(
            conn,
            prompt_id,
            Some(&historical.title),
            Some(&historical.content),
            Some(historical.description.as_deref()),
            Some(historical.notes.as_deref()),
            Some(historical.language.as_deref()),
            None, // No optimistic concurrency for version restore.
        )?;

        prompts::get_prompt(conn, prompt_id)
    })?)
}
