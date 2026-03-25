// =============================================================================
// MCP server lifecycle: start, stop, stdio listener.
//
// The McpServer wraps the NeuronPrompterMcp tool container and the rmcp
// ServerHandler implementation. When started, it listens on stdin/stdout
// for JSON-RPC 2.0 messages from external AI clients.
// =============================================================================

use std::sync::Arc;

use neuronprompter_db::Database;
use rmcp::ServiceExt;

use crate::McpError;
use crate::auth::ensure_mcp_user;
use crate::tools::NeuronPrompterMcp;

/// Runs the MCP server in headless mode over stdin/stdout.
/// Opens its own database connection, ensures the `mcp_agent` user exists,
/// and serves tool requests until the transport closes.
///
/// # Errors
///
/// Returns `McpError::Service` if the `mcp_agent` user cannot be created or
/// looked up. Returns `McpError::Transport` if the stdio transport fails to
/// initialize or encounters an error during operation.
pub async fn run_stdio_server(db: Arc<Database>) -> Result<(), McpError> {
    let mcp_user = ensure_mcp_user(&db)?;

    tracing::info!(
        "MCP server starting for user: {} (id={})",
        mcp_user.username,
        mcp_user.id
    );

    let tools = NeuronPrompterMcp::new(db, mcp_user.id);
    let transport = rmcp::transport::io::stdio();

    let service = tools
        .serve(transport)
        .await
        .map_err(|e| McpError::Transport(format!("Failed to start MCP service: {e}")))?;

    // Wait until the transport closes (stdin EOF or explicit shutdown).
    service
        .waiting()
        .await
        .map_err(|e| McpError::Transport(format!("MCP service error: {e}")))?;

    tracing::info!("MCP server stopped");
    Ok(())
}
