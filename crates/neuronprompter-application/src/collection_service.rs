// =============================================================================
// Collection service: CRUD with per-user uniqueness enforcement.
//
// Thin service layer over the collection repository. Collection names are
// unique within a user's namespace; duplicate detection is handled at the
// database constraint level and surfaced as `CoreError::Duplicate`.
// =============================================================================

use neuronprompter_core::domain::collection::Collection;
use neuronprompter_core::validation;
use neuronprompter_db::ConnectionProvider;
use neuronprompter_db::repo::collections;

use crate::ServiceError;

/// Creates a collection under the specified user. The (`user_id`, name)
/// uniqueness constraint is enforced by the database.
///
/// # Errors
///
/// Returns `ServiceError::Core(Validation)` if the name fails taxonomy name validation.
/// Returns `ServiceError::Database` if the persistence layer fails (including duplicate name).
pub fn create_collection(
    cp: &impl ConnectionProvider,
    user_id: i64,
    name: &str,
) -> Result<Collection, ServiceError> {
    let trimmed = validation::validate_taxonomy_name(name)?;
    Ok(cp.with_connection(|conn| collections::create_collection(conn, user_id, &trimmed))?)
}

/// Returns all collections owned by a user, ordered alphabetically by name.
///
/// # Errors
///
/// Returns `ServiceError::Database` if the persistence layer fails.
pub fn list_collections(
    cp: &impl ConnectionProvider,
    user_id: i64,
) -> Result<Vec<Collection>, ServiceError> {
    Ok(cp.with_connection(|conn| collections::list_collections_for_user(conn, user_id))?)
}

/// Renames an existing collection. Fails with `CoreError::NotFound` if the
/// collection does not exist, or `CoreError::Duplicate` if the new name
/// conflicts with an existing collection under the same user.
///
/// # Errors
///
/// Returns `ServiceError::Core(Validation)` if the new name fails taxonomy name validation.
/// Returns `ServiceError::Database` if the persistence layer fails (including not-found or duplicate).
pub fn rename_collection(
    cp: &impl ConnectionProvider,
    collection_id: i64,
    new_name: &str,
) -> Result<(), ServiceError> {
    let trimmed = validation::validate_taxonomy_name(new_name)?;
    Ok(cp.with_connection(|conn| collections::rename_collection(conn, collection_id, &trimmed))?)
}

/// Deletes a collection and removes all junction-table links via CASCADE.
///
/// # Errors
///
/// Returns `ServiceError::Database` if the persistence layer fails.
pub fn delete_collection(
    cp: &impl ConnectionProvider,
    collection_id: i64,
) -> Result<(), ServiceError> {
    Ok(cp.with_connection(|conn| collections::delete_collection(conn, collection_id))?)
}
