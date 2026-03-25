// =============================================================================
// Integration tests for the API crate.
//
// Tests verify that the router builds correctly, the health endpoint responds,
// and error mapping works as expected.
// =============================================================================

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

#[cfg(test)]
#[allow(clippy::module_inception)]
mod tests {
    use std::sync::Arc;

    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    use crate::error::ApiError;
    use crate::state::{AppState, ClipboardState, OllamaState};

    /// Creates a test `AppState` with an in-memory database pool.
    fn test_app_state() -> Arc<AppState> {
        let pool =
            neuronprompter_db::create_in_memory_pool().expect("in-memory pool should be created");
        let (log_tx, _) = tokio::sync::broadcast::channel(128);
        let (model_tx, _) = tokio::sync::broadcast::channel(64);
        Arc::new(AppState {
            pool,
            ollama: OllamaState::new(),
            clipboard: ClipboardState::new(),
            log_tx,
            model_tx,
            cancellation: tokio_util::sync::CancellationToken::new(),
            session_token: "test-token".to_owned(),
            rate_limiter: std::sync::Arc::new(crate::middleware::rate_limit::RateLimiter::new(
                120, 60,
            )),
            sessions: crate::session::SessionStore::new(
                1024,
                std::time::Duration::from_secs(86400),
                true,
            ),
        })
    }

    /// Creates a session for a user and returns the cookie header value.
    fn create_session_for_user(state: &Arc<AppState>, user_id: i64) -> String {
        let ip = std::net::IpAddr::from([127, 0, 0, 1]);
        let token = state.sessions.create_session(ip, Some(user_id));
        format!("np_session={token}")
    }

    /// Creates a session without a selected user (for user creation).
    fn create_anonymous_session(state: &Arc<AppState>) -> String {
        let ip = std::net::IpAddr::from([127, 0, 0, 1]);
        let token = state.sessions.create_session(ip, None);
        format!("np_session={token}")
    }

    #[tokio::test]
    async fn health_endpoint_returns_ok() {
        let state = test_app_state();
        let app = crate::build_router(state, "*");

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "ok");
    }

    #[tokio::test]
    async fn unknown_api_route_returns_404() {
        let state = test_app_state();
        let app = crate::build_router(state, "*");

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/nonexistent")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn list_users_returns_empty_on_fresh_db() {
        let state = test_app_state();
        let app = crate::build_router(state, "*");

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/users")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let users: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
        // Fresh in-memory DB should have no users before any are created.
        assert!(users.is_empty());
    }

    #[tokio::test]
    async fn create_user_and_list() {
        let state = test_app_state();
        let app = crate::build_router(state.clone(), "*");
        let cookie = create_anonymous_session(&state);

        // Create a user
        let create_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/users")
                    .header("content-type", "application/json")
                    .header("cookie", &cookie)
                    .body(Body::from(
                        r#"{"username":"testuser","display_name":"Test User"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(create_response.status(), StatusCode::CREATED);

        let body = axum::body::to_bytes(create_response.into_body(), usize::MAX)
            .await
            .unwrap();
        let user: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(user["username"], "testuser");

        // List users (does not require auth)
        let list_response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/users")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(list_response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(list_response.into_body(), usize::MAX)
            .await
            .unwrap();
        let users: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
        assert!(users.iter().any(|u| u["username"] == "testuser"));
    }

    // =========================================================================
    // Error mapping tests
    // =========================================================================

    #[test]
    fn api_error_from_validation_returns_400() {
        let core_err = neuronprompter_core::CoreError::Validation {
            field: "title".into(),
            message: "too long".into(),
        };
        let service_err = neuronprompter_application::ServiceError::Core(core_err);
        let api_err = ApiError::from(service_err);
        assert_eq!(api_err.status, StatusCode::BAD_REQUEST);
        assert_eq!(api_err.body.code, "VALIDATION_ERROR");
    }

    #[test]
    fn api_error_from_not_found_returns_404() {
        let core_err = neuronprompter_core::CoreError::NotFound {
            entity: "prompt".into(),
            id: 42,
        };
        let service_err = neuronprompter_application::ServiceError::Core(core_err);
        let api_err = ApiError::from(service_err);
        assert_eq!(api_err.status, StatusCode::NOT_FOUND);
        assert_eq!(api_err.body.code, "NOT_FOUND");
    }

    #[test]
    fn api_error_from_duplicate_returns_409() {
        let core_err = neuronprompter_core::CoreError::Duplicate {
            entity: "tag".into(),
            field: "name".into(),
            value: "rust".into(),
        };
        let service_err = neuronprompter_application::ServiceError::Core(core_err);
        let api_err = ApiError::from(service_err);
        assert_eq!(api_err.status, StatusCode::CONFLICT);
        assert_eq!(api_err.body.code, "DUPLICATE");
    }

    #[test]
    fn api_error_from_ollama_unavailable_returns_502() {
        let service_err =
            neuronprompter_application::ServiceError::OllamaUnavailable("timeout".into());
        let api_err = ApiError::from(service_err);
        assert_eq!(api_err.status, StatusCode::BAD_GATEWAY);
        assert_eq!(api_err.body.code, "OLLAMA_UNAVAILABLE");
    }

    // =========================================================================
    // ClipboardState tests
    // =========================================================================

    #[test]
    fn clipboard_state_push_and_entries() {
        let state = ClipboardState::new();
        let user_id = 1;
        state.push(
            user_id,
            crate::state::ClipboardEntry {
                content: "hello".into(),
                prompt_title: "test".into(),
                copied_at: "2026-01-01T00:00:00Z".into(),
            },
        );
        let entries = state.entries(user_id);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].content, "hello");
    }

    #[test]
    fn clipboard_state_user_isolation() {
        let state = ClipboardState::new();
        state.push(
            1,
            crate::state::ClipboardEntry {
                content: "user1".into(),
                prompt_title: "test".into(),
                copied_at: "2026-01-01T00:00:00Z".into(),
            },
        );
        state.push(
            2,
            crate::state::ClipboardEntry {
                content: "user2".into(),
                prompt_title: "test".into(),
                copied_at: "2026-01-01T00:00:00Z".into(),
            },
        );
        assert_eq!(state.entries(1).len(), 1);
        assert_eq!(state.entries(1)[0].content, "user1");
        assert_eq!(state.entries(2).len(), 1);
        assert_eq!(state.entries(2)[0].content, "user2");
    }

    #[test]
    fn clipboard_state_clear() {
        let state = ClipboardState::new();
        let user_id = 1;
        state.push(
            user_id,
            crate::state::ClipboardEntry {
                content: "hello".into(),
                prompt_title: "test".into(),
                copied_at: "2026-01-01T00:00:00Z".into(),
            },
        );
        state.clear(user_id);
        assert!(state.entries(user_id).is_empty());
    }

    #[test]
    fn clipboard_state_capacity_50() {
        let state = ClipboardState::new();
        let user_id = 1;
        for i in 0..60 {
            state.push(
                user_id,
                crate::state::ClipboardEntry {
                    content: format!("item-{i}"),
                    prompt_title: "test".into(),
                    copied_at: "2026-01-01T00:00:00Z".into(),
                },
            );
        }
        assert_eq!(state.entries(user_id).len(), 50);
        assert_eq!(state.entries(user_id)[0].content, "item-59");
    }

    // =========================================================================
    // Database-wrapped NotFound maps to 404, not 500
    // =========================================================================

    #[test]
    fn api_error_from_db_core_not_found_returns_404() {
        let core_err = neuronprompter_core::CoreError::NotFound {
            entity: "Prompt".into(),
            id: 99999,
        };
        let db_err = neuronprompter_db::DbError::Core(core_err);
        let service_err = neuronprompter_application::ServiceError::Database(db_err);
        let api_err = ApiError::from(service_err);
        assert_eq!(api_err.status, StatusCode::NOT_FOUND);
        assert_eq!(api_err.body.code, "NOT_FOUND");
    }

    #[test]
    fn api_error_from_db_core_duplicate_returns_409() {
        let core_err = neuronprompter_core::CoreError::Duplicate {
            entity: "User".into(),
            field: "username".into(),
            value: "alice".into(),
        };
        let db_err = neuronprompter_db::DbError::Core(core_err);
        let service_err = neuronprompter_application::ServiceError::Database(db_err);
        let api_err = ApiError::from(service_err);
        assert_eq!(api_err.status, StatusCode::CONFLICT);
        assert_eq!(api_err.body.code, "DUPLICATE");
    }

    #[test]
    fn api_error_from_db_core_validation_returns_400() {
        let core_err = neuronprompter_core::CoreError::Validation {
            field: "tag_ids".into(),
            message: "belongs to another user".into(),
        };
        let db_err = neuronprompter_db::DbError::Core(core_err);
        let service_err = neuronprompter_application::ServiceError::Database(db_err);
        let api_err = ApiError::from(service_err);
        assert_eq!(api_err.status, StatusCode::BAD_REQUEST);
        assert_eq!(api_err.body.code, "VALIDATION_ERROR");
    }

    // =========================================================================
    // Non-existent resource returns 404 via API endpoint
    // =========================================================================

    #[tokio::test]
    async fn get_nonexistent_prompt_returns_404() {
        let state = test_app_state();
        // Create a user and session
        let pool = state.pool.clone();
        let user = neuronprompter_application::user_service::create_user(
            &pool,
            &neuronprompter_core::domain::user::NewUser {
                username: "testuser".to_owned(),
                display_name: "Test".to_owned(),
            },
        )
        .unwrap();
        let cookie = create_session_for_user(&state, user.id);
        let app = crate::build_router(state, "*");

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/prompts/99999")
                    .header("cookie", cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["code"], "NOT_FOUND");
    }

    #[tokio::test]
    async fn get_nonexistent_script_returns_404() {
        let state = test_app_state();
        let pool = state.pool.clone();
        let user = neuronprompter_application::user_service::create_user(
            &pool,
            &neuronprompter_core::domain::user::NewUser {
                username: "testuser2".to_owned(),
                display_name: "Test".to_owned(),
            },
        )
        .unwrap();
        let cookie = create_session_for_user(&state, user.id);
        let app = crate::build_router(state, "*");

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/scripts/99999")
                    .header("cookie", cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn get_nonexistent_chain_returns_404() {
        let state = test_app_state();
        let pool = state.pool.clone();
        let user = neuronprompter_application::user_service::create_user(
            &pool,
            &neuronprompter_core::domain::user::NewUser {
                username: "testuser3".to_owned(),
                display_name: "Test".to_owned(),
            },
        )
        .unwrap();
        let cookie = create_session_for_user(&state, user.id);
        let app = crate::build_router(state, "*");

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/chains/99999")
                    .header("cookie", cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    // =========================================================================
    // Duplicate user returns 409 via API
    // =========================================================================

    #[tokio::test]
    async fn duplicate_username_returns_409() {
        let state = test_app_state();
        let app = crate::build_router(state.clone(), "*");
        let cookie = create_anonymous_session(&state);

        let body_json = r#"{"username":"dupuser","display_name":"First"}"#;

        // Create first user
        let resp1 = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/users")
                    .header("content-type", "application/json")
                    .header("cookie", &cookie)
                    .body(Body::from(body_json))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp1.status(), StatusCode::CREATED);

        // Create duplicate
        let resp2 = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/users")
                    .header("content-type", "application/json")
                    .header("cookie", &cookie)
                    .body(Body::from(body_json))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp2.status(), StatusCode::CONFLICT);

        let body = axum::body::to_bytes(resp2.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["code"], "DUPLICATE");
    }

    // =========================================================================
    // Helper: create a user and return user_id
    // =========================================================================

    async fn create_test_user(app: &axum::Router, state: &Arc<AppState>, username: &str) -> i64 {
        let cookie = create_anonymous_session(state);
        let body = serde_json::json!({
            "username": username,
            "display_name": format!("Test {username}")
        });
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/users")
                    .header("content-type", "application/json")
                    .header("cookie", &cookie)
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED, "create user failed");
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        json["id"].as_i64().expect("user id should be i64")
    }

    /// Helper: create a prompt for a user and return the prompt JSON.
    async fn create_test_prompt(
        app: &axum::Router,
        cookie: &str,
        user_id: i64,
        title: &str,
    ) -> serde_json::Value {
        let body = serde_json::json!({
            "user_id": user_id,
            "title": title,
            "content": format!("Content for {title}")
        });
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/prompts")
                    .header("content-type", "application/json")
                    .header("cookie", cookie)
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED, "create prompt failed");
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    /// Helper: update a prompt to create a new version, return updated JSON.
    async fn update_test_prompt(
        app: &axum::Router,
        cookie: &str,
        prompt_id: i64,
        new_content: &str,
    ) -> serde_json::Value {
        let body = serde_json::json!({
            "prompt_id": prompt_id,
            "content": new_content
        });
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri(format!("/api/v1/prompts/{prompt_id}"))
                    .header("content-type", "application/json")
                    .header("cookie", cookie)
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK, "update prompt failed");
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    // =========================================================================
    // DEF-001: Cross-Prompt Version Restore blocked
    // =========================================================================

    #[tokio::test]
    async fn restore_version_with_foreign_version_id_returns_400() {
        let state = test_app_state();
        let app = crate::build_router(state.clone(), "*");

        let uid = create_test_user(&app, &state, "versionuser").await;
        let cookie = create_session_for_user(&state, uid);
        let prompt_a = create_test_prompt(&app, &cookie, uid, "Prompt A").await;
        let prompt_b = create_test_prompt(&app, &cookie, uid, "Prompt B").await;
        let a_id = prompt_a["id"].as_i64().unwrap();
        let b_id = prompt_b["id"].as_i64().unwrap();

        // Update both prompts to create version 2.
        update_test_prompt(&app, &cookie, a_id, "A updated content").await;
        update_test_prompt(&app, &cookie, b_id, "B updated content").await;

        // Get version IDs for prompt B.
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/v1/versions/prompt/{b_id}"))
                    .header("cookie", &cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let versions: Vec<serde_json::Value> = serde_json::from_slice(&bytes).unwrap();
        let b_version_id = versions[0]["id"].as_i64().unwrap();

        // Attempt to restore prompt A using prompt B's version_id.
        let restore_body = serde_json::json!({ "version_id": b_version_id });
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/v1/versions/prompt/{a_id}/restore"))
                    .header("content-type", "application/json")
                    .header("cookie", &cookie)
                    .body(Body::from(restore_body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["code"], "VALIDATION_ERROR");
    }

    // =========================================================================
    // DEF-002: User isolation in list endpoints
    // =========================================================================

    #[tokio::test]
    async fn list_prompts_enforces_user_isolation() {
        let state = test_app_state();
        let app = crate::build_router(state.clone(), "*");

        let uid_a = create_test_user(&app, &state, "usera").await;
        let uid_b = create_test_user(&app, &state, "userb").await;
        let cookie_a = create_session_for_user(&state, uid_a);
        let cookie_b = create_session_for_user(&state, uid_b);
        create_test_prompt(&app, &cookie_a, uid_a, "A's Prompt").await;
        create_test_prompt(&app, &cookie_b, uid_b, "B's Prompt").await;

        // List prompts as user A -- should only see user A's prompt.
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/prompts/search")
                    .header("content-type", "application/json")
                    .header("cookie", &cookie_a)
                    .body(Body::from("{}"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let prompts = body["items"].as_array().unwrap();
        assert_eq!(prompts.len(), 1);
        assert_eq!(prompts[0]["user_id"], uid_a);
    }

    #[tokio::test]
    async fn list_prompts_without_session_returns_401() {
        // Override is_localhost to false so auto-session is not created.
        let pool =
            neuronprompter_db::create_in_memory_pool().expect("in-memory pool should be created");
        let (log_tx, _) = tokio::sync::broadcast::channel(128);
        let (model_tx, _) = tokio::sync::broadcast::channel(64);
        let state = Arc::new(AppState {
            pool,
            ollama: OllamaState::new(),
            clipboard: ClipboardState::new(),
            log_tx,
            model_tx,
            cancellation: tokio_util::sync::CancellationToken::new(),
            session_token: "test-token".to_owned(),
            rate_limiter: std::sync::Arc::new(crate::middleware::rate_limit::RateLimiter::new(
                120, 60,
            )),
            sessions: crate::session::SessionStore::new(
                1024,
                std::time::Duration::from_secs(86400),
                false, // not localhost
            ),
        });
        let app = crate::build_router(state, "*");

        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/prompts/search")
                    .header("content-type", "application/json")
                    .body(Body::from("{}"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    // =========================================================================
    // DEF-005: Optional association ID vectors
    // =========================================================================

    #[tokio::test]
    async fn create_prompt_without_association_ids_succeeds() {
        let state = test_app_state();
        let app = crate::build_router(state.clone(), "*");

        let uid = create_test_user(&app, &state, "noassoc").await;
        let cookie = create_session_for_user(&state, uid);
        // Deliberately omit tag_ids, category_ids, collection_ids.
        let body = serde_json::json!({
            "user_id": uid,
            "title": "No Associations",
            "content": "Simple prompt"
        });
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/prompts")
                    .header("content-type", "application/json")
                    .header("cookie", cookie)
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);
    }

    #[tokio::test]
    async fn create_script_without_association_ids_succeeds() {
        let state = test_app_state();
        let app = crate::build_router(state.clone(), "*");

        let uid = create_test_user(&app, &state, "noassocscript").await;
        let cookie = create_session_for_user(&state, uid);
        let body = serde_json::json!({
            "user_id": uid,
            "title": "No Associations Script",
            "content": "print('hi')",
            "script_language": "python"
        });
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/scripts")
                    .header("content-type", "application/json")
                    .header("cookie", cookie)
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);
    }

    // =========================================================================
    // DEF-009: Chain creation with non-existent IDs returns 404
    // =========================================================================

    #[tokio::test]
    async fn create_chain_with_nonexistent_prompt_returns_404() {
        let state = test_app_state();
        let app = crate::build_router(state.clone(), "*");

        let uid = create_test_user(&app, &state, "chainuser").await;
        let cookie = create_session_for_user(&state, uid);
        let body = serde_json::json!({
            "user_id": uid,
            "title": "Bad Chain",
            "prompt_ids": [99999]
        });
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/chains")
                    .header("content-type", "application/json")
                    .header("cookie", cookie)
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["code"], "NOT_FOUND");
    }

    #[tokio::test]
    async fn create_chain_with_nonexistent_script_step_returns_404() {
        let state = test_app_state();
        let app = crate::build_router(state.clone(), "*");

        let uid = create_test_user(&app, &state, "chainuserscript").await;
        let cookie = create_session_for_user(&state, uid);
        let body = serde_json::json!({
            "user_id": uid,
            "title": "Bad Script Chain",
            "steps": [
                { "step_type": "script", "item_id": 99999 }
            ]
        });
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/chains")
                    .header("content-type", "application/json")
                    .header("cookie", cookie)
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    // =========================================================================
    // DEF-010/011: Ollama content validation
    // =========================================================================

    #[tokio::test]
    async fn ollama_improve_empty_content_returns_400() {
        let state = test_app_state();
        let app = crate::build_router(state.clone(), "*");

        // Create a user and session since ollama_improve requires AuthUser.
        let uid = create_test_user(&app, &state, "ollamauser1").await;
        let cookie = create_session_for_user(&state, uid);

        let body = serde_json::json!({
            "base_url": "http://localhost:11434",
            "model": "test",
            "content": ""
        });
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/ollama/improve")
                    .header("content-type", "application/json")
                    .header("cookie", &cookie)
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["code"], "VALIDATION_ERROR");
    }

    #[tokio::test]
    async fn ollama_translate_empty_content_returns_400() {
        let state = test_app_state();
        let app = crate::build_router(state.clone(), "*");

        // Create a user and session since ollama_translate requires AuthUser.
        let uid = create_test_user(&app, &state, "ollamauser2").await;
        let cookie = create_session_for_user(&state, uid);

        let body = serde_json::json!({
            "base_url": "http://localhost:11434",
            "model": "test",
            "content": "",
            "target_language": "de"
        });
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/ollama/translate")
                    .header("content-type", "application/json")
                    .header("cookie", &cookie)
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn ollama_autofill_empty_content_returns_400() {
        let state = test_app_state();
        let app = crate::build_router(state.clone(), "*");

        // Create a user and session since ollama_autofill requires AuthUser.
        let uid = create_test_user(&app, &state, "ollamauser3").await;
        let cookie = create_session_for_user(&state, uid);

        let body = serde_json::json!({
            "base_url": "http://localhost:11434",
            "model": "test",
            "content": "   "
        });
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/ollama/autofill")
                    .header("content-type", "application/json")
                    .header("cookie", &cookie)
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    // =========================================================================
    // DEF-012: Script language hyphen position
    // =========================================================================

    #[tokio::test]
    async fn create_script_with_leading_hyphen_language_returns_400() {
        let state = test_app_state();
        let app = crate::build_router(state.clone(), "*");

        let uid = create_test_user(&app, &state, "hyphenuser").await;
        let cookie = create_session_for_user(&state, uid);
        let body = serde_json::json!({
            "user_id": uid,
            "title": "Bad Language",
            "content": "print('hi')",
            "script_language": "-python"
        });
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/scripts")
                    .header("content-type", "application/json")
                    .header("cookie", cookie)
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["code"], "VALIDATION_ERROR");
    }

    // =========================================================================
    // DEF-013: Binary file import returns 400, not 500
    // =========================================================================

    #[tokio::test]
    async fn import_binary_file_returns_validation_error() {
        let state = test_app_state();
        let app = crate::build_router(state.clone(), "*");

        let uid = create_test_user(&app, &state, "binaryuser").await;
        let cookie = create_session_for_user(&state, uid);

        // Create a temp binary file inside the user's home directory
        // to pass path-within-home validation.
        let home = std::env::var("HOME").map_or_else(
            |_| std::path::PathBuf::from("/tmp"),
            std::path::PathBuf::from,
        );
        let tmp_dir = tempfile::tempdir_in(&home).unwrap();
        let bin_path = tmp_dir.path().join("test.bin");
        std::fs::write(&bin_path, [0xFF, 0xFE, 0x00, 0x01, 0x80, 0x81]).unwrap();

        let body = serde_json::json!({
            "user_id": uid,
            "path": bin_path.to_str().unwrap(),
            "is_synced": false
        });
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/scripts/import-file")
                    .header("content-type", "application/json")
                    .header("cookie", cookie)
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        // Should be 400 VALIDATION_ERROR, not 500 IO_ERROR.
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["code"], "VALIDATION_ERROR");
    }

    // =========================================================================
    // DEF-016: Shutdown endpoint not registered in headless mode
    // =========================================================================

    #[tokio::test]
    async fn shutdown_not_available_in_headless_mode() {
        let state = test_app_state();
        let app = crate::build_router_headless(state, "*");

        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/shutdown")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"token":"anything"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        // Endpoint should not exist in headless mode.
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn shutdown_requires_valid_token_in_gui_mode() {
        let state = test_app_state();
        let app = crate::build_router(state, "*");

        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/shutdown")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"token":"wrong-token"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    // =========================================================================
    // DEF-005 extended: Chain creation without association IDs
    // =========================================================================

    #[tokio::test]
    async fn create_chain_without_association_ids_succeeds() {
        let state = test_app_state();
        let app = crate::build_router(state.clone(), "*");

        let uid = create_test_user(&app, &state, "chainnoassoc").await;
        let cookie = create_session_for_user(&state, uid);
        let prompt = create_test_prompt(&app, &cookie, uid, "Chain Prompt").await;
        let pid = prompt["id"].as_i64().unwrap();

        // Omit tag_ids, category_ids, collection_ids.
        let body = serde_json::json!({
            "user_id": uid,
            "title": "Chain No Assoc",
            "prompt_ids": [pid]
        });
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/chains")
                    .header("content-type", "application/json")
                    .header("cookie", cookie)
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);
    }

    // =========================================================================
    // DEF-007: Unicode taxonomy names stored and retrieved correctly
    // =========================================================================

    #[tokio::test]
    async fn unicode_tag_names_round_trip() {
        let state = test_app_state();
        let app = crate::build_router(state.clone(), "*");

        let uid = create_test_user(&app, &state, "unicodetaguser").await;
        let cookie = create_session_for_user(&state, uid);

        // Create a tag with Chinese characters.
        let body = serde_json::json!({
            "name": "\u{4e2d}\u{6587}\u{6807}\u{7b7e}",
            "user_id": uid
        });
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/tags")
                    .header("content-type", "application/json")
                    .header("cookie", &cookie)
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);

        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let tag: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(tag["name"], "\u{4e2d}\u{6587}\u{6807}\u{7b7e}");
    }
}
