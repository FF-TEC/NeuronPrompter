// =============================================================================
// Search service: FTS5 full-text search delegation with filter assembly.
//
// Delegates to the search repository which builds FTS5 MATCH queries with
// BM25 ranking. The service layer enforces user scoping and passes through
// optional tag/category/collection filters.
// =============================================================================

use neuronprompter_core::domain::chain::{Chain, ChainFilter};
use neuronprompter_core::domain::prompt::{Prompt, PromptFilter};
use neuronprompter_core::domain::script::{Script, ScriptFilter};
use neuronprompter_db::ConnectionProvider;
use neuronprompter_db::repo::{chains, scripts, search};

use crate::ServiceError;

/// Searches prompts matching the query string within a user's scope.
/// Results are ranked by FTS5 BM25 relevance and limited to 200 entries.
/// Archived prompts are excluded unless the filter explicitly includes them.
///
/// # Errors
///
/// Returns `ServiceError::Database` if the persistence layer fails.
pub fn search_prompts(
    cp: &impl ConnectionProvider,
    user_id: i64,
    query: &str,
    filter: &PromptFilter,
) -> Result<Vec<Prompt>, ServiceError> {
    Ok(cp.with_connection(|conn| search::search_prompts(conn, user_id, query, filter))?)
}

/// Searches chains matching the query string within a user's scope.
///
/// # Errors
///
/// Returns `ServiceError::Database` if the persistence layer fails.
pub fn search_chains(
    cp: &impl ConnectionProvider,
    user_id: i64,
    query: &str,
    filter: &ChainFilter,
) -> Result<Vec<Chain>, ServiceError> {
    Ok(cp.with_connection(|conn| chains::search_chains(conn, user_id, query, filter))?)
}

/// Searches scripts matching the query string within a user's scope.
///
/// # Errors
///
/// Returns `ServiceError::Database` if the persistence layer fails.
pub fn search_scripts(
    cp: &impl ConnectionProvider,
    user_id: i64,
    query: &str,
    filter: &ScriptFilter,
) -> Result<Vec<Script>, ServiceError> {
    Ok(cp.with_connection(|conn| scripts::search_scripts(conn, user_id, query, filter))?)
}
