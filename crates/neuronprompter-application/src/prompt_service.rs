// =============================================================================
// Prompt service: create, update, delete, duplicate, toggle operations.
//
// Coordinates prompt lifecycle operations that span validation, version
// snapshots, field updates, and association synchronization. All multi-step
// mutations run within a database transaction to maintain consistency.
// =============================================================================

use std::collections::HashSet;

use neuronprompter_core::CoreError;
use neuronprompter_core::domain::PaginatedList;
use neuronprompter_core::domain::prompt::{
    NewPrompt, Prompt, PromptFilter, PromptWithAssociations, UpdatePrompt,
};
use neuronprompter_core::validation;
use neuronprompter_db::ConnectionProvider;
use neuronprompter_db::repo::{categories, chains, collections, prompts, tags};

use crate::ServiceError;
use crate::association_sync::{sync_associations, verify_taxonomy_ownership};
use crate::version_service;

/// Creates a prompt with validated fields and initial associations.
/// Runs within a transaction so that the prompt row and all junction-table
/// links are created atomically.
///
/// # Errors
///
/// Returns `ServiceError::Core(Validation)` if the title, content, content size,
/// or language code fails validation.
/// Returns `ServiceError::Database` if taxonomy ownership verification fails or
/// the persistence layer fails.
pub fn create_prompt(
    cp: &impl ConnectionProvider,
    new: &NewPrompt,
) -> Result<Prompt, ServiceError> {
    validation::validate_title(&new.title)?;
    validation::validate_content(&new.content)?;
    validation::validate_content_size(&new.content, validation::MAX_CONTENT_BYTES)?;
    validation::validate_language_code(new.language.as_deref())?;

    Ok(cp.with_transaction(|conn| {
        verify_taxonomy_ownership(
            conn,
            new.user_id,
            &new.tag_ids,
            &new.category_ids,
            &new.collection_ids,
        )?;
        prompts::create_prompt(conn, new)
    })?)
}

/// Updates a prompt's fields and synchronizes its associations.
///
/// The update workflow:
/// 1. Load the current prompt state.
/// 2. Determine if any fields differ from the current values.
/// 3. If changes exist: snapshot the pre-edit state as a version, apply
///    field updates, and synchronize associations.
/// 4. If no changes: return the current prompt without creating a version.
///
/// # Errors
///
/// Returns `ServiceError::Core(Validation)` if any supplied title, content,
/// content size, or language code fails validation.
/// Returns `ServiceError::Database` if the prompt does not exist, taxonomy
/// ownership verification fails, or the persistence layer fails.
pub fn update_prompt(
    cp: &impl ConnectionProvider,
    update: &UpdatePrompt,
) -> Result<Prompt, ServiceError> {
    // Validate supplied fields before entering the transaction.
    if let Some(ref title) = update.title {
        validation::validate_title(title)?;
    }
    if let Some(ref content) = update.content {
        validation::validate_content(content)?;
        validation::validate_content_size(content, validation::MAX_CONTENT_BYTES)?;
    }
    if let Some(ref language) = update.language {
        validation::validate_language_code(language.as_deref())?;
    }

    Ok(cp.with_transaction(|conn| {
        let current = prompts::get_prompt(conn, update.prompt_id)?;

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
            version_service::create_version_snapshot(conn, &current)?;

            // Apply field updates. Convert `UpdatePrompt`'s `Option<T>` fields
            // into the parameter format expected by the repo function.
            prompts::update_prompt_fields(
                conn,
                update.prompt_id,
                update.title.as_deref(),
                update.content.as_deref(),
                update.description.as_ref().map(|opt| opt.as_deref()),
                update.notes.as_ref().map(|opt| opt.as_deref()),
                update.language.as_ref().map(|opt| opt.as_deref()),
                update.expected_version,
            )?;
        }

        // Synchronize tag associations if a new set was provided.
        if let Some(ref desired_tag_ids) = update.tag_ids {
            let current_tags = tags::get_tags_for_prompt(conn, update.prompt_id)?;
            let current_ids: HashSet<i64> = current_tags.iter().map(|t| t.id).collect();
            let desired: HashSet<i64> = desired_tag_ids.iter().copied().collect();
            sync_associations(
                conn,
                update.prompt_id,
                &current_ids,
                &desired,
                tags::unlink_prompt_tag,
                tags::link_prompt_tag,
            )?;
        }

        // Synchronize category associations if a new set was provided.
        if let Some(ref desired_cat_ids) = update.category_ids {
            let current_cats = categories::get_categories_for_prompt(conn, update.prompt_id)?;
            let current_ids: HashSet<i64> = current_cats.iter().map(|c| c.id).collect();
            let desired: HashSet<i64> = desired_cat_ids.iter().copied().collect();
            sync_associations(
                conn,
                update.prompt_id,
                &current_ids,
                &desired,
                categories::unlink_prompt_category,
                categories::link_prompt_category,
            )?;
        }

        // Synchronize collection associations if a new set was provided.
        if let Some(ref desired_col_ids) = update.collection_ids {
            let current_cols = collections::get_collections_for_prompt(conn, update.prompt_id)?;
            let current_ids: HashSet<i64> = current_cols.iter().map(|c| c.id).collect();
            let desired: HashSet<i64> = desired_col_ids.iter().copied().collect();
            sync_associations(
                conn,
                update.prompt_id,
                &current_ids,
                &desired,
                collections::unlink_prompt_collection,
                collections::link_prompt_collection,
            )?;
        }

        prompts::get_prompt(conn, update.prompt_id)
    })?)
}

/// Retrieves a prompt with all its resolved associations (tags, categories,
/// collections).
///
/// # Errors
///
/// Returns `ServiceError::Database` if no prompt with the given ID exists or
/// the persistence layer fails.
pub fn get_prompt(
    cp: &impl ConnectionProvider,
    prompt_id: i64,
) -> Result<PromptWithAssociations, ServiceError> {
    Ok(cp.with_connection(|conn| prompts::get_prompt_with_associations(conn, prompt_id))?)
}

/// Returns the total number of prompts owned by the given user.
///
/// # Errors
///
/// Returns `ServiceError::Database` if the persistence layer fails.
pub fn count_prompts(cp: &impl ConnectionProvider, user_id: i64) -> Result<i64, ServiceError> {
    Ok(cp.with_connection(|conn| prompts::count_prompts(conn, user_id))?)
}

/// Returns prompts matching the given filter, ordered by `updated_at` descending,
/// wrapped in pagination metadata.
///
/// # Errors
///
/// Returns `ServiceError::Database` if the persistence layer fails.
pub fn list_prompts(
    cp: &impl ConnectionProvider,
    filter: &PromptFilter,
) -> Result<PaginatedList<Prompt>, ServiceError> {
    Ok(cp.with_connection(|conn| {
        // Performance note: The item list and total count are fetched as two separate queries
        // with identical filter parameters. Using a COUNT(*) OVER() window function would
        // combine both into a single query. SQLite supports window functions since version
        // 3.25.0. This is tracked as a future optimization.
        let items = prompts::list_prompts(conn, filter)?;
        let total = prompts::count_filtered_prompts(conn, filter)?;
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

/// Deletes a prompt and all associated data via CASCADE rules.
/// Returns `EntityInUse` error if the prompt is referenced by any chains.
/// The referencing check and delete run inside a transaction to prevent TOCTOU races.
///
/// # Errors
///
/// Returns `ServiceError::Core(EntityInUse)` if the prompt is referenced by
/// one or more chains.
/// Returns `ServiceError::Database` if no prompt with the given ID exists or
/// the persistence layer fails.
pub fn delete_prompt(cp: &impl ConnectionProvider, prompt_id: i64) -> Result<(), ServiceError> {
    cp.with_transaction(|conn| {
        let referencing = chains::get_chains_containing_prompt(conn, prompt_id)?;
        if !referencing.is_empty() {
            let titles: Vec<String> = referencing.iter().map(|c| c.title.clone()).collect();
            return Err(CoreError::EntityInUse {
                entity_type: "Prompt".to_owned(),
                entity_id: prompt_id,
                referencing_titles: titles,
            }
            .into());
        }
        prompts::delete_prompt(conn, prompt_id)
    })?;
    Ok(())
}

/// Creates a copy of an existing prompt with "(copy)" appended to the title,
/// version reset to 1, and all associations duplicated.
///
/// # Errors
///
/// Returns `ServiceError::Database` if no prompt with the given ID exists or
/// the persistence layer fails.
pub fn duplicate_prompt(
    cp: &impl ConnectionProvider,
    prompt_id: i64,
) -> Result<Prompt, ServiceError> {
    Ok(cp.with_transaction(|conn| prompts::duplicate_prompt(conn, prompt_id))?)
}

/// Toggles the favorite flag on a prompt.
///
/// # Errors
///
/// Returns `ServiceError::Database` if no prompt with the given ID exists or
/// the persistence layer fails.
pub fn toggle_favorite(
    cp: &impl ConnectionProvider,
    prompt_id: i64,
    is_favorite: bool,
) -> Result<(), ServiceError> {
    Ok(cp.with_connection(|conn| prompts::set_favorite(conn, prompt_id, is_favorite))?)
}

/// Returns the distinct non-null language codes used by a user's prompts.
///
/// # Errors
///
/// Returns `ServiceError::Database` if the persistence layer fails.
pub fn list_prompt_languages(
    cp: &impl ConnectionProvider,
    user_id: i64,
) -> Result<Vec<String>, ServiceError> {
    Ok(cp.with_connection(|conn| prompts::list_prompt_languages(conn, user_id))?)
}

/// Toggles the archived flag on a prompt.
///
/// # Errors
///
/// Returns `ServiceError::Database` if no prompt with the given ID exists or
/// the persistence layer fails.
pub fn toggle_archive(
    cp: &impl ConnectionProvider,
    prompt_id: i64,
    is_archived: bool,
) -> Result<(), ServiceError> {
    Ok(cp.with_connection(|conn| prompts::set_archived(conn, prompt_id, is_archived))?)
}

/// Applies bulk operations (favorite, archive, tag/category/collection changes)
/// to multiple prompts in a single transaction. Validates ownership and taxonomy
/// membership before applying any changes.
///
/// # Errors
///
/// Returns `ServiceError::Core(Validation)` if no operations are specified.
/// Returns `ServiceError::Core(NotFound)` if a prompt does not exist or belongs
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
        // Performance note: Taxonomy link/unlink operations are executed per-item in a loop.
        // For N items with M taxonomy changes each, this produces N*M individual SQL
        // statements. A batch SQL approach using INSERT INTO ... SELECT or multi-value
        // INSERT would reduce this to M statements total. This is tracked as a future
        // optimization.
        for &id in &input.ids {
            let owner_id = prompts::get_prompt_owner(conn, id)?;
            if owner_id != user_id {
                return Err(neuronprompter_db::DbError::Core(CoreError::NotFound {
                    entity: "Prompt".to_owned(),
                    id,
                }));
            }

            if let Some(fav) = input.set_favorite {
                prompts::set_favorite(conn, id, fav)?;
            }
            if let Some(arch) = input.set_archived {
                prompts::set_archived(conn, id, arch)?;
            }
            for &tag_id in &input.add_tag_ids {
                tags::link_prompt_tag(conn, id, tag_id)?;
            }
            for &tag_id in &input.remove_tag_ids {
                tags::unlink_prompt_tag(conn, id, tag_id)?;
            }
            for &cat_id in &input.add_category_ids {
                categories::link_prompt_category(conn, id, cat_id)?;
            }
            for &cat_id in &input.remove_category_ids {
                categories::unlink_prompt_category(conn, id, cat_id)?;
            }
            for &col_id in &input.add_collection_ids {
                collections::link_prompt_collection(conn, id, col_id)?;
            }
            for &col_id in &input.remove_collection_ids {
                collections::unlink_prompt_collection(conn, id, col_id)?;
            }
            updated += 1;
        }
        Ok(updated)
    })?)
}

/// Compares the `UpdatePrompt` fields against the current `Prompt` state to
/// detect whether any content or metadata fields have changed.
fn has_content_changes(current: &Prompt, update: &UpdatePrompt) -> bool {
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
    if let Some(ref language) = update.language
        && *language != current.language
    {
        return true;
    }
    false
}
