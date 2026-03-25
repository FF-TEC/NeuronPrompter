// =============================================================================
// MCP user authentication: find or create the `mcp_agent` system user.
//
// All MCP operations are scoped to a dedicated system user to enforce
// access boundaries. This module ensures the `mcp_agent` user exists in the
// database, creating it on first use.
// =============================================================================

use neuronprompter_application::user_service;
use neuronprompter_core::domain::user::{NewUser, User};
use neuronprompter_db::Database;

use crate::McpError;

/// The reserved username for the MCP system user. Uses underscores (not
/// hyphens) to satisfy the username validation rule `[a-z0-9_]`.
const MCP_USERNAME: &str = "mcp_agent";

/// The display name for the MCP system user.
const MCP_DISPLAY_NAME: &str = "MCP Agent";

/// Ensures the `mcp_agent` user exists in the database, creating it if absent.
/// Returns the user record for use as the session identity.
///
/// # Errors
///
/// Returns `McpError::Service` if listing users or creating the `mcp_agent`
/// user fails due to a database or validation error.
pub fn ensure_mcp_user(db: &Database) -> Result<User, McpError> {
    let users = user_service::list_users(db).map_err(McpError::Service)?;

    if let Some(mcp_user) = users.into_iter().find(|u| u.username == MCP_USERNAME) {
        return Ok(mcp_user);
    }

    let new_user = NewUser {
        username: MCP_USERNAME.to_owned(),
        display_name: MCP_DISPLAY_NAME.to_owned(),
    };

    user_service::create_user(db, &new_user).map_err(McpError::Service)
}
