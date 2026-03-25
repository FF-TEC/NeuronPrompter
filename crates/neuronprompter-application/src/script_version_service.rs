// =============================================================================
// Script version service: snapshot creation, history listing, and version restore.
//
// Manages the lifecycle of script version snapshots. Before each script update,
// the service captures the pre-edit state as a version row. The restore
// operation overwrites the current script with a historical version's data
// while preserving the ability to undo the restore (by snapshotting first).
// =============================================================================

use neuronprompter_core::domain::script::Script;
use neuronprompter_core::domain::script_version::ScriptVersion;
use neuronprompter_db::ConnectionProvider;
use neuronprompter_db::DbError;
use neuronprompter_db::repo::{script_versions, scripts};

use crate::ServiceError;

/// Captures the current state of a script as an immutable version snapshot.
/// Called by the script service before applying field updates.
///
/// The snapshot records the script's content and metadata at the specified
/// `version_number`, which corresponds to the script's `current_version`
/// before the update increments it.
///
/// # Errors
///
/// Returns `DbError` if the version insertion fails in the persistence layer.
pub fn create_version_snapshot(
    conn: &rusqlite::Connection,
    script: &Script,
) -> Result<ScriptVersion, DbError> {
    script_versions::insert_version(
        conn,
        script.id,
        script.current_version,
        &script.title,
        &script.content,
        script.description.as_deref(),
        script.notes.as_deref(),
        &script.script_language,
        script.language.as_deref(),
    )
}

/// Returns all version snapshots for a script, ordered by version number
/// ascending.
///
/// # Errors
///
/// Returns `ServiceError::Database` if the persistence layer fails.
pub fn list_versions(
    cp: &impl ConnectionProvider,
    script_id: i64,
) -> Result<Vec<ScriptVersion>, ServiceError> {
    Ok(cp.with_connection(|conn| script_versions::list_versions_for_script(conn, script_id))?)
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
) -> Result<ScriptVersion, ServiceError> {
    Ok(cp.with_connection(|conn| script_versions::get_version_by_id(conn, version_id))?)
}

/// Restores a script to a historical version's content and metadata.
///
/// Before overwriting, the current state is saved as a version snapshot
/// so the restore operation itself is reversible. The script's content fields
/// are replaced with the historical version's values and the version counter
/// is incremented.
///
/// If a snapshot with the current version number already exists (e.g. from a
/// previous restore attempt), the duplicate snapshot creation is skipped to
/// prevent duplicate version rows.
///
/// # Errors
///
/// Returns `ServiceError::Database` if the script does not exist, the
/// requested version number does not exist, or the persistence layer fails.
pub fn restore_version(
    cp: &impl ConnectionProvider,
    script_id: i64,
    version_number: i64,
) -> Result<Script, ServiceError> {
    Ok(cp.with_transaction(|conn| {
        // Load the current script state.
        let current = scripts::get_script(conn, script_id)?;

        // Check whether a snapshot with the current version number already exists.
        // If it does, skip snapshot creation to avoid duplicate version rows.
        let existing_snapshot =
            script_versions::get_version_by_number(conn, script_id, current.current_version);
        if existing_snapshot.is_err() {
            // No snapshot exists for this version number, so create one.
            create_version_snapshot(conn, &current)?;
        }

        // Load the historical version to restore from.
        let historical = script_versions::get_version_by_number(conn, script_id, version_number)?;

        // Overwrite the script's content fields with the historical version.
        scripts::update_script_fields(
            conn,
            script_id,
            Some(&historical.title),
            Some(&historical.content),
            Some(historical.description.as_deref()),
            Some(historical.notes.as_deref()),
            Some(&historical.script_language),
            Some(historical.language.as_deref()),
            None,
            None,
            None,
        )?;

        scripts::get_script(conn, script_id)
    })?)
}
