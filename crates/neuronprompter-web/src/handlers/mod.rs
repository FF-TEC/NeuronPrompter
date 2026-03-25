// =============================================================================
// Web-specific handler modules.
//
// These handlers serve endpoints that the browser-based frontend needs but
// the headless API does not expose: MCP registration, Ollama model catalog,
// and pull/delete operations with SSE progress reporting.
// =============================================================================

pub mod dialogs;
pub mod doctor;
pub mod mcp;
pub mod ollama;
pub mod setup;
