// =============================================================================
// Ollama client and workflow integration tests.
//
// Uses mockito to simulate the Ollama HTTP API, verifying health checks,
// generation requests, and the three workflow modules (improve, translate,
// metadata) against controlled mock responses.
// =============================================================================

use crate::ServiceError;
use crate::ollama::client::OllamaClient;
use crate::ollama::{improve, metadata, translate};

#[tokio::test]
async fn health_check_returns_model_list() {
    let mut server = mockito::Server::new_async().await;

    let mock = server
        .mock("GET", "/api/tags")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"models":[{"name":"llama3:latest"},{"name":"mistral:7b"}]}"#)
        .create_async()
        .await;

    let client = OllamaClient::with_base_url(&server.url());
    let models = client.check_health().await.unwrap();

    assert_eq!(models.len(), 2);
    assert!(models.contains(&"llama3:latest".to_owned()));
    assert!(models.contains(&"mistral:7b".to_owned()));
    mock.assert_async().await;
}

#[tokio::test]
async fn health_check_unavailable_returns_error() {
    // Point to a port that is not listening.
    let client = OllamaClient::with_base_url("http://127.0.0.1:1");
    let result = client.check_health().await;

    assert!(result.is_err());
    match result.unwrap_err() {
        ServiceError::OllamaUnavailable(_) => {}
        other => panic!("expected OllamaUnavailable, got: {other:?}"),
    }
}

#[tokio::test]
async fn health_check_non_200_returns_error() {
    let mut server = mockito::Server::new_async().await;

    let mock = server
        .mock("GET", "/api/tags")
        .with_status(500)
        .with_body("Internal Server Error")
        .create_async()
        .await;

    let client = OllamaClient::with_base_url(&server.url());
    let result = client.check_health().await;

    assert!(result.is_err());
    match result.unwrap_err() {
        ServiceError::OllamaUnavailable(msg) => {
            assert!(msg.contains("500"), "error should contain HTTP status");
        }
        other => panic!("expected OllamaUnavailable, got: {other:?}"),
    }
    mock.assert_async().await;
}

#[tokio::test]
async fn generate_returns_response_text() {
    let mut server = mockito::Server::new_async().await;

    let mock = server
        .mock("POST", "/api/generate")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"response":"Generated text output"}"#)
        .create_async()
        .await;

    let client = OllamaClient::with_base_url(&server.url());
    let result = client
        .generate("llama3", "Hello", Some("Be helpful"), None)
        .await
        .unwrap();

    assert_eq!(result, "Generated text output");
    mock.assert_async().await;
}

#[tokio::test]
async fn generate_non_200_returns_error() {
    let mut server = mockito::Server::new_async().await;

    let mock = server
        .mock("POST", "/api/generate")
        .with_status(404)
        .with_body("model not found")
        .create_async()
        .await;

    let client = OllamaClient::with_base_url(&server.url());
    let result = client.generate("nonexistent", "Hello", None, None).await;

    assert!(result.is_err());
    match result.unwrap_err() {
        ServiceError::OllamaError(msg) => {
            assert!(msg.contains("404"));
        }
        other => panic!("expected OllamaError, got: {other:?}"),
    }
    mock.assert_async().await;
}

// ---------------------------------------------------------------------------
// Workflow tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn improve_prompt_returns_refined_text() {
    let mut server = mockito::Server::new_async().await;

    let mock = server
        .mock("POST", "/api/generate")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"response":"A clearer version of the prompt."}"#)
        .create_async()
        .await;

    let client = OllamaClient::with_base_url(&server.url());
    let result = improve::improve_prompt(&client, "llama3", "Vague prompt text")
        .await
        .unwrap();

    assert_eq!(result, "A clearer version of the prompt.");
    mock.assert_async().await;
}

#[tokio::test]
async fn translate_prompt_returns_translated_text() {
    let mut server = mockito::Server::new_async().await;

    let mock = server
        .mock("POST", "/api/generate")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"response":"Hallo Welt"}"#)
        .create_async()
        .await;

    let client = OllamaClient::with_base_url(&server.url());
    let result = translate::translate_prompt(&client, "llama3", "Hello World", "German")
        .await
        .unwrap();

    assert_eq!(result, "Hallo Welt");
    mock.assert_async().await;
}

#[tokio::test]
async fn derive_all_metadata_returns_structured_result() {
    let mut server = mockito::Server::new_async().await;

    // The metadata module makes 5 sequential POST calls to /api/generate.
    // We set up a mock that matches all of them and returns a simple string.
    let mock = server
        .mock("POST", "/api/generate")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"response":"Code Review"}"#)
        .expect_at_least(5)
        .create_async()
        .await;

    let client = OllamaClient::with_base_url(&server.url());
    let result = metadata::derive_all_metadata(&client, "llama3", "Review this code for bugs")
        .await
        .unwrap();

    assert!(result.errors.is_empty());
    mock.assert_async().await;
}

#[tokio::test]
async fn derive_all_metadata_server_down_returns_error() {
    // Point to a port that is not listening — all 5 sub-calls will fail.
    let client = OllamaClient::with_base_url("http://127.0.0.1:1");
    let result = metadata::derive_all_metadata(&client, "llama3", "Some prompt").await;

    assert!(result.is_err());
    match result.unwrap_err() {
        ServiceError::OllamaUnavailable(_) => {}
        other => panic!("expected OllamaUnavailable, got: {other:?}"),
    }
}
