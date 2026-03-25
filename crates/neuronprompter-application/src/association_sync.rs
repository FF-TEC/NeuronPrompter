// =============================================================================
// Association synchronization: set-difference-based junction table updates.
//
// Computes the difference between the current set of linked entity IDs and
// the desired set, then calls the appropriate unlink/link functions to bring
// the junction table into the desired state. Used by the prompt service to
// synchronize tags, categories, and collections in a single pass.
// =============================================================================

use std::collections::HashSet;

use neuronprompter_core::CoreError;
use neuronprompter_db::DbError;
use neuronprompter_db::repo::{categories, collections, tags};

/// Synchronizes a many-to-many relationship between a prompt and a set of
/// associated entities (tags, categories, or collections).
///
/// Computes the set difference between `current_ids` (existing links) and
/// `desired_ids` (target state), then removes links for IDs in
/// (current - desired) and adds links for IDs in (desired - current).
///
/// # Errors
///
/// Returns `DbError` if any unlink or link database operation fails.
#[allow(clippy::implicit_hasher)]
pub fn sync_associations<F, G>(
    conn: &rusqlite::Connection,
    entity_id: i64,
    current_ids: &HashSet<i64>,
    desired_ids: &HashSet<i64>,
    unlink_fn: F,
    link_fn: G,
) -> Result<(), DbError>
where
    F: Fn(&rusqlite::Connection, i64, i64) -> Result<(), DbError>,
    G: Fn(&rusqlite::Connection, i64, i64) -> Result<(), DbError>,
{
    // Remove links that are no longer desired.
    for &id in current_ids.difference(desired_ids) {
        unlink_fn(conn, entity_id, id)?;
    }
    // Add links that are newly desired.
    for &id in desired_ids.difference(current_ids) {
        link_fn(conn, entity_id, id)?;
    }
    Ok(())
}

/// Verifies that all referenced tag, category, and collection IDs exist and
/// belong to the specified user. Returns `NotFound` if an ID does not exist,
/// or `Validation` if an entity belongs to a different user.
///
/// Performance note: Each taxonomy ID is verified individually with a SELECT query. For bulk
/// operations with many taxonomy IDs, a batch approach using SELECT id, user_id FROM tags
/// WHERE id IN (...) would be more efficient. This is tracked as a future optimization.
///
/// This prevents cross-user taxonomy assignment and also catches
/// non-existent foreign keys early.
///
/// # Errors
///
/// Returns `DbError::Core(CoreError::NotFound)` if a tag, category, or
/// collection ID does not exist.
/// Returns `DbError::Core(CoreError::Validation)` if an entity belongs to a
/// different user than `user_id`.
pub fn verify_taxonomy_ownership(
    conn: &rusqlite::Connection,
    user_id: i64,
    tag_ids: &[i64],
    category_ids: &[i64],
    collection_ids: &[i64],
) -> Result<(), DbError> {
    for &id in tag_ids {
        let tag = tags::get_tag(conn, id)?;
        if tag.user_id != user_id {
            return Err(DbError::Core(CoreError::Validation {
                field: "tag_ids".to_owned(),
                message: format!("Tag {id} belongs to another user"),
            }));
        }
    }
    for &id in category_ids {
        let cat = categories::get_category(conn, id)?;
        if cat.user_id != user_id {
            return Err(DbError::Core(CoreError::Validation {
                field: "category_ids".to_owned(),
                message: format!("Category {id} belongs to another user"),
            }));
        }
    }
    for &id in collection_ids {
        let col = collections::get_collection(conn, id)?;
        if col.user_id != user_id {
            return Err(DbError::Core(CoreError::Validation {
                field: "collection_ids".to_owned(),
                message: format!("Collection {id} belongs to another user"),
            }));
        }
    }
    Ok(())
}

/// Maximum number of IDs allowed in a single bulk-update request.
pub const MAX_BULK_IDS: usize = 1000;

/// Input for bulk update operations. Shared across prompts, scripts, chains.
pub struct BulkUpdateInput {
    pub ids: Vec<i64>,
    pub set_favorite: Option<bool>,
    pub set_archived: Option<bool>,
    pub add_tag_ids: Vec<i64>,
    pub remove_tag_ids: Vec<i64>,
    pub add_category_ids: Vec<i64>,
    pub remove_category_ids: Vec<i64>,
    pub add_collection_ids: Vec<i64>,
    pub remove_collection_ids: Vec<i64>,
}

impl BulkUpdateInput {
    /// Returns true if at least one operation field is set.
    pub fn has_operations(&self) -> bool {
        self.set_favorite.is_some()
            || self.set_archived.is_some()
            || !self.add_tag_ids.is_empty()
            || !self.remove_tag_ids.is_empty()
            || !self.add_category_ids.is_empty()
            || !self.remove_category_ids.is_empty()
            || !self.add_collection_ids.is_empty()
            || !self.remove_collection_ids.is_empty()
    }
}
