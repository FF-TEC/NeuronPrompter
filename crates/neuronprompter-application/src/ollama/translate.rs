// =============================================================================
// Ollama translate workflow: prompt translation to a target language.
//
// Sends the user's prompt content to a local Ollama model with a system
// prompt instructing translation to the specified target language while
// maintaining formatting and technical terminology.
// =============================================================================

use super::client::OllamaClient;
use crate::ServiceError;

/// Translates the prompt content to the specified target language using
/// the Ollama model. Returns the translated text.
///
/// # Errors
///
/// Returns `ServiceError::OllamaUnavailable` if the server is not reachable.
/// Returns `ServiceError::OllamaError` if the server returns a non-success
/// HTTP status or the response cannot be parsed.
pub async fn translate_prompt(
    client: &OllamaClient,
    model: &str,
    content: &str,
    target_language: &str,
) -> Result<String, ServiceError> {
    let system_prompt = format!(
        "Translate the following text to {target_language}. Rules:\n\
         1. Preserve all formatting (markdown, line breaks, indentation)\n\
         2. Keep technical terms, code snippets, and proper nouns unchanged\n\
         3. Maintain the same tone and register\n\
         4. Return only the translated text, no commentary"
    );

    client
        .generate(model, content, Some(&system_prompt), None)
        .await
}
