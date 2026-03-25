// =============================================================================
// Ollama integration modules.
//
// Provides an HTTP client for the Ollama local inference server and three
// workflow modules (improve, translate, metadata) that compose system prompts
// and parse model responses.
// =============================================================================

pub mod client;
pub mod improve;
pub mod metadata;
pub mod translate;
