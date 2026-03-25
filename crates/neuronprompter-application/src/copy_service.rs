// =============================================================================
// Copy service: cross-user copy operations.
//
// Handles copying prompts, scripts, and chains from one user to another,
// including taxonomy resolution (tags, categories, collections), title
// conflict handling, chain deep-copy with step remapping, and bulk copy.
// =============================================================================

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use neuronprompter_core::domain::chain::StepType;
use neuronprompter_db::ConnectionProvider;
use neuronprompter_db::repo::{chains, prompts, scripts, users};

use crate::ServiceError;
use crate::io_service;

// ---------------------------------------------------------------------------
// Public data structures
// ---------------------------------------------------------------------------

/// Summary of a cross-user copy operation.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CopySummary {
    pub prompts_copied: usize,
    pub scripts_copied: usize,
    pub chains_copied: usize,
    pub tags_created: usize,
    pub categories_created: usize,
    pub collections_created: usize,
    pub skipped: Vec<SkippedItem>,
}

/// An item that was skipped during a copy operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkippedItem {
    pub entity_type: String,
    pub title: String,
    pub reason: String,
}

// ---------------------------------------------------------------------------
// Title generation
// ---------------------------------------------------------------------------

/// Maximum number of suffix attempts before giving up.
const MAX_TITLE_ATTEMPTS: usize = 10;

/// Generates a unique title for the copied entity in the target user's
/// namespace. If the original title is available, it is used as-is.
/// Otherwise, appends `(from <source_username>)` and increments a counter.
fn generate_prompt_title(
    conn: &rusqlite::Connection,
    original_title: &str,
    source_username: &str,
    target_user_id: i64,
) -> Result<String, ServiceError> {
    if !prompts::prompt_title_exists(conn, target_user_id, original_title)? {
        return Ok(original_title.to_owned());
    }
    let candidate = format!("{original_title} (from {source_username})");
    if !prompts::prompt_title_exists(conn, target_user_id, &candidate)? {
        return Ok(candidate);
    }
    for i in 2..=MAX_TITLE_ATTEMPTS {
        let numbered = format!("{original_title} (from {source_username} {i})");
        if !prompts::prompt_title_exists(conn, target_user_id, &numbered)? {
            return Ok(numbered);
        }
    }
    Err(ServiceError::Core(
        neuronprompter_core::CoreError::Validation {
            field: "title".to_owned(),
            message: format!(
                "Could not generate unique title for \"{original_title}\" after {MAX_TITLE_ATTEMPTS} attempts"
            ),
        },
    ))
}

fn generate_script_title(
    conn: &rusqlite::Connection,
    original_title: &str,
    source_username: &str,
    target_user_id: i64,
) -> Result<String, ServiceError> {
    if !scripts::script_title_exists(conn, target_user_id, original_title)? {
        return Ok(original_title.to_owned());
    }
    let candidate = format!("{original_title} (from {source_username})");
    if !scripts::script_title_exists(conn, target_user_id, &candidate)? {
        return Ok(candidate);
    }
    for i in 2..=MAX_TITLE_ATTEMPTS {
        let numbered = format!("{original_title} (from {source_username} {i})");
        if !scripts::script_title_exists(conn, target_user_id, &numbered)? {
            return Ok(numbered);
        }
    }
    Err(ServiceError::Core(
        neuronprompter_core::CoreError::Validation {
            field: "title".to_owned(),
            message: format!(
                "Could not generate unique title for \"{original_title}\" after {MAX_TITLE_ATTEMPTS} attempts"
            ),
        },
    ))
}

fn generate_chain_title(
    conn: &rusqlite::Connection,
    original_title: &str,
    source_username: &str,
    target_user_id: i64,
) -> Result<String, ServiceError> {
    if !chains::chain_title_exists(conn, target_user_id, original_title)? {
        return Ok(original_title.to_owned());
    }
    let candidate = format!("{original_title} (from {source_username})");
    if !chains::chain_title_exists(conn, target_user_id, &candidate)? {
        return Ok(candidate);
    }
    for i in 2..=MAX_TITLE_ATTEMPTS {
        let numbered = format!("{original_title} (from {source_username} {i})");
        if !chains::chain_title_exists(conn, target_user_id, &numbered)? {
            return Ok(numbered);
        }
    }
    Err(ServiceError::Core(
        neuronprompter_core::CoreError::Validation {
            field: "title".to_owned(),
            message: format!(
                "Could not generate unique title for \"{original_title}\" after {MAX_TITLE_ATTEMPTS} attempts"
            ),
        },
    ))
}

// ---------------------------------------------------------------------------
// Taxonomy extraction helpers
// ---------------------------------------------------------------------------

fn tag_names(tags: &[neuronprompter_core::domain::tag::Tag]) -> Vec<String> {
    tags.iter().map(|t| t.name.clone()).collect()
}

fn category_names(cats: &[neuronprompter_core::domain::category::Category]) -> Vec<String> {
    cats.iter().map(|c| c.name.clone()).collect()
}

fn collection_names(cols: &[neuronprompter_core::domain::collection::Collection]) -> Vec<String> {
    cols.iter().map(|c| c.name.clone()).collect()
}

// ---------------------------------------------------------------------------
// Error conversion helper
// ---------------------------------------------------------------------------

/// Converts a `ServiceError` into a `DbError` for use inside
/// `ConnectionProvider` closures that must return `Result<T, DbError>`.
fn service_err_to_db(e: ServiceError) -> neuronprompter_db::DbError {
    match e {
        ServiceError::Core(ce) => neuronprompter_db::DbError::Core(ce),
        ServiceError::Database(de) => de,
        other => neuronprompter_db::DbError::Core(neuronprompter_core::CoreError::Validation {
            field: "copy".to_owned(),
            message: other.to_string(),
        }),
    }
}

// ---------------------------------------------------------------------------
// Per-item copy: Prompt
// ---------------------------------------------------------------------------

/// Copies a single prompt from one user to another. Resolves taxonomy by name,
/// auto-creating missing items in the target user's namespace.
///
/// # Errors
///
/// Returns `ServiceError::Core(Validation)` if the source and target user are
/// the same, or if a unique title cannot be generated.
/// Returns `ServiceError::Database` if the prompt or either user does not exist,
/// or the persistence layer fails.
pub fn copy_prompt_to_user(
    cp: &impl ConnectionProvider,
    prompt_id: i64,
    target_user_id: i64,
) -> Result<CopySummary, ServiceError> {
    // F9: Use transaction so taxonomy creation + entity copy roll back together.
    Ok(cp.with_transaction(|conn| {
        copy_prompt_to_user_inner(conn, prompt_id, target_user_id).map_err(service_err_to_db)
    })?)
}

fn copy_prompt_to_user_inner(
    conn: &rusqlite::Connection,
    prompt_id: i64,
    target_user_id: i64,
) -> Result<CopySummary, ServiceError> {
    let pwa = prompts::get_prompt_with_associations(conn, prompt_id)?;
    let source_user = users::get_user(conn, pwa.prompt.user_id)?;
    // Validate target user exists (will return NotFound if not).
    users::get_user(conn, target_user_id)?;

    if pwa.prompt.user_id == target_user_id {
        return Err(ServiceError::Core(
            neuronprompter_core::CoreError::Validation {
                field: "target_user_id".to_owned(),
                message: "Cannot copy to the same user".to_owned(),
            },
        ));
    }

    let mut summary = CopySummary::default();

    // Check for deduplication (same title + content already exists at target)
    if let Some(_existing) = prompts::find_prompt_by_title_and_content(
        conn,
        target_user_id,
        &pwa.prompt.title,
        &pwa.prompt.content,
    )? {
        summary.skipped.push(SkippedItem {
            entity_type: "prompt".to_owned(),
            title: pwa.prompt.title.clone(),
            reason: "Already exists with identical title and content".to_owned(),
        });
        return Ok(summary);
    }

    // Resolve taxonomy
    let tag_ids = io_service::resolve_tags(
        conn,
        target_user_id,
        &tag_names(&pwa.tags),
        &mut summary.tags_created,
    )?;
    let cat_ids = io_service::resolve_categories(
        conn,
        target_user_id,
        &category_names(&pwa.categories),
        &mut summary.categories_created,
    )?;
    let col_ids = io_service::resolve_collections(
        conn,
        target_user_id,
        &collection_names(&pwa.collections),
        &mut summary.collections_created,
    )?;

    // Generate unique title
    let new_title = generate_prompt_title(
        conn,
        &pwa.prompt.title,
        &source_user.username,
        target_user_id,
    )?;

    prompts::copy_prompt_to_user(
        conn,
        prompt_id,
        target_user_id,
        &new_title,
        &tag_ids,
        &cat_ids,
        &col_ids,
    )?;

    summary.prompts_copied = 1;
    Ok(summary)
}

// ---------------------------------------------------------------------------
// Per-item copy: Script
// ---------------------------------------------------------------------------

/// Copies a single script from one user to another.
///
/// # Errors
///
/// Returns `ServiceError::Core(Validation)` if the source and target user are
/// the same, or if a unique title cannot be generated.
/// Returns `ServiceError::Database` if the script or either user does not exist,
/// or the persistence layer fails.
pub fn copy_script_to_user(
    cp: &impl ConnectionProvider,
    script_id: i64,
    target_user_id: i64,
) -> Result<CopySummary, ServiceError> {
    // F9: Use transaction so taxonomy creation + entity copy roll back together.
    Ok(cp.with_transaction(|conn| {
        copy_script_to_user_inner(conn, script_id, target_user_id).map_err(service_err_to_db)
    })?)
}

fn copy_script_to_user_inner(
    conn: &rusqlite::Connection,
    script_id: i64,
    target_user_id: i64,
) -> Result<CopySummary, ServiceError> {
    let swa = scripts::get_script_with_associations(conn, script_id)?;
    let source_user = users::get_user(conn, swa.script.user_id)?;
    // Validate target user exists (will return NotFound if not).
    users::get_user(conn, target_user_id)?;

    if swa.script.user_id == target_user_id {
        return Err(ServiceError::Core(
            neuronprompter_core::CoreError::Validation {
                field: "target_user_id".to_owned(),
                message: "Cannot copy to the same user".to_owned(),
            },
        ));
    }

    let mut summary = CopySummary::default();

    // Deduplication
    if let Some(_existing) = scripts::find_script_by_title_and_content(
        conn,
        target_user_id,
        &swa.script.title,
        &swa.script.content,
    )? {
        summary.skipped.push(SkippedItem {
            entity_type: "script".to_owned(),
            title: swa.script.title.clone(),
            reason: "Already exists with identical title and content".to_owned(),
        });
        return Ok(summary);
    }

    let tag_ids = io_service::resolve_tags(
        conn,
        target_user_id,
        &tag_names(&swa.tags),
        &mut summary.tags_created,
    )?;
    let cat_ids = io_service::resolve_categories(
        conn,
        target_user_id,
        &category_names(&swa.categories),
        &mut summary.categories_created,
    )?;
    let col_ids = io_service::resolve_collections(
        conn,
        target_user_id,
        &collection_names(&swa.collections),
        &mut summary.collections_created,
    )?;

    let new_title = generate_script_title(
        conn,
        &swa.script.title,
        &source_user.username,
        target_user_id,
    )?;

    scripts::copy_script_to_user(
        conn,
        script_id,
        target_user_id,
        &new_title,
        &tag_ids,
        &cat_ids,
        &col_ids,
    )?;

    summary.scripts_copied = 1;
    Ok(summary)
}

// ---------------------------------------------------------------------------
// Per-item copy: Chain (deep copy)
// ---------------------------------------------------------------------------

/// Copies a chain and all its referenced prompts/scripts to another user.
/// Existing prompts/scripts with matching title+content are reused.
///
/// # Errors
///
/// Returns `ServiceError::Core(Validation)` if the source and target user are
/// the same, or if a unique title cannot be generated.
/// Returns `ServiceError::Database` if the chain or either user does not exist,
/// or the persistence layer fails.
pub fn copy_chain_to_user(
    cp: &impl ConnectionProvider,
    chain_id: i64,
    target_user_id: i64,
) -> Result<CopySummary, ServiceError> {
    // F9: Use transaction so taxonomy creation + step remapping roll back together.
    Ok(cp.with_transaction(|conn| {
        copy_chain_to_user_inner(conn, chain_id, target_user_id).map_err(service_err_to_db)
    })?)
}

#[allow(clippy::too_many_lines)]
fn copy_chain_to_user_inner(
    conn: &rusqlite::Connection,
    chain_id: i64,
    target_user_id: i64,
) -> Result<CopySummary, ServiceError> {
    let cws = chains::get_chain_with_steps(conn, chain_id)?;
    let source_user = users::get_user(conn, cws.chain.user_id)?;
    // Validate target user exists (will return NotFound if not).
    users::get_user(conn, target_user_id)?;

    if cws.chain.user_id == target_user_id {
        return Err(ServiceError::Core(
            neuronprompter_core::CoreError::Validation {
                field: "target_user_id".to_owned(),
                message: "Cannot copy to the same user".to_owned(),
            },
        ));
    }

    let mut summary = CopySummary::default();

    // Validate all steps are resolvable before starting the copy
    for resolved_step in &cws.steps {
        let valid = match resolved_step.step.step_type {
            StepType::Prompt => resolved_step.prompt.is_some(),
            StepType::Script => resolved_step.script.is_some(),
        };
        if !valid {
            summary.skipped.push(SkippedItem {
                entity_type: "chain".to_owned(),
                title: cws.chain.title.clone(),
                reason: "Contains unresolvable steps".to_owned(),
            });
            return Ok(summary);
        }
    }

    // Deep-copy each step's referenced entity and build the step mapping
    let mut step_mapping: Vec<(String, i64)> = Vec::new();
    let mut prompt_id_map: HashMap<i64, i64> = HashMap::new();
    let mut script_id_map: HashMap<i64, i64> = HashMap::new();

    for resolved_step in &cws.steps {
        match resolved_step.step.step_type {
            StepType::Prompt => {
                if let Some(prompt) = &resolved_step.prompt {
                    let new_id = if let Some(&mapped_id) = prompt_id_map.get(&prompt.id) {
                        mapped_id
                    } else {
                        let new_id = copy_or_reuse_prompt(
                            conn,
                            prompt.id,
                            target_user_id,
                            &source_user.username,
                            &mut summary,
                        )?;
                        prompt_id_map.insert(prompt.id, new_id);
                        new_id
                    };
                    step_mapping.push(("prompt".to_owned(), new_id));
                }
            }
            StepType::Script => {
                if let Some(script) = &resolved_step.script {
                    let new_id = if let Some(&mapped_id) = script_id_map.get(&script.id) {
                        mapped_id
                    } else {
                        let new_id = copy_or_reuse_script(
                            conn,
                            script.id,
                            target_user_id,
                            &source_user.username,
                            &mut summary,
                        )?;
                        script_id_map.insert(script.id, new_id);
                        new_id
                    };
                    step_mapping.push(("script".to_owned(), new_id));
                }
            }
        }
    }

    // Resolve chain taxonomy
    let tag_ids = io_service::resolve_tags(
        conn,
        target_user_id,
        &tag_names(&cws.tags),
        &mut summary.tags_created,
    )?;
    let cat_ids = io_service::resolve_categories(
        conn,
        target_user_id,
        &category_names(&cws.categories),
        &mut summary.categories_created,
    )?;
    let col_ids = io_service::resolve_collections(
        conn,
        target_user_id,
        &collection_names(&cws.collections),
        &mut summary.collections_created,
    )?;

    let new_title = generate_chain_title(
        conn,
        &cws.chain.title,
        &source_user.username,
        target_user_id,
    )?;

    chains::copy_chain_to_user(
        conn,
        chain_id,
        target_user_id,
        &new_title,
        &tag_ids,
        &cat_ids,
        &col_ids,
        &step_mapping,
    )?;

    summary.chains_copied = 1;
    Ok(summary)
}

/// Copies a prompt to the target user or returns the existing ID if an
/// identical prompt (title + content) already exists.
/// Performance note: The prompt is fetched twice (once for deduplication check via
/// get_prompt, once for associations via get_prompt_with_associations). Combining into a
/// single get_prompt_with_associations call would eliminate the redundant fetch.
fn copy_or_reuse_prompt(
    conn: &rusqlite::Connection,
    prompt_id: i64,
    target_user_id: i64,
    source_username: &str,
    summary: &mut CopySummary,
) -> Result<i64, ServiceError> {
    let original = prompts::get_prompt(conn, prompt_id)?;

    // Check if target already has this exact prompt
    if let Some(existing) = prompts::find_prompt_by_title_and_content(
        conn,
        target_user_id,
        &original.title,
        &original.content,
    )? {
        return Ok(existing.id);
    }

    // Get associations for taxonomy resolution
    let pwa = prompts::get_prompt_with_associations(conn, prompt_id)?;

    let tag_ids = io_service::resolve_tags(
        conn,
        target_user_id,
        &tag_names(&pwa.tags),
        &mut summary.tags_created,
    )?;
    let cat_ids = io_service::resolve_categories(
        conn,
        target_user_id,
        &category_names(&pwa.categories),
        &mut summary.categories_created,
    )?;
    let col_ids = io_service::resolve_collections(
        conn,
        target_user_id,
        &collection_names(&pwa.collections),
        &mut summary.collections_created,
    )?;

    let new_title = generate_prompt_title(conn, &original.title, source_username, target_user_id)?;

    let new_prompt = prompts::copy_prompt_to_user(
        conn,
        prompt_id,
        target_user_id,
        &new_title,
        &tag_ids,
        &cat_ids,
        &col_ids,
    )?;

    summary.prompts_copied += 1;
    Ok(new_prompt.id)
}

/// Copies a script to the target user or returns the existing ID if an
/// identical script (title + content) already exists.
fn copy_or_reuse_script(
    conn: &rusqlite::Connection,
    script_id: i64,
    target_user_id: i64,
    source_username: &str,
    summary: &mut CopySummary,
) -> Result<i64, ServiceError> {
    let original = scripts::get_script(conn, script_id)?;

    if let Some(existing) = scripts::find_script_by_title_and_content(
        conn,
        target_user_id,
        &original.title,
        &original.content,
    )? {
        return Ok(existing.id);
    }

    let swa = scripts::get_script_with_associations(conn, script_id)?;

    let tag_ids = io_service::resolve_tags(
        conn,
        target_user_id,
        &tag_names(&swa.tags),
        &mut summary.tags_created,
    )?;
    let cat_ids = io_service::resolve_categories(
        conn,
        target_user_id,
        &category_names(&swa.categories),
        &mut summary.categories_created,
    )?;
    let col_ids = io_service::resolve_collections(
        conn,
        target_user_id,
        &collection_names(&swa.collections),
        &mut summary.collections_created,
    )?;

    let new_title = generate_script_title(conn, &original.title, source_username, target_user_id)?;

    let new_script = scripts::copy_script_to_user(
        conn,
        script_id,
        target_user_id,
        &new_title,
        &tag_ids,
        &cat_ids,
        &col_ids,
    )?;

    summary.scripts_copied += 1;
    Ok(new_script.id)
}

// ---------------------------------------------------------------------------
// Bulk copy: all items from one user to another
// ---------------------------------------------------------------------------

/// Copies all prompts, scripts, and chains from one user to another.
/// Items with identical title+content at the target are skipped.
/// Chain steps are remapped to the newly-created target entities.
/// The entire operation runs inside a single transaction for atomicity.
///
/// # Errors
///
/// Returns `ServiceError::Core(Validation)` if the source and target user are
/// the same, or if a unique title cannot be generated for any entity.
/// Returns `ServiceError::Database` if either user does not exist or the
/// persistence layer fails.
pub fn bulk_copy_all(
    cp: &impl ConnectionProvider,
    source_user_id: i64,
    target_user_id: i64,
) -> Result<CopySummary, ServiceError> {
    Ok(cp.with_transaction(|conn| {
        bulk_copy_all_inner(conn, source_user_id, target_user_id).map_err(service_err_to_db)
    })?)
}

#[allow(clippy::too_many_lines)]
fn bulk_copy_all_inner(
    conn: &rusqlite::Connection,
    source_user_id: i64,
    target_user_id: i64,
) -> Result<CopySummary, ServiceError> {
    let source_user = users::get_user(conn, source_user_id)?;
    // Validate target user exists (will return NotFound if not).
    users::get_user(conn, target_user_id)?;

    if source_user_id == target_user_id {
        return Err(ServiceError::Core(
            neuronprompter_core::CoreError::Validation {
                field: "target_user_id".to_owned(),
                message: "Cannot copy to the same user".to_owned(),
            },
        ));
    }

    let mut summary = CopySummary::default();
    let mut prompt_id_map: HashMap<i64, i64> = HashMap::new();
    let mut script_id_map: HashMap<i64, i64> = HashMap::new();

    // 1. Copy all prompts (F6: use list_all_prompts to avoid pagination truncation)
    let all_prompts = prompts::list_all_prompts(conn, source_user_id)?;

    for prompt in &all_prompts {
        // Check deduplication
        if let Some(existing) = prompts::find_prompt_by_title_and_content(
            conn,
            target_user_id,
            &prompt.title,
            &prompt.content,
        )? {
            prompt_id_map.insert(prompt.id, existing.id);
            summary.skipped.push(SkippedItem {
                entity_type: "prompt".to_owned(),
                title: prompt.title.clone(),
                reason: "Already exists with identical title and content".to_owned(),
            });
            continue;
        }

        let new_id = copy_or_reuse_prompt(
            conn,
            prompt.id,
            target_user_id,
            &source_user.username,
            &mut summary,
        )?;
        prompt_id_map.insert(prompt.id, new_id);
    }

    // 2. Copy all scripts (F6: use list_all_scripts to avoid pagination truncation)
    let all_scripts = scripts::list_all_scripts(conn, source_user_id)?;

    for script in &all_scripts {
        if let Some(existing) = scripts::find_script_by_title_and_content(
            conn,
            target_user_id,
            &script.title,
            &script.content,
        )? {
            script_id_map.insert(script.id, existing.id);
            summary.skipped.push(SkippedItem {
                entity_type: "script".to_owned(),
                title: script.title.clone(),
                reason: "Already exists with identical title and content".to_owned(),
            });
            continue;
        }

        let new_id = copy_or_reuse_script(
            conn,
            script.id,
            target_user_id,
            &source_user.username,
            &mut summary,
        )?;
        script_id_map.insert(script.id, new_id);
    }

    // 3. Copy all chains with remapped steps (F6: use list_all_chains to avoid pagination truncation)
    let all_chains = chains::list_all_chains(conn, source_user_id)?;

    for chain in &all_chains {
        let cws = chains::get_chain_with_steps(conn, chain.id)?;

        // Build step mapping from existing ID maps
        let mut step_mapping: Vec<(String, i64)> = Vec::new();
        let mut steps_valid = true;

        for resolved_step in &cws.steps {
            match resolved_step.step.step_type {
                StepType::Prompt => {
                    if let Some(prompt) = &resolved_step.prompt {
                        if let Some(&new_id) = prompt_id_map.get(&prompt.id) {
                            step_mapping.push(("prompt".to_owned(), new_id));
                        } else {
                            // Prompt wasn't in the bulk set -- copy it now
                            let new_id = copy_or_reuse_prompt(
                                conn,
                                prompt.id,
                                target_user_id,
                                &source_user.username,
                                &mut summary,
                            )?;
                            prompt_id_map.insert(prompt.id, new_id);
                            step_mapping.push(("prompt".to_owned(), new_id));
                        }
                    } else {
                        steps_valid = false;
                    }
                }
                StepType::Script => {
                    if let Some(script) = &resolved_step.script {
                        if let Some(&new_id) = script_id_map.get(&script.id) {
                            step_mapping.push(("script".to_owned(), new_id));
                        } else {
                            let new_id = copy_or_reuse_script(
                                conn,
                                script.id,
                                target_user_id,
                                &source_user.username,
                                &mut summary,
                            )?;
                            script_id_map.insert(script.id, new_id);
                            step_mapping.push(("script".to_owned(), new_id));
                        }
                    } else {
                        steps_valid = false;
                    }
                }
            }
        }

        if !steps_valid {
            summary.skipped.push(SkippedItem {
                entity_type: "chain".to_owned(),
                title: chain.title.clone(),
                reason: "Contains invalid or unresolvable steps".to_owned(),
            });
            continue;
        }

        // Resolve chain taxonomy
        let tag_ids = io_service::resolve_tags(
            conn,
            target_user_id,
            &tag_names(&cws.tags),
            &mut summary.tags_created,
        )?;
        let cat_ids = io_service::resolve_categories(
            conn,
            target_user_id,
            &category_names(&cws.categories),
            &mut summary.categories_created,
        )?;
        let col_ids = io_service::resolve_collections(
            conn,
            target_user_id,
            &collection_names(&cws.collections),
            &mut summary.collections_created,
        )?;

        let new_title =
            generate_chain_title(conn, &chain.title, &source_user.username, target_user_id)?;

        chains::copy_chain_to_user(
            conn,
            chain.id,
            target_user_id,
            &new_title,
            &tag_ids,
            &cat_ids,
            &col_ids,
            &step_mapping,
        )?;

        summary.chains_copied += 1;
    }

    Ok(summary)
}
