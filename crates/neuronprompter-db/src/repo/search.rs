// =============================================================================
// Full-text search repository operations.
//
// Implements prompt search using the FTS5 virtual table (prompts_fts) with
// BM25 ranking. Search terms are tokenized, each term gets a prefix wildcard
// appended, and results are ranked by relevance. Optional filters for user,
// tag, category, and collection constrain the result set. Archived prompts
// are excluded by default. Results are limited to 200 entries.
// =============================================================================

use std::fmt::Write;

use neuronprompter_core::domain::prompt::{Prompt, PromptFilter};

use crate::DbError;

/// Default maximum number of search results returned per query.
const DEFAULT_SEARCH_LIMIT: i64 = 200;
/// Absolute maximum number of search results allowed.
const MAX_SEARCH_LIMIT: i64 = 1000;

/// Searches prompts matching the query string with optional filters.
/// Each query term is suffixed with `*` for prefix matching. Results are
/// ranked by FTS5 BM25 relevance score and limited to 200 entries.
///
/// Archived prompts are excluded unless the filter explicitly sets
/// `is_archived = Some(true)`.
///
/// # Errors
///
/// Returns `DbError::Query` if the SQL statement fails.
#[allow(clippy::too_many_lines)]
pub fn search_prompts(
    conn: &rusqlite::Connection,
    user_id: i64,
    query: &str,
    filter: &PromptFilter,
) -> Result<Vec<Prompt>, DbError> {
    // Tokenize the query and append wildcard for prefix matching.
    let fts_query = super::build_fts_query(query);
    if fts_query.is_empty() {
        return Ok(Vec::new());
    }

    let mut sql = String::from(
        "SELECT p.id, p.user_id, p.title, p.content, \
         p.description, p.notes, p.language, p.is_favorite, p.is_archived, \
         p.current_version, p.created_at, p.updated_at \
         FROM prompts p \
         INNER JOIN prompts_fts fts ON fts.rowid = p.id \
         WHERE fts.prompts_fts MATCH ?1 \
         AND p.user_id = ?2",
    );
    let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    param_values.push(Box::new(fts_query));
    param_values.push(Box::new(user_id));
    let mut param_idx: usize = 3;

    // Exclude archived prompts unless explicitly requested.
    let is_archived = filter.is_archived.unwrap_or(false);
    let _ = write!(sql, " AND p.is_archived = ?{param_idx}");
    param_values.push(Box::new(is_archived));
    param_idx += 1;

    if let Some(fav) = filter.is_favorite {
        let _ = write!(sql, " AND p.is_favorite = ?{param_idx}");
        param_values.push(Box::new(fav));
        param_idx += 1;
    }

    if let Some(tag_id) = filter.tag_id {
        let _ = write!(
            sql,
            " AND p.id IN (SELECT prompt_id FROM prompt_tags WHERE tag_id = ?{param_idx})"
        );
        param_values.push(Box::new(tag_id));
        param_idx += 1;
    }

    if let Some(cat_id) = filter.category_id {
        let _ = write!(
            sql,
            " AND p.id IN (SELECT prompt_id FROM prompt_categories WHERE category_id = ?{param_idx})"
        );
        param_values.push(Box::new(cat_id));
        param_idx += 1;
    }

    if let Some(col_id) = filter.collection_id {
        let _ = write!(
            sql,
            " AND p.id IN (SELECT prompt_id FROM prompt_collections WHERE collection_id = ?{param_idx})"
        );
        param_values.push(Box::new(col_id));
        param_idx += 1;
    }

    if let Some(has_vars) = filter.has_variables {
        // The LIKE '%{{%' pattern is a rough approximation of the template variable
        // syntax {{identifier}}. It may produce false positives for content containing
        // literal double-braces (e.g. JSON or Mustache templates). This is an acceptable
        // trade-off since SQL LIKE cannot express the full regex needed for precise matching.
        if has_vars {
            sql.push_str(" AND p.content LIKE '%{{%'");
        } else {
            sql.push_str(" AND p.content NOT LIKE '%{{%'");
        }
    }

    if let Some(ref var_name) = filter.variable_name {
        let escaped = var_name.replace('%', r"\%").replace('_', r"\_");
        let pattern = format!("%{{{{{escaped}}}}}%");
        let _ = write!(sql, " AND p.content LIKE ?{param_idx} ESCAPE '\\'");
        param_values.push(Box::new(pattern));
        param_idx += 1;
    }

    let effective_limit = filter
        .limit
        .unwrap_or(DEFAULT_SEARCH_LIMIT)
        .clamp(1, MAX_SEARCH_LIMIT);
    let _ = write!(sql, " ORDER BY bm25(prompts_fts) LIMIT ?{param_idx}");
    param_values.push(Box::new(effective_limit));
    param_idx += 1;

    // OFFSET is omitted when the effective offset is zero as an optimization.
    // SQLite treats a missing OFFSET clause identically to OFFSET 0, so the
    // query result is the same in both cases.
    let effective_offset = filter.offset.unwrap_or(0).max(0);
    if effective_offset > 0 {
        let _ = write!(sql, " OFFSET ?{param_idx}");
        param_values.push(Box::new(effective_offset));
    }

    let mut stmt = conn.prepare(&sql).map_err(|e| DbError::Query {
        operation: "search_prompts".to_owned(),
        source: e,
    })?;

    let params_ref: Vec<&dyn rusqlite::types::ToSql> =
        param_values.iter().map(AsRef::as_ref).collect();

    let rows = stmt
        .query_map(params_ref.as_slice(), super::prompts::row_to_prompt)
        .map_err(|e| DbError::Query {
            operation: "search_prompts".to_owned(),
            source: e,
        })?;

    let mut result = Vec::new();
    for row in rows {
        result.push(row.map_err(|e| DbError::Query {
            operation: "search_prompts".to_owned(),
            source: e,
        })?);
    }
    Ok(result)
}
