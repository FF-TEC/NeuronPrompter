// =============================================================================
// Chain service: create, update, delete, duplicate, toggle operations.
//
// Coordinates chain lifecycle operations including validation, step management,
// and association synchronization. All multi-step mutations run within a
// database transaction. Chains have no versioning since their content is
// derived from referenced prompts.
// =============================================================================

use std::collections::HashSet;

use neuronprompter_core::CoreError;
use neuronprompter_core::domain::PaginatedList;
use neuronprompter_core::domain::chain::{
    Chain, ChainFilter, ChainWithSteps, NewChain, StepType, UpdateChain,
};
use neuronprompter_core::validation;
use neuronprompter_db::ConnectionProvider;
use neuronprompter_db::SqliteConnection;
use neuronprompter_db::repo::{chains, prompts, scripts};

use crate::ServiceError;
use crate::association_sync::{sync_associations, verify_taxonomy_ownership};

/// Validates that all prompt and script IDs referenced by a new chain exist.
fn verify_step_references(
    conn: &SqliteConnection,
    new: &NewChain,
) -> Result<(), neuronprompter_db::DbError> {
    if new.steps.is_empty() {
        // Legacy prompt_ids path.
        for &pid in &new.prompt_ids {
            prompts::get_prompt(conn, pid)?;
        }
    } else {
        for step in &new.steps {
            match step.step_type {
                StepType::Prompt => {
                    prompts::get_prompt(conn, step.item_id)?;
                }
                StepType::Script => {
                    scripts::get_script(conn, step.item_id)?;
                }
            }
        }
    }
    Ok(())
}

/// Creates a chain with validated fields, ordered steps, and initial associations.
///
/// # Errors
///
/// Returns `ServiceError::Core(Validation)` if the title, language, steps, or
/// separator fails validation.
/// Returns `ServiceError::Database` if a referenced prompt/script does not exist,
/// taxonomy ownership verification fails, or the persistence layer fails.
pub fn create_chain(cp: &impl ConnectionProvider, new: &NewChain) -> Result<Chain, ServiceError> {
    validation::validate_title(&new.title)?;
    validation::validate_language_code(new.language.as_deref())?;
    // Validate steps: prefer `steps` over legacy `prompt_ids`.
    if new.steps.is_empty() {
        validation::validate_chain_steps(&new.prompt_ids)?;
    } else {
        validation::validate_chain_steps_mixed(&new.steps)?;
    }
    if let Some(ref sep) = new.separator {
        validation::validate_separator(sep)?;
    }

    Ok(cp.with_transaction(|conn| {
        verify_step_references(conn, new)?;
        verify_taxonomy_ownership(
            conn,
            new.user_id,
            &new.tag_ids,
            &new.category_ids,
            &new.collection_ids,
        )?;
        chains::create_chain(conn, new)
    })?)
}

/// Updates a chain's metadata, steps, and taxonomy associations.
///
/// # Errors
///
/// Returns `ServiceError::Core(Validation)` if any supplied title, language,
/// separator, or steps field fails validation.
/// Returns `ServiceError::Database` if the chain does not exist, a referenced
/// prompt/script does not exist, taxonomy ownership verification fails, or the
/// persistence layer fails.
pub fn update_chain(
    cp: &impl ConnectionProvider,
    update: &UpdateChain,
) -> Result<Chain, ServiceError> {
    if let Some(ref title) = update.title {
        validation::validate_title(title)?;
    }
    if let Some(ref language) = update.language {
        validation::validate_language_code(language.as_deref())?;
    }
    if let Some(ref sep) = update.separator {
        validation::validate_separator(sep)?;
    }
    if let Some(ref steps) = update.steps {
        validation::validate_chain_steps_mixed(steps)?;
    } else if let Some(ref prompt_ids) = update.prompt_ids {
        validation::validate_chain_steps(prompt_ids)?;
    }

    Ok(cp.with_transaction(|conn| {
        // Verify taxonomy ownership before syncing associations.
        let current = chains::get_chain(conn, update.chain_id)?;
        verify_taxonomy_ownership(
            conn,
            current.user_id,
            update.tag_ids.as_deref().unwrap_or_default(),
            update.category_ids.as_deref().unwrap_or_default(),
            update.collection_ids.as_deref().unwrap_or_default(),
        )?;

        // Apply metadata field updates if any are present.
        let has_field_updates = update.title.is_some()
            || update.description.is_some()
            || update.notes.is_some()
            || update.language.is_some()
            || update.separator.is_some();

        if has_field_updates {
            chains::update_chain_fields(
                conn,
                update.chain_id,
                update.title.as_deref(),
                update.description.as_ref().map(|opt| opt.as_deref()),
                update.notes.as_ref().map(|opt| opt.as_deref()),
                update.language.as_ref().map(|opt| opt.as_deref()),
                update.separator.as_deref(),
            )?;
        }

        // Replace steps if a new sequence was provided.
        // `steps` takes precedence over legacy `prompt_ids`.
        // Validate that all referenced prompts/scripts exist before replacing.
        if let Some(ref steps) = update.steps {
            for step in steps {
                match step.step_type {
                    StepType::Prompt => {
                        prompts::get_prompt(conn, step.item_id)?;
                    }
                    StepType::Script => {
                        scripts::get_script(conn, step.item_id)?;
                    }
                }
            }
            chains::replace_chain_steps_mixed(conn, update.chain_id, steps)?;
        } else if let Some(ref prompt_ids) = update.prompt_ids {
            for &pid in prompt_ids {
                prompts::get_prompt(conn, pid)?;
            }
            chains::replace_chain_steps(conn, update.chain_id, prompt_ids)?;
        }

        // Synchronize tag associations.
        if let Some(ref desired_tag_ids) = update.tag_ids {
            let current_tags = chains::get_tags_for_chain(conn, update.chain_id)?;
            let current_ids: HashSet<i64> = current_tags.iter().map(|t| t.id).collect();
            let desired: HashSet<i64> = desired_tag_ids.iter().copied().collect();
            sync_associations(
                conn,
                update.chain_id,
                &current_ids,
                &desired,
                chains::unlink_chain_tag,
                chains::link_chain_tag,
            )?;
        }

        // Synchronize category associations.
        if let Some(ref desired_cat_ids) = update.category_ids {
            let current_cats = chains::get_categories_for_chain(conn, update.chain_id)?;
            let current_ids: HashSet<i64> = current_cats.iter().map(|c| c.id).collect();
            let desired: HashSet<i64> = desired_cat_ids.iter().copied().collect();
            sync_associations(
                conn,
                update.chain_id,
                &current_ids,
                &desired,
                chains::unlink_chain_category,
                chains::link_chain_category,
            )?;
        }

        // Synchronize collection associations.
        if let Some(ref desired_col_ids) = update.collection_ids {
            let current_cols = chains::get_collections_for_chain(conn, update.chain_id)?;
            let current_ids: HashSet<i64> = current_cols.iter().map(|c| c.id).collect();
            let desired: HashSet<i64> = desired_col_ids.iter().copied().collect();
            sync_associations(
                conn,
                update.chain_id,
                &current_ids,
                &desired,
                chains::unlink_chain_collection,
                chains::link_chain_collection,
            )?;
        }

        chains::get_chain(conn, update.chain_id)
    })?)
}

/// Retrieves a chain with all its resolved steps and associations.
///
/// # Errors
///
/// Returns `ServiceError::Database` if no chain with the given ID exists or the
/// persistence layer fails.
pub fn get_chain(
    cp: &impl ConnectionProvider,
    chain_id: i64,
) -> Result<ChainWithSteps, ServiceError> {
    Ok(cp.with_connection(|conn| chains::get_chain_with_steps(conn, chain_id))?)
}

/// Returns the total number of chains owned by the given user.
///
/// # Errors
///
/// Returns `ServiceError::Database` if the persistence layer fails.
pub fn count_chains(cp: &impl ConnectionProvider, user_id: i64) -> Result<i64, ServiceError> {
    Ok(cp.with_connection(|conn| chains::count_chains(conn, user_id))?)
}

/// Returns chains matching the given filter, wrapped in pagination metadata.
///
/// # Errors
///
/// Returns `ServiceError::Database` if the persistence layer fails.
pub fn list_chains(
    cp: &impl ConnectionProvider,
    filter: &ChainFilter,
) -> Result<PaginatedList<Chain>, ServiceError> {
    Ok(cp.with_connection(|conn| {
        let items = chains::list_chains(conn, filter)?;
        let total = chains::count_filtered_chains(conn, filter)?;
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

/// Deletes a chain and all associated data via CASCADE.
///
/// # Errors
///
/// Returns `ServiceError::Database` if no chain with the given ID exists or the
/// persistence layer fails.
pub fn delete_chain(cp: &impl ConnectionProvider, chain_id: i64) -> Result<(), ServiceError> {
    Ok(cp.with_transaction(|conn| chains::delete_chain(conn, chain_id))?)
}

/// Creates a copy of an existing chain.
///
/// # Errors
///
/// Returns `ServiceError::Database` if no chain with the given ID exists or the
/// persistence layer fails.
pub fn duplicate_chain(cp: &impl ConnectionProvider, chain_id: i64) -> Result<Chain, ServiceError> {
    Ok(cp.with_transaction(|conn| chains::duplicate_chain(conn, chain_id))?)
}

/// Toggles the favorite flag on a chain.
///
/// # Errors
///
/// Returns `ServiceError::Database` if no chain with the given ID exists or the
/// persistence layer fails.
pub fn toggle_chain_favorite(
    cp: &impl ConnectionProvider,
    chain_id: i64,
    is_favorite: bool,
) -> Result<(), ServiceError> {
    Ok(cp.with_connection(|conn| chains::set_chain_favorite(conn, chain_id, is_favorite))?)
}

/// Toggles the archived flag on a chain.
///
/// # Errors
///
/// Returns `ServiceError::Database` if no chain with the given ID exists or the
/// persistence layer fails.
pub fn toggle_chain_archive(
    cp: &impl ConnectionProvider,
    chain_id: i64,
    is_archived: bool,
) -> Result<(), ServiceError> {
    Ok(cp.with_connection(|conn| chains::set_chain_archived(conn, chain_id, is_archived))?)
}

/// Returns the concatenated content of a chain's steps.
///
/// # Errors
///
/// Returns `ServiceError::Database` if no chain with the given ID exists or the
/// persistence layer fails.
pub fn get_composed_content(
    cp: &impl ConnectionProvider,
    chain_id: i64,
) -> Result<String, ServiceError> {
    Ok(cp.with_connection(|conn| chains::get_composed_content(conn, chain_id))?)
}

/// Returns all chains that reference a given prompt.
///
/// # Errors
///
/// Returns `ServiceError::Database` if the persistence layer fails.
pub fn get_chains_for_prompt(
    cp: &impl ConnectionProvider,
    prompt_id: i64,
) -> Result<Vec<Chain>, ServiceError> {
    Ok(cp.with_connection(|conn| chains::get_chains_containing_prompt(conn, prompt_id))?)
}

/// Returns all chains owned by `user_id` that reference a given prompt.
/// Returns an empty list if the prompt does not exist or has no referencing chains.
///
/// # Errors
///
/// Returns `ServiceError::Database` if the persistence layer fails.
pub fn get_chains_for_prompt_by_user(
    cp: &impl ConnectionProvider,
    user_id: i64,
    prompt_id: i64,
) -> Result<Vec<Chain>, ServiceError> {
    Ok(cp.with_connection(|conn| {
        chains::get_chains_containing_prompt_for_user(conn, user_id, prompt_id)
    })?)
}

/// Applies bulk operations (favorite, archive, tag/category/collection changes)
/// to multiple chains in a single transaction. Validates ownership and taxonomy
/// membership before applying any changes.
///
/// # Errors
///
/// Returns `ServiceError::Core(Validation)` if no operations are specified.
/// Returns `ServiceError::Core(NotFound)` if a chain does not exist or belongs
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
            let owner_id = chains::get_chain_owner(conn, id)?;
            if owner_id != user_id {
                return Err(neuronprompter_db::DbError::Core(CoreError::NotFound {
                    entity: "Chain".to_owned(),
                    id,
                }));
            }

            if let Some(fav) = input.set_favorite {
                chains::set_chain_favorite(conn, id, fav)?;
            }
            if let Some(arch) = input.set_archived {
                chains::set_chain_archived(conn, id, arch)?;
            }
            for &tag_id in &input.add_tag_ids {
                chains::link_chain_tag(conn, id, tag_id)?;
            }
            for &tag_id in &input.remove_tag_ids {
                chains::unlink_chain_tag(conn, id, tag_id)?;
            }
            for &cat_id in &input.add_category_ids {
                chains::link_chain_category(conn, id, cat_id)?;
            }
            for &cat_id in &input.remove_category_ids {
                chains::unlink_chain_category(conn, id, cat_id)?;
            }
            for &col_id in &input.add_collection_ids {
                chains::link_chain_collection(conn, id, col_id)?;
            }
            for &col_id in &input.remove_collection_ids {
                chains::unlink_chain_collection(conn, id, col_id)?;
            }
            updated += 1;
        }
        Ok(updated)
    })?)
}
