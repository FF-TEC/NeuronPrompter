// =============================================================================
// Category service: CRUD with per-user uniqueness enforcement.
//
// Thin service layer over the category repository. Category names are unique
// within a user's namespace; duplicate detection is handled at the database
// constraint level and surfaced as `CoreError::Duplicate`.
// =============================================================================

use neuronprompter_core::domain::category::Category;
use neuronprompter_core::validation;
use neuronprompter_db::ConnectionProvider;
use neuronprompter_db::repo::categories;

use crate::ServiceError;

/// Creates a category under the specified user. The (`user_id`, name)
/// uniqueness constraint is enforced by the database.
///
/// # Errors
///
/// Returns `ServiceError::Core(Validation)` if the name fails taxonomy name validation.
/// Returns `ServiceError::Database` if the persistence layer fails (including duplicate name).
pub fn create_category(
    cp: &impl ConnectionProvider,
    user_id: i64,
    name: &str,
) -> Result<Category, ServiceError> {
    let trimmed = validation::validate_taxonomy_name(name)?;
    Ok(cp.with_connection(|conn| categories::create_category(conn, user_id, &trimmed))?)
}

/// Returns all categories owned by a user, ordered alphabetically by name.
///
/// # Errors
///
/// Returns `ServiceError::Database` if the persistence layer fails.
pub fn list_categories(
    cp: &impl ConnectionProvider,
    user_id: i64,
) -> Result<Vec<Category>, ServiceError> {
    Ok(cp.with_connection(|conn| categories::list_categories_for_user(conn, user_id))?)
}

/// Renames an existing category. Fails with `CoreError::NotFound` if the
/// category does not exist, or `CoreError::Duplicate` if the new name
/// conflicts with an existing category under the same user.
///
/// # Errors
///
/// Returns `ServiceError::Core(Validation)` if the new name fails taxonomy name validation.
/// Returns `ServiceError::Database` if the persistence layer fails (including not-found or duplicate).
pub fn rename_category(
    cp: &impl ConnectionProvider,
    category_id: i64,
    new_name: &str,
) -> Result<(), ServiceError> {
    let trimmed = validation::validate_taxonomy_name(new_name)?;
    Ok(cp.with_connection(|conn| categories::rename_category(conn, category_id, &trimmed))?)
}

/// Deletes a category and removes all junction-table links via CASCADE.
///
/// # Errors
///
/// Returns `ServiceError::Database` if the persistence layer fails.
pub fn delete_category(cp: &impl ConnectionProvider, category_id: i64) -> Result<(), ServiceError> {
    Ok(cp.with_connection(|conn| categories::delete_category(conn, category_id))?)
}
