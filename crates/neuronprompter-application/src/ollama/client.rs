// =============================================================================
// Ollama HTTP client for local LLM inference.
//
// Wraps reqwest to communicate with the Ollama REST API at a configurable
// base URL. Provides health checking (model list retrieval), text generation
// via the /api/generate endpoint, and model management operations (list,
// pull, delete, show).
// =============================================================================

use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

use crate::ServiceError;

/// HTTP client for the Ollama local inference server.
pub struct OllamaClient {
    http: reqwest::Client,
    base_url: String,
}

/// Detailed model information returned by the `/api/tags` and `/api/ps`
/// endpoints. Fields are optional because different endpoints return
/// different subsets of metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaModelInfo {
    /// Model tag (e.g., "llama3.2:3b").
    pub name: String,
    /// Size in bytes on disk (from `/api/tags`) or in VRAM (from `/api/ps`).
    pub size: Option<u64>,
    /// Content-addressable digest of the model blob.
    pub digest: Option<String>,
    /// ISO-8601 timestamp of the last modification.
    pub modified_at: Option<String>,
    /// Additional details (family, parameter_size, quantization, etc.).
    #[serde(default)]
    pub details: Option<serde_json::Value>,
}

/// Response payload from the Ollama /api/tags endpoint listing available models.
#[derive(Debug, Deserialize)]
struct TagsResponse {
    models: Vec<OllamaModelInfo>,
}

/// Response payload from the Ollama /api/ps endpoint listing running models.
#[derive(Debug, Deserialize)]
struct PsResponse {
    models: Vec<OllamaModelInfo>,
}

/// A single progress line from a streaming `/api/pull` response.
#[derive(Debug, Deserialize)]
struct PullProgressLine {
    status: Option<String>,
    total: Option<u64>,
    completed: Option<u64>,
}

/// Request payload for the Ollama /api/generate endpoint.
#[derive(Debug, Serialize)]
struct GenerateRequest<'a> {
    model: &'a str,
    prompt: &'a str,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    format: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<&'a str>,
}

/// Response payload from the Ollama /api/generate endpoint (non-streaming).
#[derive(Debug, Deserialize)]
struct GenerateResponse {
    response: String,
}

impl OllamaClient {
    /// Creates a client targeting the specified Ollama base URL.
    /// The URL should not include a trailing slash.
    ///
    /// The HTTP client uses a 300-second (5-minute) timeout to accommodate
    /// slow LLM inference on large models. Ollama processes generation
    /// requests sequentially, so callers should avoid sending concurrent
    /// requests to prevent timeout budget exhaustion from queue waiting.
    #[must_use]
    pub fn with_base_url(base_url: &str) -> Self {
        let timeout = std::time::Duration::from_secs(300);
        let http = reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .unwrap_or_else(|e| {
                // The builder failed, likely due to TLS or system configuration.
                // Retry without TLS customization but preserve the timeout to
                // prevent indefinite waits on slow or unresponsive servers.
                tracing::error!(
                    "Failed to build HTTP client with TLS configuration: {e}. \
                     Retrying without TLS customization."
                );
                reqwest::Client::builder()
                    .timeout(timeout)
                    .build()
                    .unwrap_or_else(|e2| {
                        tracing::error!(
                            "Failed to build fallback HTTP client: {e2}. \
                             Using default client without timeout."
                        );
                        reqwest::Client::new()
                    })
            });

        Self {
            http,
            base_url: base_url.trim_end_matches('/').to_owned(),
        }
    }

    /// Creates an OllamaClient that reuses an existing HTTP client connection pool.
    /// The caller provides a pre-configured reqwest::Client (with timeout, TLS, etc.)
    /// and the Ollama server base URL. This avoids allocating a separate connection
    /// pool per request when the application already maintains a shared client.
    #[must_use]
    pub fn with_client_and_url(client: reqwest::Client, base_url: &str) -> Self {
        Self {
            http: client,
            base_url: base_url.trim_end_matches('/').to_owned(),
        }
    }

    /// Returns the base URL of this client.
    #[must_use]
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Checks whether the Ollama server is reachable by calling GET /api/tags.
    /// Returns the list of available model names on success.
    ///
    /// # Errors
    ///
    /// Returns `ServiceError::OllamaUnavailable` if the server is not reachable
    /// or returns a non-success HTTP status.
    /// Returns `ServiceError::OllamaError` if the response cannot be parsed.
    pub async fn check_health(&self) -> Result<Vec<String>, ServiceError> {
        let models = self.list_local_models().await?;
        Ok(models.into_iter().map(|m| m.name).collect())
    }

    /// Lists all models installed on the Ollama server (GET `/api/tags`).
    ///
    /// # Errors
    ///
    /// Returns `ServiceError::OllamaUnavailable` if the server is not reachable
    /// or returns a non-success HTTP status.
    /// Returns `ServiceError::OllamaError` if the response cannot be parsed.
    pub async fn list_local_models(&self) -> Result<Vec<OllamaModelInfo>, ServiceError> {
        let url = format!("{}/api/tags", self.base_url);
        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .map_err(|e| ServiceError::OllamaUnavailable(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(ServiceError::OllamaUnavailable(format!(
                "HTTP {}",
                resp.status()
            )));
        }

        let tags: TagsResponse = resp
            .json()
            .await
            .map_err(|e| ServiceError::OllamaError(e.to_string()))?;

        Ok(tags.models)
    }

    /// Lists models currently loaded in GPU/CPU RAM (GET `/api/ps`).
    ///
    /// # Errors
    ///
    /// Returns `ServiceError::OllamaUnavailable` if the server is not reachable
    /// or returns a non-success HTTP status.
    /// Returns `ServiceError::OllamaError` if the response cannot be parsed.
    pub async fn list_running_models(&self) -> Result<Vec<OllamaModelInfo>, ServiceError> {
        let url = format!("{}/api/ps", self.base_url);
        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .map_err(|e| ServiceError::OllamaUnavailable(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(ServiceError::OllamaUnavailable(format!(
                "HTTP {}",
                resp.status()
            )));
        }

        let ps: PsResponse = resp
            .json()
            .await
            .map_err(|e| ServiceError::OllamaError(e.to_string()))?;

        Ok(ps.models)
    }

    /// Pulls (downloads) a model from the Ollama registry with streaming
    /// progress. Progress events are sent through the provided broadcast
    /// channel as JSON strings. The method blocks until the pull completes
    /// or fails.
    ///
    /// # Errors
    ///
    /// Returns `ServiceError::OllamaUnavailable` if the server is not reachable.
    /// Returns `ServiceError::OllamaError` if the server returns a non-success
    /// HTTP status or a chunk cannot be read.
    pub async fn pull_model_streaming(
        &self,
        name: &str,
        progress_tx: &broadcast::Sender<String>,
    ) -> Result<(), ServiceError> {
        let url = format!("{}/api/pull", self.base_url);
        let body = serde_json::json!({ "name": name, "stream": true });

        let mut resp = self
            .http
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| ServiceError::OllamaUnavailable(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(ServiceError::OllamaError(format!(
                "pull HTTP {}",
                resp.status()
            )));
        }

        // Read the streaming NDJSON response using chunked transfer.
        // `resp.chunk()` returns `Option<Bytes>` without requiring the
        // reqwest "stream" feature flag.
        let mut buffer = String::new();
        let model_name = name.to_string();

        while let Some(chunk) = resp
            .chunk()
            .await
            .map_err(|e| ServiceError::OllamaError(e.to_string()))?
        {
            buffer.push_str(&String::from_utf8_lossy(&chunk));

            // Performance note: The NDJSON line parser allocates a new String per line via
            // drain+collect. A zero-copy approach using slice references would avoid these
            // allocations but would complicate lifetime management. The allocation overhead
            // is negligible compared to the network I/O latency of model inference.
            // Process complete lines from the buffer.
            while let Some(newline_pos) = buffer.find('\n') {
                let line: String = buffer.drain(..=newline_pos).collect();
                let line = line.trim().to_string();

                if line.is_empty() {
                    continue;
                }

                if let Ok(progress) = serde_json::from_str::<PullProgressLine>(&line) {
                    let event = serde_json::json!({
                        "event": "ollama_pull_progress",
                        "model": &model_name,
                        "status": progress.status.as_deref().unwrap_or(""),
                        "total": progress.total,
                        "completed": progress.completed,
                    });
                    let _ = progress_tx.send(event.to_string());
                }
            }
        }

        Ok(())
    }

    /// Deletes an installed model from the Ollama server (DELETE `/api/delete`).
    ///
    /// # Errors
    ///
    /// Returns `ServiceError::OllamaUnavailable` if the server is not reachable.
    /// Returns `ServiceError::OllamaError` if the server returns a non-success
    /// HTTP status.
    pub async fn delete_model(&self, name: &str) -> Result<(), ServiceError> {
        let url = format!("{}/api/delete", self.base_url);
        let body = serde_json::json!({ "name": name });

        let resp = self
            .http
            .delete(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| ServiceError::OllamaUnavailable(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(ServiceError::OllamaError(format!(
                "delete HTTP {}",
                resp.status()
            )));
        }

        Ok(())
    }

    /// Retrieves detailed metadata for a single model (POST `/api/show`).
    ///
    /// # Errors
    ///
    /// Returns `ServiceError::OllamaUnavailable` if the server is not reachable.
    /// Returns `ServiceError::OllamaError` if the server returns a non-success
    /// HTTP status or the response cannot be parsed.
    pub async fn show_model(&self, name: &str) -> Result<serde_json::Value, ServiceError> {
        let url = format!("{}/api/show", self.base_url);
        let body = serde_json::json!({ "name": name });

        let resp = self
            .http
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| ServiceError::OllamaUnavailable(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(ServiceError::OllamaError(format!(
                "show HTTP {}",
                resp.status()
            )));
        }

        let detail: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| ServiceError::OllamaError(e.to_string()))?;

        Ok(detail)
    }

    /// Sends a generation request to Ollama with streaming disabled.
    /// Returns the complete response text.
    ///
    /// # Errors
    ///
    /// Returns `ServiceError::OllamaUnavailable` if the server is not reachable.
    /// Returns `ServiceError::OllamaError` if the server returns a non-success
    /// HTTP status or the response cannot be parsed.
    pub async fn generate(
        &self,
        model: &str,
        prompt: &str,
        system: Option<&str>,
        format: Option<&str>,
    ) -> Result<String, ServiceError> {
        let url = format!("{}/api/generate", self.base_url);
        let body = GenerateRequest {
            model,
            prompt,
            stream: false,
            format,
            system,
        };

        let resp = self
            .http
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| ServiceError::OllamaUnavailable(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(ServiceError::OllamaError(format!("HTTP {}", resp.status())));
        }

        let result: GenerateResponse = resp
            .json()
            .await
            .map_err(|e| ServiceError::OllamaError(e.to_string()))?;

        Ok(result.response)
    }
}
