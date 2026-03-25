// =============================================================================
// Dependency probe handler for the welcome dialog.
//
// Checks whether optional dependencies (currently only Ollama) are reachable
// and returns structured status information for the frontend to display.
//
// Endpoints:
// - GET /api/v1/web/doctor/probes -- check dependency availability
// =============================================================================

use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use serde::Serialize;

use neuronprompter_application::ollama::client::OllamaClient;

use crate::WebState;

use neuronprompter_core::constants::DEFAULT_OLLAMA_URL;

/// Status of a single dependency probe.
#[derive(Debug, Serialize)]
pub struct DependencyProbe {
    /// Display name of the dependency.
    pub name: String,
    /// Short description of what the dependency provides.
    pub purpose: String,
    /// Whether the dependency is currently reachable.
    pub available: bool,
    /// Whether the dependency is required for basic operation.
    pub required: bool,
    /// Human-readable hint when the dependency is unavailable.
    pub hint: String,
    /// URL for manual installation instructions.
    pub link: String,
    /// Number of models available (Ollama-specific, 0 otherwise).
    pub model_count: usize,
}

/// Aggregate response for all dependency probes.
#[derive(Debug, Serialize)]
pub struct DoctorProbes {
    pub probes: Vec<DependencyProbe>,
}

/// GET /api/v1/web/doctor/probes
///
/// Probes all optional dependencies and returns their status. Currently
/// checks Ollama connectivity by attempting to list installed models.
pub async fn run_probes(State(_web_state): State<Arc<WebState>>) -> Json<DoctorProbes> {
    let client = OllamaClient::with_base_url(DEFAULT_OLLAMA_URL);

    let (available, model_count) = match client.check_health().await {
        Ok(models) => (true, models.len()),
        Err(_) => (false, 0),
    };

    let ollama_probe = DependencyProbe {
        name: "Ollama".to_string(),
        purpose: "Local AI server for prompt optimization, translation, and auto-fill".to_string(),
        available,
        required: false,
        hint: "Install Ollama for AI-powered features. NeuronPrompter works without it."
            .to_string(),
        link: "https://ollama.com".to_string(),
        model_count,
    };

    Json(DoctorProbes {
        probes: vec![ollama_probe],
    })
}
