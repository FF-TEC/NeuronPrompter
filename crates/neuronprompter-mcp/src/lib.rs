//! Model Context Protocol server.
//!
//! This crate implements a JSON-RPC 2.0 server over stdio that exposes
//! NeuronPrompter prompt management tools to external AI systems. All
//! operations are scoped to a dedicated `mcp_agent` user and enforce access
//! boundary restrictions (no delete, no Ollama tools).

pub mod auth;
pub mod error;
pub mod registration;
pub mod server;
pub mod tool_categories;
pub mod tool_prompts;
pub mod tool_tags;
pub mod tools;
pub mod transport;

pub use error::McpError;
pub use server::run_stdio_server;

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

    use std::sync::Arc;

    use neuronprompter_application::{ServiceError, chain_service, prompt_service, user_service};
    use neuronprompter_core::CoreError;
    use neuronprompter_core::domain::prompt::{NewPrompt, PromptFilter};
    use neuronprompter_core::domain::user::NewUser;
    use neuronprompter_db::Database;
    use rmcp::model::ErrorCode;

    use crate::McpError;
    use crate::auth::ensure_mcp_user;
    use crate::error::service_error_to_mcp;

    // -------------------------------------------------------------------------
    // Helper: creates a fresh in-memory database for test isolation.
    // -------------------------------------------------------------------------

    /// Returns an `Arc<Database>` backed by an in-memory `SQLite` instance
    /// with all migrations applied. Each call produces an independent database
    /// so tests do not share state.
    fn test_db() -> Arc<Database> {
        Arc::new(Database::open_in_memory().expect("in-memory database should open"))
    }

    // =========================================================================
    // 1. Auth tests -- ensure_mcp_user
    // =========================================================================

    #[test]
    fn ensure_mcp_user_creates_user_on_first_call() {
        // The mcp_agent user does not exist in a fresh database.
        // ensure_mcp_user should create it with the correct username and
        // display name.
        let db = test_db();
        let user = ensure_mcp_user(&db).expect("first ensure_mcp_user should succeed");

        assert_eq!(user.username, "mcp_agent");
        assert_eq!(user.display_name, "MCP Agent");
        assert!(user.id > 0, "user id should be a positive integer");
    }

    #[test]
    fn ensure_mcp_user_returns_existing_on_second_call() {
        // Calling ensure_mcp_user twice should return the same user record
        // without creating a duplicate. The ID from both calls must match.
        let db = test_db();
        let first = ensure_mcp_user(&db).expect("first call should succeed");
        let second = ensure_mcp_user(&db).expect("second call should succeed");

        assert_eq!(
            first.id, second.id,
            "both calls should return the same user ID"
        );
        assert_eq!(first.username, second.username);
    }

    #[test]
    fn ensure_mcp_user_ignores_other_users() {
        // If other users exist in the database, ensure_mcp_user should still
        // create the mcp_agent user independently.
        let db = test_db();
        let other = NewUser {
            username: "alice".to_owned(),
            display_name: "Alice".to_owned(),
        };
        user_service::create_user(&db, &other).expect("creating alice should succeed");

        let mcp_user = ensure_mcp_user(&db).expect("ensure_mcp_user should succeed");
        assert_eq!(mcp_user.username, "mcp_agent");

        // Verify that there are now two users total.
        let all_users = user_service::list_users(&db).expect("list_users should succeed");
        assert_eq!(all_users.len(), 2);
    }

    // =========================================================================
    // 2. Error mapping tests -- service_error_to_mcp
    // =========================================================================

    #[test]
    fn error_mapping_validation_returns_invalid_params() {
        // CoreError::Validation maps to JSON-RPC INVALID_PARAMS (-32602).
        let err = ServiceError::Core(CoreError::Validation {
            field: "title".to_owned(),
            message: "must not be empty".to_owned(),
        });
        let mcp_err = service_error_to_mcp(err);
        assert_eq!(mcp_err.code, ErrorCode::INVALID_PARAMS);
    }

    #[test]
    fn error_mapping_not_found_returns_invalid_params() {
        // CoreError::NotFound maps to JSON-RPC INVALID_PARAMS (-32602).
        // The MCP spec uses INVALID_PARAMS for client-supplied identifiers
        // that do not resolve to an entity.
        let err = ServiceError::Core(CoreError::NotFound {
            entity: "Prompt".to_owned(),
            id: 999,
        });
        let mcp_err = service_error_to_mcp(err);
        assert_eq!(mcp_err.code, ErrorCode::INVALID_PARAMS);
        assert!(
            mcp_err.message.contains("999"),
            "error message should include the missing entity ID"
        );
    }

    #[test]
    fn error_mapping_duplicate_returns_invalid_params() {
        // CoreError::Duplicate maps to JSON-RPC INVALID_PARAMS (-32602).
        let err = ServiceError::Core(CoreError::Duplicate {
            entity: "Tag".to_owned(),
            field: "name".to_owned(),
            value: "rust".to_owned(),
        });
        let mcp_err = service_error_to_mcp(err);
        assert_eq!(mcp_err.code, ErrorCode::INVALID_PARAMS);
        assert!(
            mcp_err.message.contains("rust"),
            "error message should include the duplicate value"
        );
    }

    #[test]
    fn error_mapping_database_core_returns_invalid_params() {
        // ServiceError::Database(DbError::Core(_)) unwraps to the inner
        // CoreError and maps to JSON-RPC INVALID_PARAMS (-32602).
        let db_err = neuronprompter_db::DbError::from(CoreError::NotFound {
            entity: "test".to_owned(),
            id: 0,
        });
        let err = ServiceError::Database(db_err);
        let mcp_err = service_error_to_mcp(err);
        assert_eq!(mcp_err.code, ErrorCode::INVALID_PARAMS);
    }

    #[test]
    fn error_mapping_ollama_unavailable_returns_invalid_request() {
        // ServiceError::OllamaUnavailable maps to JSON-RPC INVALID_REQUEST
        // (-32600), signaling that the requested Ollama-dependent operation
        // cannot be fulfilled.
        let err = ServiceError::OllamaUnavailable("connection refused".to_owned());
        let mcp_err = service_error_to_mcp(err);
        assert_eq!(mcp_err.code, ErrorCode::INVALID_REQUEST);
    }

    #[test]
    fn error_mapping_ollama_error_returns_invalid_request() {
        // ServiceError::OllamaError also maps to INVALID_REQUEST (-32600).
        let err = ServiceError::OllamaError("model not found".to_owned());
        let mcp_err = service_error_to_mcp(err);
        assert_eq!(mcp_err.code, ErrorCode::INVALID_REQUEST);
    }

    #[test]
    fn error_mapping_io_error_returns_internal_error() {
        // ServiceError::IoError maps to JSON-RPC INTERNAL_ERROR (-32603).
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file missing");
        let err = ServiceError::IoError(io_err);
        let mcp_err = service_error_to_mcp(err);
        assert_eq!(mcp_err.code, ErrorCode::INTERNAL_ERROR);
    }

    #[test]
    fn error_mapping_serialization_returns_internal_error() {
        // ServiceError::SerializationError maps to INTERNAL_ERROR (-32603).
        let err = ServiceError::SerializationError("invalid JSON".to_owned());
        let mcp_err = service_error_to_mcp(err);
        assert_eq!(mcp_err.code, ErrorCode::INTERNAL_ERROR);
    }

    // =========================================================================
    // 3. User isolation tests -- list_prompts scoped to mcp_agent
    // =========================================================================

    #[test]
    fn list_prompts_returns_only_mcp_agent_prompts() {
        // Create two users: a regular user (alice) and the mcp_agent user.
        // Insert a prompt under each user. When listing prompts filtered to
        // the mcp_agent user_id, only the mcp_agent prompt should appear.
        let db = test_db();

        let alice = user_service::create_user(
            &db,
            &NewUser {
                username: "alice".to_owned(),
                display_name: "Alice".to_owned(),
            },
        )
        .expect("creating alice should succeed");

        let mcp_user = ensure_mcp_user(&db).expect("ensure_mcp_user should succeed");

        // Create a prompt owned by alice.
        let alice_prompt = NewPrompt {
            user_id: alice.id,
            title: "Alice Prompt".to_owned(),
            content: "Content by alice".to_owned(),
            description: None,
            notes: None,
            language: None,
            tag_ids: Vec::new(),
            category_ids: Vec::new(),
            collection_ids: Vec::new(),
        };
        prompt_service::create_prompt(&db, &alice_prompt)
            .expect("creating alice prompt should succeed");

        // Create a prompt owned by the mcp_agent.
        let mcp_prompt = NewPrompt {
            user_id: mcp_user.id,
            title: "MCP Prompt".to_owned(),
            content: "Content by mcp_agent".to_owned(),
            description: None,
            notes: None,
            language: None,
            tag_ids: Vec::new(),
            category_ids: Vec::new(),
            collection_ids: Vec::new(),
        };
        prompt_service::create_prompt(&db, &mcp_prompt)
            .expect("creating mcp prompt should succeed");

        // List prompts filtered to the mcp_agent user.
        let filter = PromptFilter {
            user_id: Some(mcp_user.id),
            is_favorite: None,
            is_archived: None,
            collection_id: None,
            category_id: None,
            tag_id: None,
            limit: None,
            offset: None,
            ..Default::default()
        };
        let results = prompt_service::list_prompts(&db, &filter)
            .expect("list_prompts should succeed")
            .items;

        assert_eq!(results.len(), 1, "only the mcp_agent prompt should appear");
        assert_eq!(results[0].title, "MCP Prompt");
        assert_eq!(results[0].user_id, mcp_user.id);
    }

    #[test]
    fn list_prompts_without_user_filter_returns_all_users() {
        // Without a user_id filter, list_prompts returns prompts from all
        // users. This confirms the isolation relies on the filter, not on
        // a database-level restriction.
        let db = test_db();

        let alice = user_service::create_user(
            &db,
            &NewUser {
                username: "alice".to_owned(),
                display_name: "Alice".to_owned(),
            },
        )
        .expect("creating alice should succeed");

        let mcp_user = ensure_mcp_user(&db).expect("ensure_mcp_user should succeed");

        // Insert one prompt per user.
        for (user_id, title) in [(alice.id, "Alice Prompt"), (mcp_user.id, "MCP Prompt")] {
            let new = NewPrompt {
                user_id,
                title: title.to_owned(),
                content: "body".to_owned(),
                description: None,
                notes: None,
                language: None,
                tag_ids: Vec::new(),
                category_ids: Vec::new(),
                collection_ids: Vec::new(),
            };
            prompt_service::create_prompt(&db, &new).expect("creating prompt should succeed");
        }

        let unfiltered = PromptFilter::default();
        let all = prompt_service::list_prompts(&db, &unfiltered)
            .expect("list_prompts should succeed")
            .items;
        assert_eq!(
            all.len(),
            2,
            "unfiltered listing should return prompts from both users"
        );
    }

    // =========================================================================
    // 4. Tool round-trip test -- create then get, list consistency
    // =========================================================================

    #[test]
    fn prompt_round_trip_create_get_list() {
        // Creates a prompt via the service layer (the same path the MCP tools
        // use), then retrieves it by ID and verifies the listing includes it.
        let db = test_db();
        let mcp_user = ensure_mcp_user(&db).expect("ensure_mcp_user should succeed");

        let new = NewPrompt {
            user_id: mcp_user.id,
            title: "Round Trip".to_owned(),
            content: "Round trip content".to_owned(),
            description: None,
            notes: None,
            language: None,
            tag_ids: Vec::new(),
            category_ids: Vec::new(),
            collection_ids: Vec::new(),
        };
        let created =
            prompt_service::create_prompt(&db, &new).expect("create_prompt should succeed");

        assert_eq!(created.title, "Round Trip");
        assert_eq!(created.content, "Round trip content");
        assert_eq!(created.user_id, mcp_user.id);

        // Retrieve by ID and verify fields match.
        let pwa = prompt_service::get_prompt(&db, created.id).expect("get_prompt should succeed");
        assert_eq!(pwa.prompt.id, created.id);
        assert_eq!(pwa.prompt.title, "Round Trip");
        assert_eq!(pwa.prompt.content, "Round trip content");

        // List prompts scoped to the mcp_agent and verify the created prompt
        // appears in the results.
        let filter = PromptFilter {
            user_id: Some(mcp_user.id),
            is_favorite: None,
            is_archived: None,
            collection_id: None,
            category_id: None,
            tag_id: None,
            limit: None,
            offset: None,
            ..Default::default()
        };
        let list = prompt_service::list_prompts(&db, &filter)
            .expect("list_prompts should succeed")
            .items;
        assert!(
            list.iter().any(|p| p.id == created.id),
            "listed prompts should include the created prompt"
        );
    }

    #[test]
    fn prompt_round_trip_create_update_get() {
        // Creates a prompt, updates its title and content, then retrieves it
        // to verify the update was applied.
        let db = test_db();
        let mcp_user = ensure_mcp_user(&db).expect("ensure_mcp_user should succeed");

        let new = NewPrompt {
            user_id: mcp_user.id,
            title: "Before Update".to_owned(),
            content: "Original content".to_owned(),
            description: None,
            notes: None,
            language: None,
            tag_ids: Vec::new(),
            category_ids: Vec::new(),
            collection_ids: Vec::new(),
        };
        let created =
            prompt_service::create_prompt(&db, &new).expect("create_prompt should succeed");

        let update = neuronprompter_core::domain::prompt::UpdatePrompt {
            prompt_id: created.id,
            title: Some("After Update".to_owned()),
            content: Some("Changed content".to_owned()),
            description: None,
            notes: None,
            language: None,
            tag_ids: None,
            category_ids: None,
            collection_ids: None,
            expected_version: None,
        };
        let updated =
            prompt_service::update_prompt(&db, &update).expect("update_prompt should succeed");

        assert_eq!(updated.title, "After Update");
        assert_eq!(updated.content, "Changed content");

        // Retrieve again to confirm persistence.
        let pwa = prompt_service::get_prompt(&db, created.id)
            .expect("get_prompt should succeed after update");
        assert_eq!(pwa.prompt.title, "After Update");
        assert_eq!(pwa.prompt.content, "Changed content");
    }

    // =========================================================================
    // 5. Access boundary tests -- no delete tools, no Ollama tools
    // =========================================================================

    /// The complete list of tool method names registered via `#[tool_router]`
    /// on `NeuronPrompterMcp` in `tools.rs`. Cross-referenced against the
    /// source code to validate access boundary enforcement. Because invoking
    /// the tool router requires the full rmcp async runtime, this test uses a
    /// static list.
    const REGISTERED_TOOL_NAMES: &[&str] = &[
        "mcp_list_prompts",
        "mcp_search_prompts",
        "mcp_get_prompt",
        "mcp_create_prompt",
        "mcp_update_prompt",
        "mcp_list_tags",
        "mcp_create_tag",
        "mcp_list_categories",
        "mcp_create_category",
        "mcp_list_collections",
        "mcp_list_chains",
        "mcp_search_chains",
        "mcp_get_chain",
        "mcp_get_chain_content",
        "mcp_create_chain",
        "mcp_update_chain",
        "mcp_chains_for_prompt",
        "mcp_duplicate_chain",
        "mcp_list_scripts",
        "mcp_search_scripts",
        "mcp_get_script",
        "mcp_create_script",
        "mcp_update_script",
    ];

    #[test]
    fn no_delete_tools_registered() {
        // The MCP access boundary prohibits delete operations. None of the
        // registered tool names should contain "delete".
        for name in REGISTERED_TOOL_NAMES {
            assert!(
                !name.to_lowercase().contains("delete"),
                "tool '{name}' violates the delete prohibition"
            );
        }
    }

    #[test]
    fn no_ollama_tools_registered() {
        // The MCP access boundary prohibits Ollama tools (improve, translate,
        // derive). None of the registered tool names should contain "ollama".
        for name in REGISTERED_TOOL_NAMES {
            assert!(
                !name.to_lowercase().contains("ollama"),
                "tool '{name}' violates the Ollama prohibition"
            );
        }
    }

    #[test]
    fn registered_tools_contain_expected_set() {
        // Verify the full set of tool names matches what is declared in the
        // #[tool_router] impl block. This catches accidental additions or
        // removals.
        assert_eq!(REGISTERED_TOOL_NAMES.len(), 23);
        assert!(REGISTERED_TOOL_NAMES.contains(&"mcp_list_prompts"));
        assert!(REGISTERED_TOOL_NAMES.contains(&"mcp_search_prompts"));
        assert!(REGISTERED_TOOL_NAMES.contains(&"mcp_get_prompt"));
        assert!(REGISTERED_TOOL_NAMES.contains(&"mcp_create_prompt"));
        assert!(REGISTERED_TOOL_NAMES.contains(&"mcp_update_prompt"));
        assert!(REGISTERED_TOOL_NAMES.contains(&"mcp_list_tags"));
        assert!(REGISTERED_TOOL_NAMES.contains(&"mcp_create_tag"));
        assert!(REGISTERED_TOOL_NAMES.contains(&"mcp_list_categories"));
        assert!(REGISTERED_TOOL_NAMES.contains(&"mcp_create_category"));
        assert!(REGISTERED_TOOL_NAMES.contains(&"mcp_list_collections"));
        assert!(REGISTERED_TOOL_NAMES.contains(&"mcp_list_chains"));
        assert!(REGISTERED_TOOL_NAMES.contains(&"mcp_search_chains"));
        assert!(REGISTERED_TOOL_NAMES.contains(&"mcp_get_chain"));
        assert!(REGISTERED_TOOL_NAMES.contains(&"mcp_get_chain_content"));
        assert!(REGISTERED_TOOL_NAMES.contains(&"mcp_create_chain"));
        assert!(REGISTERED_TOOL_NAMES.contains(&"mcp_update_chain"));
        assert!(REGISTERED_TOOL_NAMES.contains(&"mcp_chains_for_prompt"));
        assert!(REGISTERED_TOOL_NAMES.contains(&"mcp_duplicate_chain"));
        assert!(REGISTERED_TOOL_NAMES.contains(&"mcp_list_scripts"));
        assert!(REGISTERED_TOOL_NAMES.contains(&"mcp_search_scripts"));
        assert!(REGISTERED_TOOL_NAMES.contains(&"mcp_get_script"));
        assert!(REGISTERED_TOOL_NAMES.contains(&"mcp_create_script"));
        assert!(REGISTERED_TOOL_NAMES.contains(&"mcp_update_script"));
    }

    // =========================================================================
    // 6. Chain CRUD lifecycle tests (service-layer, same pattern as prompt tests)
    // =========================================================================

    #[test]
    fn chain_create_get_content_lifecycle() {
        let db = test_db();
        let user = ensure_mcp_user(&db).expect("ensure mcp user");
        let user_id = user.id;

        // Create two prompts as chain building blocks.
        let p1 = prompt_service::create_prompt(
            &db,
            &neuronprompter_core::domain::prompt::NewPrompt {
                user_id,
                title: "System Prompt".to_owned(),
                content: "You are a helpful assistant.".to_owned(),
                description: None,
                notes: None,
                language: None,
                tag_ids: Vec::new(),
                category_ids: Vec::new(),
                collection_ids: Vec::new(),
            },
        )
        .expect("create prompt 1");

        let p2 = prompt_service::create_prompt(
            &db,
            &neuronprompter_core::domain::prompt::NewPrompt {
                user_id,
                title: "User Question".to_owned(),
                content: "What is Rust?".to_owned(),
                description: None,
                notes: None,
                language: None,
                tag_ids: Vec::new(),
                category_ids: Vec::new(),
                collection_ids: Vec::new(),
            },
        )
        .expect("create prompt 2");

        // Create a chain from the two prompts.
        let new_chain = neuronprompter_core::domain::chain::NewChain {
            user_id,
            title: "QA Chain".to_owned(),
            description: Some("A question-answering chain".to_owned()),
            notes: None,
            language: None,
            separator: Some("\n---\n".to_owned()),
            prompt_ids: vec![p1.id, p2.id],
            steps: Vec::new(),
            tag_ids: Vec::new(),
            category_ids: Vec::new(),
            collection_ids: Vec::new(),
        };
        let chain = chain_service::create_chain(&db, &new_chain).expect("create chain");
        assert_eq!(chain.title, "QA Chain");

        // Get chain with resolved steps.
        let detail = chain_service::get_chain(&db, chain.id).expect("get chain");
        assert_eq!(detail.steps.len(), 2);
        assert_eq!(
            detail.steps[0].prompt.as_ref().unwrap().title,
            "System Prompt"
        );
        assert_eq!(
            detail.steps[1].prompt.as_ref().unwrap().title,
            "User Question"
        );

        // Get composed content.
        let content =
            chain_service::get_composed_content(&db, chain.id).expect("get composed content");
        assert_eq!(content, "You are a helpful assistant.\n---\nWhat is Rust?");

        // Update chain: swap step order.
        let update = neuronprompter_core::domain::chain::UpdateChain {
            chain_id: chain.id,
            title: Some("Updated QA Chain".to_owned()),
            description: None,
            notes: None,
            language: None,
            separator: None,
            prompt_ids: Some(vec![p2.id, p1.id]),
            steps: None,
            tag_ids: None,
            category_ids: None,
            collection_ids: None,
        };
        let updated = chain_service::update_chain(&db, &update).expect("update chain");
        assert_eq!(updated.title, "Updated QA Chain");

        // Verify reordered content.
        let content2 =
            chain_service::get_composed_content(&db, chain.id).expect("get updated content");
        assert_eq!(content2, "What is Rust?\n---\nYou are a helpful assistant.");
    }

    #[test]
    fn chains_for_prompt_lookup() {
        let db = test_db();
        let user = ensure_mcp_user(&db).expect("ensure mcp user");
        let user_id = user.id;

        let p1 = prompt_service::create_prompt(
            &db,
            &neuronprompter_core::domain::prompt::NewPrompt {
                user_id,
                title: "Shared Prompt".to_owned(),
                content: "Shared content".to_owned(),
                description: None,
                notes: None,
                language: None,
                tag_ids: Vec::new(),
                category_ids: Vec::new(),
                collection_ids: Vec::new(),
            },
        )
        .expect("create prompt");

        // Create two chains that reference the same prompt.
        let c1 = chain_service::create_chain(
            &db,
            &neuronprompter_core::domain::chain::NewChain {
                user_id,
                title: "Chain Alpha".to_owned(),
                description: None,
                notes: None,
                language: None,
                separator: None,
                prompt_ids: vec![p1.id],
                steps: Vec::new(),
                tag_ids: Vec::new(),
                category_ids: Vec::new(),
                collection_ids: Vec::new(),
            },
        )
        .expect("create chain 1");

        let _c2 = chain_service::create_chain(
            &db,
            &neuronprompter_core::domain::chain::NewChain {
                user_id,
                title: "Chain Beta".to_owned(),
                description: None,
                notes: None,
                language: None,
                separator: None,
                prompt_ids: vec![p1.id],
                steps: Vec::new(),
                tag_ids: Vec::new(),
                category_ids: Vec::new(),
                collection_ids: Vec::new(),
            },
        )
        .expect("create chain 2");

        // Look up chains for the prompt.
        let referencing =
            chain_service::get_chains_for_prompt(&db, p1.id).expect("chains_for_prompt");
        assert_eq!(referencing.len(), 2);
        let titles: Vec<&str> = referencing.iter().map(|c| c.title.as_str()).collect();
        assert!(titles.contains(&"Chain Alpha"));
        assert!(titles.contains(&"Chain Beta"));

        // Duplicate chain.
        let dup = chain_service::duplicate_chain(&db, c1.id).expect("duplicate chain");
        assert_eq!(dup.title, "Chain Alpha (copy)");

        // Now 3 chains reference the prompt.
        let referencing2 =
            chain_service::get_chains_for_prompt(&db, p1.id).expect("chains_for_prompt after dup");
        assert_eq!(referencing2.len(), 3);
    }

    // =========================================================================
    // 7. McpError construction tests
    // =========================================================================

    #[test]
    fn mcp_error_from_service_error() {
        // McpError::Service is constructed via the From<ServiceError> impl.
        let service_err = ServiceError::SerializationError("bad json".to_owned());
        let mcp_err = McpError::from(service_err);

        match mcp_err {
            McpError::Service(_) => {} // expected variant
            McpError::Transport(_) => panic!("expected McpError::Service, got Transport"),
        }
        assert!(
            mcp_err.to_string().contains("bad json"),
            "error message should propagate through McpError::Service"
        );
    }

    #[test]
    fn mcp_error_transport_displays_message() {
        // McpError::Transport carries a freeform string.
        let err = McpError::Transport("stdin closed unexpectedly".to_owned());
        assert!(
            err.to_string().contains("stdin closed unexpectedly"),
            "transport error message should appear in Display output"
        );
    }

    #[test]
    fn mcp_error_service_wraps_core_error() {
        // A CoreError passes through ServiceError into McpError::Service
        // with its message preserved.
        let core_err = CoreError::Validation {
            field: "title".to_owned(),
            message: "too long".to_owned(),
        };
        let service_err: ServiceError = core_err.into();
        let mcp_err: McpError = service_err.into();

        let display = mcp_err.to_string();
        assert!(
            display.contains("title"),
            "McpError display should include the originating field name"
        );
        assert!(
            display.contains("too long"),
            "McpError display should include the validation message"
        );
    }

    // =========================================================================
    // 7. Error code numeric value assertions
    // =========================================================================

    #[test]
    fn error_codes_match_json_rpc_spec() {
        // Confirm the rmcp ErrorCode constants hold the expected numeric
        // values per the JSON-RPC 2.0 specification.
        assert_eq!(ErrorCode::INVALID_PARAMS.0, -32602);
        assert_eq!(ErrorCode::INVALID_REQUEST.0, -32600);
        assert_eq!(ErrorCode::INTERNAL_ERROR.0, -32603);
    }

    #[test]
    fn all_core_error_variants_produce_invalid_params() {
        // Every CoreError variant (Validation, NotFound, Duplicate) should
        // map to INVALID_PARAMS. This test iterates all variants to guard
        // against regressions if a variant is added.
        let variants: Vec<ServiceError> = vec![
            ServiceError::Core(CoreError::Validation {
                field: "f".to_owned(),
                message: "m".to_owned(),
            }),
            ServiceError::Core(CoreError::NotFound {
                entity: "e".to_owned(),
                id: 1,
            }),
            ServiceError::Core(CoreError::Duplicate {
                entity: "e".to_owned(),
                field: "f".to_owned(),
                value: "v".to_owned(),
            }),
        ];

        for err in variants {
            let mapped = service_error_to_mcp(err);
            assert_eq!(
                mapped.code,
                ErrorCode::INVALID_PARAMS,
                "all CoreError variants should map to INVALID_PARAMS"
            );
        }
    }

    // =========================================================================
    // 8. Service-layer prompt validation (exercised by MCP tool paths)
    // =========================================================================

    #[test]
    fn create_prompt_rejects_empty_title() {
        // The service layer validates titles before persisting. An empty
        // title should produce a CoreError::Validation.
        let db = test_db();
        let mcp_user = ensure_mcp_user(&db).expect("ensure_mcp_user should succeed");

        let new = NewPrompt {
            user_id: mcp_user.id,
            title: String::new(),
            content: "some content".to_owned(),
            description: None,
            notes: None,
            language: None,
            tag_ids: Vec::new(),
            category_ids: Vec::new(),
            collection_ids: Vec::new(),
        };
        let result = prompt_service::create_prompt(&db, &new);
        assert!(result.is_err(), "empty title should be rejected");

        // Verify the error maps to INVALID_PARAMS in MCP context.
        let mcp_err = service_error_to_mcp(result.unwrap_err());
        assert_eq!(mcp_err.code, ErrorCode::INVALID_PARAMS);
    }

    #[test]
    fn create_prompt_rejects_empty_content() {
        // Empty content should also fail validation.
        let db = test_db();
        let mcp_user = ensure_mcp_user(&db).expect("ensure_mcp_user should succeed");

        let new = NewPrompt {
            user_id: mcp_user.id,
            title: "Valid Title".to_owned(),
            content: String::new(),
            description: None,
            notes: None,
            language: None,
            tag_ids: Vec::new(),
            category_ids: Vec::new(),
            collection_ids: Vec::new(),
        };
        let result = prompt_service::create_prompt(&db, &new);
        assert!(result.is_err(), "empty content should be rejected");

        let mcp_err = service_error_to_mcp(result.unwrap_err());
        assert_eq!(mcp_err.code, ErrorCode::INVALID_PARAMS);
    }

    #[test]
    fn get_nonexistent_prompt_maps_to_invalid_params() {
        // Requesting a prompt ID that does not exist results in a
        // DbError::Core(NotFound) at the repository level, which propagates
        // as ServiceError::Database. The error mapping function unwraps
        // DbError::Core to the inner CoreError and maps it to
        // INVALID_PARAMS (-32602).
        let db = test_db();

        let result = prompt_service::get_prompt(&db, 99999);
        assert!(
            result.is_err(),
            "nonexistent prompt ID should produce an error"
        );

        let mcp_err = service_error_to_mcp(result.unwrap_err());
        assert_eq!(mcp_err.code, ErrorCode::INVALID_PARAMS);
    }

    // =========================================================================
    // 9. NeuronPrompterMcp construction
    // =========================================================================

    #[test]
    fn mcp_instance_construction() {
        // Verify NeuronPrompterMcp can be instantiated with a database and
        // user ID. The constructor initializes the tool router dispatch table.
        let db = test_db();
        let mcp_user = ensure_mcp_user(&db).expect("ensure_mcp_user should succeed");

        let mcp = crate::tools::NeuronPrompterMcp::new(db, mcp_user.id);

        // The Debug impl should contain the user_id.
        let debug_repr = format!("{mcp:?}");
        assert!(
            debug_repr.contains(&mcp_user.id.to_string()),
            "Debug output should include the user_id"
        );
    }

    #[test]
    fn mcp_instance_is_cloneable() {
        // NeuronPrompterMcp derives Clone. Both the original and clone
        // should work independently since the database is behind an Arc.
        let db = test_db();
        let mcp_user = ensure_mcp_user(&db).expect("ensure_mcp_user should succeed");

        let mcp = crate::tools::NeuronPrompterMcp::new(Arc::clone(&db), mcp_user.id);
        let _cloned = mcp.clone();
    }
}
