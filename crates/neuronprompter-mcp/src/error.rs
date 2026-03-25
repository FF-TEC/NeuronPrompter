// =============================================================================
// MCP error types and ServiceError-to-JSON-RPC mapping.
//
// Contains the top-level McpError enum and the conversion function that maps
// application-layer ServiceError variants to rmcp ErrorData responses with
// appropriate error codes and messages. Internal details (database queries,
// file paths, serialization traces) are logged server-side but replaced with
// generic messages in the JSON-RPC response.
// =============================================================================

use neuronprompter_application::ServiceError;
use neuronprompter_core::CoreError;
use neuronprompter_db::DbError;

/// Top-level error type for MCP server operations.
#[derive(Debug, thiserror::Error)]
pub enum McpError {
    /// Error from the application service layer.
    #[error("service error: {0}")]
    Service(#[from] neuronprompter_application::ServiceError),

    /// Transport-level error (stdin/stdout framing, connection loss).
    #[error("transport error: {0}")]
    Transport(String),
}

/// Converts a `ServiceError` into an rmcp `ErrorData` for JSON-RPC responses.
/// Accepts the error by value because this function is used as a function
/// pointer in `map_err(service_error_to_mcp)` which passes ownership.
#[must_use]
#[allow(clippy::needless_pass_by_value)]
pub fn service_error_to_mcp(err: ServiceError) -> rmcp::ErrorData {
    match err {
        ServiceError::Core(core_err) => core_error_to_mcp(&core_err),
        ServiceError::OllamaUnavailable(_) | ServiceError::OllamaError(_) => {
            rmcp::ErrorData::invalid_request(err.to_string(), None)
        }
        ServiceError::Database(db_err) => match db_err {
            DbError::Core(core_err) => core_error_to_mcp(&core_err),
            other => {
                // Log the database error details server-side; return a generic
                // message in the JSON-RPC response to avoid leaking SQL queries
                // or schema information.
                tracing::error!(error = %other, "MCP database error");
                rmcp::ErrorData::internal_error(
                    "An internal database error occurred".to_owned(),
                    None,
                )
            }
        },
        ServiceError::IoError(ref io_err) => {
            // Log the I/O error details server-side; return a generic message
            // to avoid leaking file system paths or OS error details.
            tracing::error!(error = %io_err, "MCP I/O error");
            rmcp::ErrorData::internal_error("An internal I/O error occurred".to_owned(), None)
        }
        ServiceError::SerializationError(ref msg) => {
            // Log the serialization error details server-side; return a generic
            // message to avoid leaking internal data structure information.
            tracing::error!(error = %msg, "MCP serialization error");
            rmcp::ErrorData::internal_error("A serialization error occurred".to_owned(), None)
        }
    }
}

/// Maps `CoreError` variants to rmcp error responses.
fn core_error_to_mcp(err: &CoreError) -> rmcp::ErrorData {
    match err {
        CoreError::Validation { .. }
        | CoreError::NotFound { .. }
        | CoreError::Duplicate { .. }
        | CoreError::EntityInUse { .. }
        | CoreError::PathTraversal { .. } => rmcp::ErrorData::invalid_params(err.to_string(), None),
        CoreError::Authorization { .. } | CoreError::Conflict { .. } => {
            rmcp::ErrorData::invalid_request(err.to_string(), None)
        }
    }
}
