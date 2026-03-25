// =============================================================================
// Ollama improve workflow: prompt refinement for clarity and specificity.
//
// Sends the user's prompt content to a local Ollama model with a system
// prompt instructing it to refine the text for clarity, specificity, and
// effectiveness while preserving the original intent and language.
// =============================================================================

use super::client::OllamaClient;
use crate::ServiceError;

/// System prompt instructing the model to refine the user's prompt text.
const IMPROVE_SYSTEM_PROMPT: &str = "\
You are a prompt engineering expert. Rewrite the following prompt to be more \
specific and actionable. Rules:\n\
1. Keep the same language as the original\n\
2. Keep the same formatting style (markdown, plain text, etc.)\n\
3. Make vague instructions concrete\n\
4. Add missing context that would help an AI understand the task\n\
5. Do not add explanations or commentary\n\
6. Return only the rewritten prompt";

/// Sends the prompt content to the Ollama model for refinement.
/// Returns the model's suggested version of the prompt.
///
/// # Errors
///
/// Returns `ServiceError::OllamaUnavailable` if the server is not reachable.
/// Returns `ServiceError::OllamaError` if the server returns a non-success
/// HTTP status or the response cannot be parsed.
pub async fn improve_prompt(
    client: &OllamaClient,
    model: &str,
    content: &str,
) -> Result<String, ServiceError> {
    client
        .generate(model, content, Some(IMPROVE_SYSTEM_PROMPT), None)
        .await
}
