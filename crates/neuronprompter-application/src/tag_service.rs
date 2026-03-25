// =============================================================================
// Tag service: CRUD with per-user uniqueness enforcement.
//
// Thin service layer over the tag repository. Tag names are unique within a
// user's namespace; duplicate detection is handled at the database constraint
// level and surfaced as `CoreError::Duplicate`.
// =============================================================================

use neuronprompter_core::domain::tag::Tag;
use neuronprompter_core::validation;
use neuronprompter_db::ConnectionProvider;
use neuronprompter_db::repo::tags;

use crate::ServiceError;

/// Creates a tag under the specified user. The (`user_id`, name) uniqueness
/// constraint is enforced by the database.
///
/// # Errors
///
/// Returns `ServiceError::Core(Validation)` if the name fails taxonomy name validation.
/// Returns `ServiceError::Database` if the persistence layer fails (including duplicate name).
pub fn create_tag(
    cp: &impl ConnectionProvider,
    user_id: i64,
    name: &str,
) -> Result<Tag, ServiceError> {
    let trimmed = validation::validate_taxonomy_name(name)?;
    Ok(cp.with_connection(|conn| tags::create_tag(conn, user_id, &trimmed))?)
}

/// Returns all tags owned by a user, ordered alphabetically by name.
///
/// # Errors
///
/// Returns `ServiceError::Database` if the persistence layer fails.
pub fn list_tags(cp: &impl ConnectionProvider, user_id: i64) -> Result<Vec<Tag>, ServiceError> {
    Ok(cp.with_connection(|conn| tags::list_tags_for_user(conn, user_id))?)
}

/// Renames an existing tag. Fails with `CoreError::NotFound` if the tag does
/// not exist, or `CoreError::Duplicate` if the new name conflicts with an
/// existing tag under the same user.
///
/// # Errors
///
/// Returns `ServiceError::Core(Validation)` if the new name fails taxonomy name validation.
/// Returns `ServiceError::Database` if the persistence layer fails (including not-found or duplicate).
pub fn rename_tag(
    cp: &impl ConnectionProvider,
    tag_id: i64,
    new_name: &str,
) -> Result<(), ServiceError> {
    let trimmed = validation::validate_taxonomy_name(new_name)?;
    Ok(cp.with_connection(|conn| tags::rename_tag(conn, tag_id, &trimmed))?)
}

/// Deletes a tag and removes all junction-table links via CASCADE.
///
/// # Errors
///
/// Returns `ServiceError::Database` if the persistence layer fails.
pub fn delete_tag(cp: &impl ConnectionProvider, tag_id: i64) -> Result<(), ServiceError> {
    Ok(cp.with_connection(|conn| tags::delete_tag(conn, tag_id))?)
}
