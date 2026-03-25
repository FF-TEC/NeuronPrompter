// =============================================================================
// Ollama metadata workflow: decomposed metadata extraction from prompt content.
//
// Instead of asking the model to return a complex JSON object (which small
// models often get wrong), this module makes 6 focused single-purpose calls.
// Each sub-call asks for exactly one piece of information in plain text,
// making it robust even with small local models.
// =============================================================================

use serde::{Deserialize, Serialize};

use super::client::OllamaClient;
use crate::ServiceError;

// ---------------------------------------------------------------------------
// System prompts -- one per sub-call, each asking for exactly one thing.
// ---------------------------------------------------------------------------

const DESCRIPTION_PROMPT: &str = "\
You are a technical writer. Read the following text and write a one-sentence \
description of what it does or instructs. \
Reply with only the description, nothing else.";

const LANGUAGE_PROMPT: &str = "\
What language is the following text written in? Reply with only the ISO 639-1 \
two-letter language code (e.g. \"en\", \"de\", \"fr\", \"es\"). \
Nothing else, just the two-letter code.";

const NOTES_PROMPT: &str = "\
You are a prompt analyst. Read the following prompt and write a brief usage note \
(1-2 sentences) explaining when and how this prompt is best used. \
Reply with only the note, nothing else.";

const TAGS_PROMPT: &str = "\
Read the following text and suggest 2-4 short keyword tags that describe its topic. \
Reply with only the tags separated by commas, nothing else. \
Example: coding, python, debugging";

const CATEGORIES_PROMPT: &str = "\
Read the following text and suggest 1-3 broad category names that classify it. \
Reply with only the category names separated by commas, nothing else. \
Example: Development, Writing, Education";

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Structured metadata derived from prompt content by individual Ollama calls.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DerivedMetadata {
    pub description: String,
    pub language: String,
    pub notes: String,
    pub suggested_tags: Vec<String>,
    pub suggested_categories: Vec<String>,
    /// Names of sub-calls that failed (empty on full success).
    pub errors: Vec<String>,
}

// ---------------------------------------------------------------------------
// Individual derive functions
// ---------------------------------------------------------------------------

/// Derives a one-sentence description of the prompt's purpose.
///
/// # Errors
///
/// Returns `ServiceError::OllamaUnavailable` if the server is not reachable.
/// Returns `ServiceError::OllamaError` if the server returns a non-success
/// HTTP status or the response cannot be parsed.
pub async fn derive_description(
    client: &OllamaClient,
    model: &str,
    content: &str,
) -> Result<String, ServiceError> {
    let raw = client
        .generate(model, content, Some(DESCRIPTION_PROMPT), None)
        .await?;
    Ok(raw.trim().trim_matches('"').to_owned())
}

/// Detects the prompt language and returns an ISO 639-1 two-letter code.
///
/// # Errors
///
/// Returns `ServiceError::OllamaUnavailable` if the server is not reachable.
/// Returns `ServiceError::OllamaError` if the server returns a non-success
/// HTTP status or the response cannot be parsed.
pub async fn derive_language(
    client: &OllamaClient,
    model: &str,
    content: &str,
) -> Result<String, ServiceError> {
    let raw = client
        .generate(model, content, Some(LANGUAGE_PROMPT), None)
        .await?;
    let code = raw.trim().trim_matches('"').to_lowercase();
    // Validate: must be exactly 2 lowercase ASCII letters.
    if code.len() == 2 && code.chars().all(|c| c.is_ascii_lowercase()) {
        Ok(code)
    } else {
        Ok("en".to_owned())
    }
}

/// Derives usage notes for the prompt.
///
/// # Errors
///
/// Returns `ServiceError::OllamaUnavailable` if the server is not reachable.
/// Returns `ServiceError::OllamaError` if the server returns a non-success
/// HTTP status or the response cannot be parsed.
pub async fn derive_notes(
    client: &OllamaClient,
    model: &str,
    content: &str,
) -> Result<String, ServiceError> {
    let raw = client
        .generate(model, content, Some(NOTES_PROMPT), None)
        .await?;
    Ok(raw.trim().trim_matches('"').to_owned())
}

/// Suggests keyword tags for the prompt (2-4 items).
///
/// # Errors
///
/// Returns `ServiceError::OllamaUnavailable` if the server is not reachable.
/// Returns `ServiceError::OllamaError` if the server returns a non-success
/// HTTP status or the response cannot be parsed.
pub async fn derive_tags(
    client: &OllamaClient,
    model: &str,
    content: &str,
) -> Result<Vec<String>, ServiceError> {
    let raw = client
        .generate(model, content, Some(TAGS_PROMPT), None)
        .await?;
    Ok(parse_comma_list(&raw, 4))
}

/// Suggests broad category names for the prompt (1-3 items).
///
/// # Errors
///
/// Returns `ServiceError::OllamaUnavailable` if the server is not reachable.
/// Returns `ServiceError::OllamaError` if the server returns a non-success
/// HTTP status or the response cannot be parsed.
pub async fn derive_categories(
    client: &OllamaClient,
    model: &str,
    content: &str,
) -> Result<Vec<String>, ServiceError> {
    let raw = client
        .generate(model, content, Some(CATEGORIES_PROMPT), None)
        .await?;
    Ok(parse_comma_list(&raw, 3))
}

// ---------------------------------------------------------------------------
// Orchestrator
// ---------------------------------------------------------------------------

/// Performance note: Metadata derivation calls are executed sequentially rather than in
/// parallel because Ollama processes inference requests through a single-model queue.
/// Parallelizing would not reduce wall-clock time for single-model deployments. A combined
/// single-prompt approach requesting all metadata as structured JSON would reduce the number
/// of inference calls from 5 to 1, but small local models often produce unreliable JSON
/// output, so decomposed calls with simple text responses are preferred for reliability.
/// Runs all 5 metadata derivation calls sequentially.
///
/// If individual sub-calls fail, the error is recorded in the `errors` vec
/// and a default value is used for that field. Only returns `Err` if the
/// Ollama server is completely unreachable (all calls fail).
///
/// # Errors
///
/// Returns `ServiceError::OllamaUnavailable` if all 5 sub-calls fail,
/// indicating the server is completely unreachable.
pub async fn derive_all_metadata(
    client: &OllamaClient,
    model: &str,
    content: &str,
) -> Result<DerivedMetadata, ServiceError> {
    let mut errors = Vec::new();

    let description = match derive_description(client, model, content).await {
        Ok(v) => v,
        Err(e) => {
            errors.push(format!("description: {e}"));
            String::new()
        }
    };

    let language = match derive_language(client, model, content).await {
        Ok(v) => v,
        Err(e) => {
            errors.push(format!("language: {e}"));
            "en".to_owned()
        }
    };

    let notes = match derive_notes(client, model, content).await {
        Ok(v) => v,
        Err(e) => {
            errors.push(format!("notes: {e}"));
            String::new()
        }
    };

    let suggested_tags = match derive_tags(client, model, content).await {
        Ok(v) => v,
        Err(e) => {
            errors.push(format!("tags: {e}"));
            Vec::new()
        }
    };

    let suggested_categories = match derive_categories(client, model, content).await {
        Ok(v) => v,
        Err(e) => {
            errors.push(format!("categories: {e}"));
            Vec::new()
        }
    };

    // If ALL 5 calls failed, the server is likely down -- propagate the error.
    if errors.len() == 5 {
        return Err(ServiceError::OllamaUnavailable(
            "All metadata derivation calls failed".to_owned(),
        ));
    }

    Ok(DerivedMetadata {
        description,
        language,
        notes,
        suggested_tags,
        suggested_categories,
        errors,
    })
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Parses a comma-separated response into a list of trimmed, non-empty strings.
fn parse_comma_list(response: &str, max_items: usize) -> Vec<String> {
    response
        .split(',')
        .map(|s| s.trim().trim_matches('"').to_owned())
        .filter(|s| !s.is_empty())
        .take(max_items)
        .collect()
}
