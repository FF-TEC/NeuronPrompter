// =============================================================================
// MCP tool registration and dispatch table.
//
// Defines the NeuronPrompterMcp struct that holds the database handle and
// mcp_agent user ID. Uses rmcp's #[tool_router] macro to register available
// tools with JSON Schema parameter validation. Access boundary enforcement:
// no delete operations and no Ollama tools are registered.
//
// Each tool method receives parameters via the Parameters<T> extractor
// pattern, delegates to the application service layer, and returns
// serialized JSON as tool output.
// =============================================================================

use std::sync::Arc;

use neuronprompter_application::{
    category_service, chain_service, collection_service, prompt_service, script_service,
    search_service, tag_service,
};
use neuronprompter_core::domain::chain::{ChainFilter, NewChain, UpdateChain};
use neuronprompter_core::domain::prompt::{NewPrompt, PromptFilter, UpdatePrompt};
use neuronprompter_core::domain::script::{NewScript, ScriptFilter, UpdateScript};
use neuronprompter_db::Database;
use rmcp::{
    ServerHandler,
    handler::server::{router::tool::ToolRouter, tool::Parameters},
    model::{CallToolResult, Content, Implementation, ServerCapabilities, ServerInfo},
    schemars, tool, tool_handler, tool_router,
};

use crate::error::service_error_to_mcp;

// ---------------------------------------------------------------------------
// Parameter structs for tool methods.
// Each struct derives Deserialize for JSON-RPC parameter parsing and
// JsonSchema for automatic MCP tool schema generation.
// ---------------------------------------------------------------------------

/// Parameters for the `search_prompts` tool.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct SearchParams {
    /// Full-text search query string.
    #[schemars(description = "Full-text search query")]
    pub query: String,
}

/// Parameters for the `get_prompt` tool.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct GetPromptParams {
    /// Database ID of the prompt to retrieve.
    #[schemars(description = "Prompt database ID")]
    pub prompt_id: i64,
}

/// Parameters for the `create_prompt` tool.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct CreatePromptParams {
    /// Title of the prompt to create.
    #[schemars(description = "Prompt title")]
    pub title: String,
    /// Body content of the prompt.
    #[schemars(description = "Prompt content")]
    pub content: String,
    /// Optional description of the prompt.
    #[schemars(description = "Optional description")]
    pub description: Option<String>,
    /// Optional notes for the prompt.
    #[schemars(description = "Optional notes")]
    pub notes: Option<String>,
    /// Optional language identifier (e.g., "en", "de").
    #[schemars(description = "Optional language identifier (e.g., 'en', 'de')")]
    pub language: Option<String>,
    /// Tag IDs to associate with the prompt.
    #[schemars(description = "Tag IDs to associate with the prompt")]
    pub tag_ids: Option<Vec<i64>>,
    /// Category IDs to associate with the prompt.
    #[schemars(description = "Category IDs to associate with the prompt")]
    pub category_ids: Option<Vec<i64>>,
    /// Collection IDs to associate with the prompt.
    #[schemars(description = "Collection IDs to associate with the prompt")]
    pub collection_ids: Option<Vec<i64>>,
}

/// Parameters for the `update_prompt` tool.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct UpdatePromptParams {
    /// Database ID of the prompt to update.
    #[schemars(description = "Prompt database ID")]
    pub prompt_id: i64,
    /// Replacement title, if changing.
    #[schemars(description = "New title (optional)")]
    pub title: Option<String>,
    /// Replacement content, if changing.
    #[schemars(description = "New content (optional)")]
    pub content: Option<String>,
    /// New description (optional, null to clear).
    #[schemars(description = "New description (optional, null to clear)")]
    pub description: Option<Option<String>>,
    /// New notes (optional, null to clear).
    #[schemars(description = "New notes (optional, null to clear)")]
    pub notes: Option<Option<String>>,
    /// New language (optional, null to clear).
    #[schemars(description = "New language (optional, null to clear)")]
    pub language: Option<Option<String>>,
    /// Tag IDs to set (replaces all tags).
    #[schemars(description = "Tag IDs to set (replaces all tags)")]
    pub tag_ids: Option<Vec<i64>>,
    /// Category IDs to set (replaces all categories).
    #[schemars(description = "Category IDs to set (replaces all categories)")]
    pub category_ids: Option<Vec<i64>>,
    /// Collection IDs to set (replaces all collections).
    #[schemars(description = "Collection IDs to set (replaces all collections)")]
    pub collection_ids: Option<Vec<i64>>,
}

/// Parameters for the `create_tag` tool.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct CreateTagParams {
    /// Name of the tag to create.
    #[schemars(description = "Tag name")]
    pub name: String,
}

/// Parameters for the `create_category` tool.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct CreateCategoryParams {
    /// Name of the category to create.
    #[schemars(description = "Category name")]
    pub name: String,
}

// ---------------------------------------------------------------------------
// Chain tool parameter structs.
// ---------------------------------------------------------------------------

/// Parameters for the `search_chains` tool.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct SearchChainsParams {
    /// Full-text search query for chain title/description.
    #[schemars(description = "Full-text search query for chain title/description")]
    pub query: String,
}

/// Parameters for the `get_chain` tool.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct GetChainParams {
    /// Database ID of the chain to retrieve.
    #[schemars(description = "Chain database ID")]
    pub chain_id: i64,
}

/// Parameters for the `get_chain_content` tool.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct GetChainContentParams {
    /// Database ID of the chain whose composed content to retrieve.
    #[schemars(description = "Chain database ID")]
    pub chain_id: i64,
}

/// Parameters for the `create_chain` tool.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct CreateChainParams {
    /// Title of the chain to create.
    #[schemars(description = "Chain title")]
    pub title: String,
    /// Ordered list of prompt IDs that form the chain steps.
    #[schemars(description = "Ordered list of prompt IDs that form the chain steps")]
    pub prompt_ids: Vec<i64>,
    /// Separator between prompt contents (default: two newlines).
    #[schemars(description = "Separator between prompt contents (default: two newlines)")]
    pub separator: Option<String>,
    /// Optional description of the chain.
    #[schemars(description = "Optional description of the chain")]
    pub description: Option<String>,
    /// Tag IDs to associate with the chain.
    #[schemars(description = "Tag IDs to associate with the chain")]
    pub tag_ids: Option<Vec<i64>>,
    /// Category IDs to associate with the chain.
    #[schemars(description = "Category IDs to associate with the chain")]
    pub category_ids: Option<Vec<i64>>,
    /// Collection IDs to associate with the chain.
    #[schemars(description = "Collection IDs to associate with the chain")]
    pub collection_ids: Option<Vec<i64>>,
}

/// Parameters for the `update_chain` tool.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct UpdateChainParams {
    /// Database ID of the chain to update.
    #[schemars(description = "Chain database ID")]
    pub chain_id: i64,
    /// New title (optional).
    #[schemars(description = "New title (optional)")]
    pub title: Option<String>,
    /// New description (optional, null to clear).
    #[schemars(description = "New description (optional, null to clear)")]
    pub description: Option<Option<String>>,
    /// New separator (optional).
    #[schemars(description = "New separator (optional)")]
    pub separator: Option<String>,
    /// New ordered list of prompt IDs (replaces all steps).
    #[schemars(description = "New ordered list of prompt IDs (replaces all steps)")]
    pub prompt_ids: Option<Vec<i64>>,
    /// Tag IDs to set (replaces all tags).
    #[schemars(description = "Tag IDs to set (replaces all tags)")]
    pub tag_ids: Option<Vec<i64>>,
    /// Category IDs to set (replaces all categories).
    #[schemars(description = "Category IDs to set (replaces all categories)")]
    pub category_ids: Option<Vec<i64>>,
    /// Collection IDs to set (replaces all collections).
    #[schemars(description = "Collection IDs to set (replaces all collections)")]
    pub collection_ids: Option<Vec<i64>>,
}

/// Parameters for the `chains_for_prompt` tool.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ChainsForPromptParams {
    /// Prompt database ID to find referencing chains for.
    #[schemars(description = "Prompt database ID to find referencing chains for")]
    pub prompt_id: i64,
}

/// Parameters for the `duplicate_chain` tool.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct DuplicateChainParams {
    /// Database ID of the chain to duplicate.
    #[schemars(description = "Chain database ID to duplicate")]
    pub chain_id: i64,
}

// ---------------------------------------------------------------------------
// Script tool parameter structs.
// ---------------------------------------------------------------------------

/// Parameters for the `search_scripts` tool.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct SearchScriptsParams {
    /// Full-text search query for script content and metadata.
    #[schemars(description = "Full-text search query")]
    pub query: String,
}

/// Parameters for the `get_script` tool.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct GetScriptParams {
    /// Database ID of the script to retrieve.
    #[schemars(description = "Script database ID")]
    pub script_id: i64,
}

/// Parameters for the `create_script` tool.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct CreateScriptParams {
    /// Title of the script to create.
    #[schemars(description = "Script title")]
    pub title: String,
    /// Code content of the script.
    #[schemars(description = "Script code content")]
    pub content: String,
    /// Programming language identifier (e.g., "python", "bash", "javascript").
    #[schemars(description = "Programming language (e.g., 'python', 'bash', 'javascript')")]
    pub script_language: String,
    /// Optional description of the script.
    #[schemars(description = "Optional description")]
    pub description: Option<String>,
    /// Optional notes for the script.
    #[schemars(description = "Optional notes")]
    pub notes: Option<String>,
    /// Optional language identifier (e.g., "en", "de").
    #[schemars(description = "Optional language identifier (e.g., 'en', 'de')")]
    pub language: Option<String>,
    /// Tag IDs to associate with the script.
    #[schemars(description = "Tag IDs to associate with the script")]
    pub tag_ids: Option<Vec<i64>>,
    /// Category IDs to associate with the script.
    #[schemars(description = "Category IDs to associate with the script")]
    pub category_ids: Option<Vec<i64>>,
    /// Collection IDs to associate with the script.
    #[schemars(description = "Collection IDs to associate with the script")]
    pub collection_ids: Option<Vec<i64>>,
}

/// Parameters for the `update_script` tool.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct UpdateScriptParams {
    /// Database ID of the script to update.
    #[schemars(description = "Script database ID")]
    pub script_id: i64,
    /// Replacement title, if changing.
    #[schemars(description = "New title (optional)")]
    pub title: Option<String>,
    /// Replacement content, if changing.
    #[schemars(description = "New content (optional)")]
    pub content: Option<String>,
    /// Replacement programming language, if changing.
    #[schemars(description = "New programming language (optional)")]
    pub script_language: Option<String>,
    /// New description (optional, null to clear).
    #[schemars(description = "New description (optional, null to clear)")]
    pub description: Option<Option<String>>,
    /// New notes (optional, null to clear).
    #[schemars(description = "New notes (optional, null to clear)")]
    pub notes: Option<Option<String>>,
    /// New language (optional, null to clear).
    #[schemars(description = "New language (optional, null to clear)")]
    pub language: Option<Option<String>>,
    /// Tag IDs to set (replaces all tags).
    #[schemars(description = "Tag IDs to set (replaces all tags)")]
    pub tag_ids: Option<Vec<i64>>,
    /// Category IDs to set (replaces all categories).
    #[schemars(description = "Category IDs to set (replaces all categories)")]
    pub category_ids: Option<Vec<i64>>,
    /// Collection IDs to set (replaces all collections).
    #[schemars(description = "Collection IDs to set (replaces all collections)")]
    pub collection_ids: Option<Vec<i64>>,
}

// ---------------------------------------------------------------------------
// MCP server tool container.
// ---------------------------------------------------------------------------

/// MCP server tool container holding the database handle and the
/// `mcp_agent` user ID for session scoping. The `tool_router` field stores
/// the generated dispatch table for all registered tool methods.
#[derive(Clone)]
pub struct NeuronPrompterMcp {
    db: Arc<Database>,
    user_id: i64,
    /// Dispatch table generated by the `#[tool_router]` procedural macro.
    /// Appears unused to the compiler because it is accessed through
    /// macro-generated trait implementations.
    #[allow(dead_code)]
    tool_router: ToolRouter<Self>,
}

impl std::fmt::Debug for NeuronPrompterMcp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NeuronPrompterMcp")
            .field("user_id", &self.user_id)
            .finish_non_exhaustive()
    }
}

impl NeuronPrompterMcp {
    /// Creates a tool container bound to the given database and `mcp_agent` user.
    /// Initializes the tool router dispatch table via the generated method.
    pub fn new(db: Arc<Database>, user_id: i64) -> Self {
        Self {
            db,
            user_id,
            tool_router: Self::tool_router(),
        }
    }
}

// ---------------------------------------------------------------------------
// Tool implementations using rmcp's #[tool_router] macro.
//
// Each tool corresponds to a prompt management operation available to
// external AI clients via the MCP protocol. Tools return serialized JSON
// as text content on success, or an rmcp ErrorData on failure.
//
// Access boundary enforcement:
//   - No delete tools are registered (prompts, tags, categories, collections).
//   - No Ollama tools are registered (improve, translate, derive).
// ---------------------------------------------------------------------------

#[tool_router]
impl NeuronPrompterMcp {
    /// Lists all prompts owned by the `mcp_agent` user.
    #[tool(description = "List all prompts in the local prompt library")]
    fn mcp_list_prompts(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        let filter = PromptFilter {
            user_id: Some(self.user_id),
            ..Default::default()
        };
        let paginated =
            prompt_service::list_prompts(&self.db, &filter).map_err(service_error_to_mcp)?;
        let json = serde_json::to_string_pretty(&paginated.items)
            .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    /// Searches prompts by text query using FTS5 full-text search.
    #[tool(description = "Search prompts by text query")]
    fn mcp_search_prompts(
        &self,
        Parameters(SearchParams { query }): Parameters<SearchParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let filter = PromptFilter {
            user_id: Some(self.user_id),
            ..Default::default()
        };
        let results = search_service::search_prompts(&self.db, self.user_id, &query, &filter)
            .map_err(service_error_to_mcp)?;
        let json = serde_json::to_string_pretty(&results)
            .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    /// Retrieves a prompt by ID with its tag, category, and collection associations.
    #[tool(description = "Get a prompt by ID with its associations")]
    fn mcp_get_prompt(
        &self,
        Parameters(GetPromptParams { prompt_id }): Parameters<GetPromptParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let pwa = prompt_service::get_prompt(&self.db, prompt_id).map_err(service_error_to_mcp)?;
        if pwa.prompt.user_id != self.user_id {
            return Err(rmcp::ErrorData::invalid_params(
                "entity not owned by current user",
                None,
            ));
        }
        let json = serde_json::to_string_pretty(&pwa)
            .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    /// Creates a prompt under the `mcp_agent` user.
    #[tool(description = "Create a prompt in the local library")]
    fn mcp_create_prompt(
        &self,
        Parameters(CreatePromptParams {
            title,
            content,
            description,
            notes,
            language,
            tag_ids,
            category_ids,
            collection_ids,
        }): Parameters<CreatePromptParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let new = NewPrompt {
            user_id: self.user_id,
            title,
            content,
            description,
            notes,
            language,
            tag_ids: tag_ids.unwrap_or_default(),
            category_ids: category_ids.unwrap_or_default(),
            collection_ids: collection_ids.unwrap_or_default(),
        };
        let prompt = prompt_service::create_prompt(&self.db, &new).map_err(service_error_to_mcp)?;
        let json = serde_json::to_string_pretty(&prompt)
            .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    /// Updates a prompt's title and/or content.
    #[tool(description = "Update an existing prompt's title or content")]
    fn mcp_update_prompt(
        &self,
        Parameters(UpdatePromptParams {
            prompt_id,
            title,
            content,
            description,
            notes,
            language,
            tag_ids,
            category_ids,
            collection_ids,
        }): Parameters<UpdatePromptParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        // Verify ownership before updating.
        let existing =
            prompt_service::get_prompt(&self.db, prompt_id).map_err(service_error_to_mcp)?;
        if existing.prompt.user_id != self.user_id {
            return Err(rmcp::ErrorData::invalid_params(
                "entity not owned by current user",
                None,
            ));
        }
        let update = UpdatePrompt {
            prompt_id,
            title,
            content,
            description,
            notes,
            language,
            tag_ids,
            category_ids,
            collection_ids,
            expected_version: None,
        };
        let prompt =
            prompt_service::update_prompt(&self.db, &update).map_err(service_error_to_mcp)?;
        let json = serde_json::to_string_pretty(&prompt)
            .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    /// Lists all tags owned by the `mcp_agent` user.
    #[tool(description = "List all tags")]
    fn mcp_list_tags(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        let tags = tag_service::list_tags(&self.db, self.user_id).map_err(service_error_to_mcp)?;
        let json = serde_json::to_string_pretty(&tags)
            .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    /// Creates a tag under the `mcp_agent` user.
    #[tool(description = "Create a tag")]
    fn mcp_create_tag(
        &self,
        Parameters(CreateTagParams { name }): Parameters<CreateTagParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let tag =
            tag_service::create_tag(&self.db, self.user_id, &name).map_err(service_error_to_mcp)?;
        let json = serde_json::to_string_pretty(&tag)
            .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    /// Lists all categories owned by the `mcp_agent` user.
    #[tool(description = "List all categories")]
    fn mcp_list_categories(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        let cats = category_service::list_categories(&self.db, self.user_id)
            .map_err(service_error_to_mcp)?;
        let json = serde_json::to_string_pretty(&cats)
            .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    /// Creates a category under the `mcp_agent` user.
    #[tool(description = "Create a category")]
    fn mcp_create_category(
        &self,
        Parameters(CreateCategoryParams { name }): Parameters<CreateCategoryParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let cat = category_service::create_category(&self.db, self.user_id, &name)
            .map_err(service_error_to_mcp)?;
        let json = serde_json::to_string_pretty(&cat)
            .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    /// Lists all collections owned by the `mcp_agent` user.
    #[tool(description = "List all collections")]
    fn mcp_list_collections(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        let cols = collection_service::list_collections(&self.db, self.user_id)
            .map_err(service_error_to_mcp)?;
        let json = serde_json::to_string_pretty(&cols)
            .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    // -----------------------------------------------------------------------
    // Chain tools
    // -----------------------------------------------------------------------

    /// Lists all chains owned by the `mcp_agent` user.
    #[tool(
        description = "List all prompt chains. Chains sequence multiple prompts together with a configurable separator."
    )]
    fn mcp_list_chains(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        let filter = ChainFilter {
            user_id: Some(self.user_id),
            ..ChainFilter::default()
        };
        let paginated =
            chain_service::list_chains(&self.db, &filter).map_err(service_error_to_mcp)?;
        let json = serde_json::to_string_pretty(&paginated.items)
            .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    /// Searches chains by text query using FTS5 full-text search on title and description.
    #[tool(description = "Search chains by text query (searches title, description, notes)")]
    fn mcp_search_chains(
        &self,
        Parameters(SearchChainsParams { query }): Parameters<SearchChainsParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let filter = ChainFilter::default();
        let results = search_service::search_chains(&self.db, self.user_id, &query, &filter)
            .map_err(service_error_to_mcp)?;
        let json = serde_json::to_string_pretty(&results)
            .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    /// Retrieves a chain by ID with its resolved steps (including full prompt data)
    /// and taxonomy associations (tags, categories, collections).
    #[tool(
        description = "Get a chain by ID with its resolved steps and associations. Each step includes the full prompt data."
    )]
    fn mcp_get_chain(
        &self,
        Parameters(GetChainParams { chain_id }): Parameters<GetChainParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let detail = chain_service::get_chain(&self.db, chain_id).map_err(service_error_to_mcp)?;
        if detail.chain.user_id != self.user_id {
            return Err(rmcp::ErrorData::invalid_params(
                "entity not owned by current user",
                None,
            ));
        }
        let json = serde_json::to_string_pretty(&detail)
            .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    /// Returns the composed content of a chain -- all prompt contents joined by the
    /// chain's separator. This is the final output text that results from the chain.
    #[tool(
        description = "Get the composed content of a chain (all prompt contents joined by the separator). Returns the final concatenated text."
    )]
    fn mcp_get_chain_content(
        &self,
        Parameters(GetChainContentParams { chain_id }): Parameters<GetChainContentParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        // Verify ownership before returning content.
        let detail = chain_service::get_chain(&self.db, chain_id).map_err(service_error_to_mcp)?;
        if detail.chain.user_id != self.user_id {
            return Err(rmcp::ErrorData::invalid_params(
                "entity not owned by current user",
                None,
            ));
        }
        let content = chain_service::get_composed_content(&self.db, chain_id)
            .map_err(service_error_to_mcp)?;
        let result = serde_json::json!({ "content": content });
        let json = serde_json::to_string_pretty(&result)
            .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    /// Creates a chain with an ordered sequence of prompt steps and optional
    /// taxonomy associations.
    #[tool(
        description = "Create a prompt chain. Requires a title and an ordered list of prompt IDs. Optionally set a separator, description, and tag/category/collection IDs."
    )]
    fn mcp_create_chain(
        &self,
        Parameters(CreateChainParams {
            title,
            prompt_ids,
            separator,
            description,
            tag_ids,
            category_ids,
            collection_ids,
        }): Parameters<CreateChainParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        // Validate that every prompt_id belongs to the current user.
        for &pid in &prompt_ids {
            let pwa = prompt_service::get_prompt(&self.db, pid).map_err(service_error_to_mcp)?;
            if pwa.prompt.user_id != self.user_id {
                return Err(rmcp::ErrorData::invalid_params(
                    format!("prompt {pid} is not owned by current user"),
                    None,
                ));
            }
        }
        let new = NewChain {
            user_id: self.user_id,
            title,
            description,
            notes: None,
            language: None,
            separator,
            prompt_ids,
            steps: Vec::new(),
            tag_ids: tag_ids.unwrap_or_default(),
            category_ids: category_ids.unwrap_or_default(),
            collection_ids: collection_ids.unwrap_or_default(),
        };
        let chain = chain_service::create_chain(&self.db, &new).map_err(service_error_to_mcp)?;
        let json = serde_json::to_string_pretty(&chain)
            .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    /// Updates a chain's metadata, steps, and/or taxonomy associations.
    /// Only provided fields are modified. When `prompt_ids` is provided,
    /// it replaces the entire step sequence.
    #[tool(
        description = "Update a chain's title, description, separator, steps (prompt_ids), or taxonomy. Only provided fields are changed. prompt_ids replaces the entire step sequence."
    )]
    fn mcp_update_chain(
        &self,
        Parameters(UpdateChainParams {
            chain_id,
            title,
            description,
            separator,
            prompt_ids,
            tag_ids,
            category_ids,
            collection_ids,
        }): Parameters<UpdateChainParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        // Verify ownership before updating.
        let existing =
            chain_service::get_chain(&self.db, chain_id).map_err(service_error_to_mcp)?;
        if existing.chain.user_id != self.user_id {
            return Err(rmcp::ErrorData::invalid_params(
                "entity not owned by current user",
                None,
            ));
        }
        // Validate that every new prompt_id belongs to the current user.
        if let Some(ref pids) = prompt_ids {
            for &pid in pids {
                let pwa =
                    prompt_service::get_prompt(&self.db, pid).map_err(service_error_to_mcp)?;
                if pwa.prompt.user_id != self.user_id {
                    return Err(rmcp::ErrorData::invalid_params(
                        format!("prompt {pid} is not owned by current user"),
                        None,
                    ));
                }
            }
        }
        let update = UpdateChain {
            chain_id,
            title,
            description,
            notes: None,
            language: None,
            separator,
            prompt_ids,
            steps: None,
            tag_ids,
            category_ids,
            collection_ids,
        };
        let chain = chain_service::update_chain(&self.db, &update).map_err(service_error_to_mcp)?;
        let json = serde_json::to_string_pretty(&chain)
            .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    /// Finds all chains that reference a given prompt. Useful for checking
    /// which chains would be affected before modifying a prompt.
    #[tool(
        description = "Find all chains that reference a specific prompt. Useful before editing or understanding prompt usage."
    )]
    fn mcp_chains_for_prompt(
        &self,
        Parameters(ChainsForPromptParams { prompt_id }): Parameters<ChainsForPromptParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        // Verify ownership of the prompt being queried.
        // If the prompt doesn't exist, return an empty list (consistent with REST).
        match prompt_service::get_prompt(&self.db, prompt_id) {
            Ok(pwa) => {
                if pwa.prompt.user_id != self.user_id {
                    return Err(rmcp::ErrorData::invalid_params(
                        "entity not owned by current user",
                        None,
                    ));
                }
            }
            Err(_) => {
                return Ok(CallToolResult::success(vec![Content::text("[]")]));
            }
        }
        let chains: Vec<_> = chain_service::get_chains_for_prompt(&self.db, prompt_id)
            .map_err(service_error_to_mcp)?
            .into_iter()
            .filter(|c| c.user_id == self.user_id)
            .collect();
        let json = serde_json::to_string_pretty(&chains)
            .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    /// Duplicates a chain including its steps and taxonomy associations.
    /// The copy gets "(copy)" appended to the title.
    #[tool(
        description = "Duplicate a chain with all its steps and associations. The copy title gets '(copy)' appended."
    )]
    fn mcp_duplicate_chain(
        &self,
        Parameters(DuplicateChainParams { chain_id }): Parameters<DuplicateChainParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        // Verify ownership before duplicating.
        let existing =
            chain_service::get_chain(&self.db, chain_id).map_err(service_error_to_mcp)?;
        if existing.chain.user_id != self.user_id {
            return Err(rmcp::ErrorData::invalid_params(
                "entity not owned by current user",
                None,
            ));
        }
        let chain =
            chain_service::duplicate_chain(&self.db, chain_id).map_err(service_error_to_mcp)?;
        let json = serde_json::to_string_pretty(&chain)
            .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    // -----------------------------------------------------------------------
    // Script tools
    // -----------------------------------------------------------------------

    /// Lists all scripts owned by the `mcp_agent` user.
    #[tool(description = "List all scripts in the local script library")]
    fn mcp_list_scripts(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        let filter = ScriptFilter {
            user_id: Some(self.user_id),
            ..ScriptFilter::default()
        };
        let paginated =
            script_service::list_scripts(&self.db, &filter).map_err(service_error_to_mcp)?;
        let json = serde_json::to_string_pretty(&paginated.items)
            .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    /// Searches scripts by text query using FTS5 full-text search.
    #[tool(description = "Search scripts by text query")]
    fn mcp_search_scripts(
        &self,
        Parameters(SearchScriptsParams { query }): Parameters<SearchScriptsParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let filter = ScriptFilter {
            user_id: Some(self.user_id),
            ..ScriptFilter::default()
        };
        let results = search_service::search_scripts(&self.db, self.user_id, &query, &filter)
            .map_err(service_error_to_mcp)?;
        let json = serde_json::to_string_pretty(&results)
            .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    /// Retrieves a script by ID with its tag, category, and collection associations.
    #[tool(description = "Get a script by ID with its associations")]
    fn mcp_get_script(
        &self,
        Parameters(GetScriptParams { script_id }): Parameters<GetScriptParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let swa = script_service::get_script(&self.db, script_id).map_err(service_error_to_mcp)?;
        if swa.script.user_id != self.user_id {
            return Err(rmcp::ErrorData::invalid_params(
                "entity not owned by current user",
                None,
            ));
        }
        let json = serde_json::to_string_pretty(&swa)
            .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    /// Creates a script under the `mcp_agent` user.
    #[tool(description = "Create a script in the local library")]
    fn mcp_create_script(
        &self,
        Parameters(CreateScriptParams {
            title,
            content,
            script_language,
            description,
            notes,
            language,
            tag_ids,
            category_ids,
            collection_ids,
        }): Parameters<CreateScriptParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let new = NewScript {
            user_id: self.user_id,
            title,
            content,
            script_language,
            description,
            notes,
            language,
            source_path: None,
            is_synced: false,
            tag_ids: tag_ids.unwrap_or_default(),
            category_ids: category_ids.unwrap_or_default(),
            collection_ids: collection_ids.unwrap_or_default(),
        };
        let script = script_service::create_script(&self.db, &new).map_err(service_error_to_mcp)?;
        let json = serde_json::to_string_pretty(&script)
            .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    /// Updates a script's title, content, and/or programming language.
    #[tool(description = "Update an existing script's title, content, or language")]
    fn mcp_update_script(
        &self,
        Parameters(UpdateScriptParams {
            script_id,
            title,
            content,
            script_language,
            description,
            notes,
            language,
            tag_ids,
            category_ids,
            collection_ids,
        }): Parameters<UpdateScriptParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        // Verify ownership before updating.
        let existing =
            script_service::get_script(&self.db, script_id).map_err(service_error_to_mcp)?;
        if existing.script.user_id != self.user_id {
            return Err(rmcp::ErrorData::invalid_params(
                "entity not owned by current user",
                None,
            ));
        }
        let update = UpdateScript {
            script_id,
            title,
            content,
            description,
            notes,
            script_language,
            language,
            source_path: None,
            is_synced: None,
            tag_ids,
            category_ids,
            collection_ids,
            expected_version: None,
        };
        let script =
            script_service::update_script(&self.db, &update).map_err(service_error_to_mcp)?;
        let json = serde_json::to_string_pretty(&script)
            .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }
}

// ---------------------------------------------------------------------------
// MCP ServerHandler implementation for protocol metadata.
// ---------------------------------------------------------------------------

#[tool_handler]
impl ServerHandler for NeuronPrompterMcp {
    /// Returns MCP server metadata including name, version, and capabilities.
    /// Advertises tool support as the only capability.
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            server_info: Implementation {
                name: "neuronprompter-mcp".into(),
                version: env!("CARGO_PKG_VERSION").into(),
            },
            instructions: Some(
                "NeuronPrompter MCP server providing prompt, script, and chain management tools. \
                 Supports creating, reading, updating, and searching prompts, scripts, and prompt chains, \
                 as well as managing tags, categories, and collections in a local prompt library. \
                 Scripts store code files of any programming language. \
                 Chains allow sequencing multiple prompts and scripts together with configurable separators."
                    .into(),
            ),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}
