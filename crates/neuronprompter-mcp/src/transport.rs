// =============================================================================
// JSON-RPC 2.0 message framing over stdin/stdout via rmcp transport-io.
//
// The rmcp crate handles JSON-RPC framing, serialization, and transport
// management. This module re-exports the transport initialization for use
// by the server module.
// =============================================================================

// Transport functionality is provided by rmcp::transport::io::stdio().
// No additional wrapper is required; the server module uses it directly.
