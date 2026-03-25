// =============================================================================
// Script service: create, update, delete, duplicate, toggle operations.
//
// Coordinates script lifecycle operations that span validation, version
// snapshots, field updates, and association synchronization. All multi-step
// mutations run within a database transaction to maintain consistency.
//
// All public functions accept `&impl ConnectionProvider` so they work
// transparently with both a single-connection `Database` and an r2d2 `DbPool`.
// =============================================================================

use std::collections::HashSet;

use neuronprompter_core::CoreError;
use neuronprompter_core::domain::PaginatedList;
use neuronprompter_core::domain::script::{
    NewScript, Script, ScriptFilter, ScriptWithAssociations, UpdateScript,
};
use neuronprompter_core::validation;
use neuronprompter_db::ConnectionProvider;
use neuronprompter_db::repo::{chains, scripts};

use crate::ServiceError;
use crate::association_sync::{sync_associations, verify_taxonomy_ownership};
use crate::script_version_service;

/// Creates a script with validated fields and initial associations.
/// Runs within a transaction so that the script row and all junction-table
/// links are created atomically.
///
/// # Errors
///
/// Returns `ServiceError::Core(Validation)` if the title, content, content size,
/// script language, or language code fails validation.
/// Returns `ServiceError::Database` if taxonomy ownership verification fails or
/// the persistence layer fails.
pub fn create_script(
    cp: &impl ConnectionProvider,
    new: &NewScript,
) -> Result<Script, ServiceError> {
    validation::validate_title(&new.title)?;
    validation::validate_content(&new.content)?;
    validation::validate_content_size(&new.content, validation::MAX_CONTENT_BYTES)?;
    validation::validate_script_language(&new.script_language)?;
    validation::validate_language_code(new.language.as_deref())?;

    Ok(cp.with_transaction(|conn| {
        verify_taxonomy_ownership(
            conn,
            new.user_id,
            &new.tag_ids,
            &new.category_ids,
            &new.collection_ids,
        )?;
        scripts::create_script(conn, new)
    })?)
}

/// Updates a script's fields and synchronizes its associations.
///
/// The update workflow:
/// 1. Load the current script state.
/// 2. Determine if any fields differ from the current values.
/// 3. If changes exist: snapshot the pre-edit state as a version, apply
///    field updates, and synchronize associations.
/// 4. If no changes: return the current script without creating a version.
///
/// # Errors
///
/// Returns `ServiceError::Core(Validation)` if any supplied title, content,
/// content size, script language, or language code fails validation.
/// Returns `ServiceError::Database` if the script does not exist, taxonomy
/// ownership verification fails, or the persistence layer fails.
pub fn update_script(
    cp: &impl ConnectionProvider,
    update: &UpdateScript,
) -> Result<Script, ServiceError> {
    // Validate supplied fields before entering the transaction.
    if let Some(ref title) = update.title {
        validation::validate_title(title)?;
    }
    if let Some(ref content) = update.content {
        validation::validate_content(content)?;
        validation::validate_content_size(content, validation::MAX_CONTENT_BYTES)?;
    }
    if let Some(ref sl) = update.script_language {
        validation::validate_script_language(sl)?;
    }
    if let Some(ref language) = update.language {
        validation::validate_language_code(language.as_deref())?;
    }

    Ok(cp.with_transaction(|conn| {
        let current = scripts::get_script(conn, update.script_id)?;

        // Verify taxonomy ownership before syncing associations.
        verify_taxonomy_ownership(
            conn,
            current.user_id,
            update.tag_ids.as_deref().unwrap_or_default(),
            update.category_ids.as_deref().unwrap_or_default(),
            update.collection_ids.as_deref().unwrap_or_default(),
        )?;

        // Detect whether any field values differ from current state.
        let has_field_changes = has_content_changes(&current, update);

        if has_field_changes {
            // Snapshot the pre-edit state.
            script_version_service::create_version_snapshot(conn, &current)?;

            // Apply field updates. Convert `UpdateScript`'s `Option<T>` fields
            // into the parameter format expected by the repo function.
            scripts::update_script_fields(
                conn,
                update.script_id,
                update.title.as_deref(),
                update.content.as_deref(),
                update.description.as_ref().map(|opt| opt.as_deref()),
                update.notes.as_ref().map(|opt| opt.as_deref()),
                update.script_language.as_deref(),
                update.language.as_ref().map(|opt| opt.as_deref()),
                update.source_path.as_ref().map(|opt| opt.as_deref()),
                update.is_synced,
                update.expected_version,
            )?;
        }

        // Synchronize tag associations if a new set was provided.
        if let Some(ref desired_tag_ids) = update.tag_ids {
            let current_tags = scripts::get_tags_for_script(conn, update.script_id)?;
            let current_ids: HashSet<i64> = current_tags.iter().map(|t| t.id).collect();
            let desired: HashSet<i64> = desired_tag_ids.iter().copied().collect();
            sync_associations(
                conn,
                update.script_id,
                &current_ids,
                &desired,
                scripts::unlink_script_tag,
                scripts::link_script_tag,
            )?;
        }

        // Synchronize category associations if a new set was provided.
        if let Some(ref desired_cat_ids) = update.category_ids {
            let current_cats = scripts::get_categories_for_script(conn, update.script_id)?;
            let current_ids: HashSet<i64> = current_cats.iter().map(|c| c.id).collect();
            let desired: HashSet<i64> = desired_cat_ids.iter().copied().collect();
            sync_associations(
                conn,
                update.script_id,
                &current_ids,
                &desired,
                scripts::unlink_script_category,
                scripts::link_script_category,
            )?;
        }

        // Synchronize collection associations if a new set was provided.
        if let Some(ref desired_col_ids) = update.collection_ids {
            let current_cols = scripts::get_collections_for_script(conn, update.script_id)?;
            let current_ids: HashSet<i64> = current_cols.iter().map(|c| c.id).collect();
            let desired: HashSet<i64> = desired_col_ids.iter().copied().collect();
            sync_associations(
                conn,
                update.script_id,
                &current_ids,
                &desired,
                scripts::unlink_script_collection,
                scripts::link_script_collection,
            )?;
        }

        scripts::get_script(conn, update.script_id)
    })?)
}

/// Retrieves a script with all its resolved associations (tags, categories,
/// collections).
///
/// # Errors
///
/// Returns `ServiceError::Database` if no script with the given ID exists or
/// the persistence layer fails.
pub fn get_script(
    cp: &impl ConnectionProvider,
    script_id: i64,
) -> Result<ScriptWithAssociations, ServiceError> {
    Ok(cp.with_connection(|conn| scripts::get_script_with_associations(conn, script_id))?)
}

/// Returns the total number of scripts owned by the given user.
///
/// # Errors
///
/// Returns `ServiceError::Database` if the persistence layer fails.
pub fn count_scripts(cp: &impl ConnectionProvider, user_id: i64) -> Result<i64, ServiceError> {
    Ok(cp.with_connection(|conn| scripts::count_scripts(conn, user_id))?)
}

/// Returns scripts matching the given filter, ordered by `updated_at` descending,
/// wrapped in pagination metadata.
///
/// # Errors
///
/// Returns `ServiceError::Database` if the persistence layer fails.
pub fn list_scripts(
    cp: &impl ConnectionProvider,
    filter: &ScriptFilter,
) -> Result<PaginatedList<Script>, ServiceError> {
    Ok(cp.with_connection(|conn| {
        let items = scripts::list_scripts(conn, filter)?;
        let total = scripts::count_filtered_scripts(conn, filter)?;
        let offset = filter.offset.unwrap_or(0);
        #[allow(clippy::cast_possible_wrap)]
        let count = items.len() as i64;
        let has_more = count > 0 && offset + count < total;
        Ok(PaginatedList {
            items,
            total,
            has_more,
        })
    })?)
}

/// Deletes a script and all associated data via CASCADE rules.
/// Returns `EntityInUse` error if the script is referenced by any chains.
/// The check and delete are wrapped in a transaction to prevent TOCTOU races.
///
/// # Errors
///
/// Returns `ServiceError::Core(EntityInUse)` if the script is referenced by
/// one or more chains.
/// Returns `ServiceError::Database` if no script with the given ID exists or
/// the persistence layer fails.
pub fn delete_script(cp: &impl ConnectionProvider, script_id: i64) -> Result<(), ServiceError> {
    cp.with_transaction(|conn| {
        let referencing = chains::get_chains_containing_script(conn, script_id)?;
        if !referencing.is_empty() {
            let titles: Vec<String> = referencing.iter().map(|c| c.title.clone()).collect();
            return Err(neuronprompter_db::DbError::Core(CoreError::EntityInUse {
                entity_type: "Script".to_owned(),
                entity_id: script_id,
                referencing_titles: titles,
            }));
        }
        scripts::delete_script(conn, script_id)
    })?;
    Ok(())
}

/// Creates a copy of an existing script with "(copy)" appended to the title,
/// version reset to 1, and all associations duplicated.
///
/// # Errors
///
/// Returns `ServiceError::Database` if no script with the given ID exists or
/// the persistence layer fails.
pub fn duplicate_script(
    cp: &impl ConnectionProvider,
    script_id: i64,
) -> Result<Script, ServiceError> {
    Ok(cp.with_transaction(|conn| scripts::duplicate_script(conn, script_id))?)
}

/// Toggles the favorite flag on a script.
///
/// # Errors
///
/// Returns `ServiceError::Database` if no script with the given ID exists or
/// the persistence layer fails.
pub fn toggle_favorite(
    cp: &impl ConnectionProvider,
    script_id: i64,
    is_favorite: bool,
) -> Result<(), ServiceError> {
    Ok(cp.with_connection(|conn| scripts::set_favorite(conn, script_id, is_favorite))?)
}

/// Returns the distinct non-null script_language codes used by a user's scripts.
///
/// # Errors
///
/// Returns `ServiceError::Database` if the persistence layer fails.
pub fn list_script_languages(
    cp: &impl ConnectionProvider,
    user_id: i64,
) -> Result<Vec<String>, ServiceError> {
    Ok(cp.with_connection(|conn| scripts::list_script_languages(conn, user_id))?)
}

/// Toggles the archived flag on a script.
///
/// # Errors
///
/// Returns `ServiceError::Database` if no script with the given ID exists or
/// the persistence layer fails.
pub fn toggle_archive(
    cp: &impl ConnectionProvider,
    script_id: i64,
    is_archived: bool,
) -> Result<(), ServiceError> {
    Ok(cp.with_connection(|conn| scripts::set_archived(conn, script_id, is_archived))?)
}

/// Applies bulk operations (favorite, archive, tag/category/collection changes)
/// to multiple scripts in a single transaction. Validates ownership and taxonomy
/// membership before applying any changes.
///
/// # Errors
///
/// Returns `ServiceError::Core(Validation)` if no operations are specified.
/// Returns `ServiceError::Core(NotFound)` if a script does not exist or belongs
/// to a different user.
/// Returns `ServiceError::Database` if taxonomy ownership verification fails or
/// the persistence layer fails.
pub fn bulk_update(
    cp: &impl ConnectionProvider,
    user_id: i64,
    input: &crate::association_sync::BulkUpdateInput,
) -> Result<usize, ServiceError> {
    if !input.has_operations() {
        return Err(ServiceError::Core(CoreError::Validation {
            field: "operations".to_owned(),
            message: "At least one operation must be specified".to_owned(),
        }));
    }

    Ok(cp.with_transaction(|conn| {
        // Verify taxonomy ownership for IDs being added to the batch.
        crate::association_sync::verify_taxonomy_ownership(
            conn,
            user_id,
            &input.add_tag_ids,
            &input.add_category_ids,
            &input.add_collection_ids,
        )?;

        // Verify taxonomy ownership for IDs being removed from the batch.
        crate::association_sync::verify_taxonomy_ownership(
            conn,
            user_id,
            &input.remove_tag_ids,
            &input.remove_category_ids,
            &input.remove_collection_ids,
        )?;

        let mut updated = 0usize;
        for &id in &input.ids {
            let owner_id = scripts::get_script_owner(conn, id)?;
            if owner_id != user_id {
                return Err(neuronprompter_db::DbError::Core(CoreError::NotFound {
                    entity: "Script".to_owned(),
                    id,
                }));
            }

            if let Some(fav) = input.set_favorite {
                scripts::set_favorite(conn, id, fav)?;
            }
            if let Some(arch) = input.set_archived {
                scripts::set_archived(conn, id, arch)?;
            }
            for &tag_id in &input.add_tag_ids {
                scripts::link_script_tag(conn, id, tag_id)?;
            }
            for &tag_id in &input.remove_tag_ids {
                scripts::unlink_script_tag(conn, id, tag_id)?;
            }
            for &cat_id in &input.add_category_ids {
                scripts::link_script_category(conn, id, cat_id)?;
            }
            for &cat_id in &input.remove_category_ids {
                scripts::unlink_script_category(conn, id, cat_id)?;
            }
            for &col_id in &input.add_collection_ids {
                scripts::link_script_collection(conn, id, col_id)?;
            }
            for &col_id in &input.remove_collection_ids {
                scripts::unlink_script_collection(conn, id, col_id)?;
            }
            updated += 1;
        }
        Ok(updated)
    })?)
}

/// Compares the `UpdateScript` fields against the current `Script` state to
/// detect whether any content or metadata fields have changed.
fn has_content_changes(current: &Script, update: &UpdateScript) -> bool {
    if let Some(ref title) = update.title
        && *title != current.title
    {
        return true;
    }
    if let Some(ref content) = update.content
        && *content != current.content
    {
        return true;
    }
    if let Some(ref description) = update.description
        && *description != current.description
    {
        return true;
    }
    if let Some(ref notes) = update.notes
        && *notes != current.notes
    {
        return true;
    }
    if let Some(ref script_language) = update.script_language
        && *script_language != current.script_language
    {
        return true;
    }
    if let Some(ref language) = update.language
        && *language != current.language
    {
        return true;
    }
    if let Some(ref source_path) = update.source_path
        && *source_path != current.source_path
    {
        return true;
    }
    if let Some(is_synced) = update.is_synced
        && is_synced != current.is_synced
    {
        return true;
    }
    false
}
