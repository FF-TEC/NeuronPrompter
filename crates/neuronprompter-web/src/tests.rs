// =============================================================================
// Web crate tests: BroadcastLayer, SSE, and asset serving.
// =============================================================================

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

#[cfg(test)]
#[allow(clippy::module_inception)]
mod tests {
    use crate::broadcast_layer::BroadcastLayer;
    use tokio::sync::broadcast;
    use tracing_subscriber::layer::SubscriberExt;

    #[test]
    fn broadcast_layer_sends_log_events() {
        let (tx, mut rx) = broadcast::channel::<String>(128);
        let layer = BroadcastLayer::new(tx);

        let subscriber = tracing_subscriber::registry().with(layer);
        let _guard = tracing::subscriber::set_default(subscriber);

        tracing::info!("test message");

        let received = rx.try_recv();
        assert!(received.is_ok(), "should receive a log event");
        let json_str = received.unwrap();
        let json: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert_eq!(json["level"], "INFO");
    }

    #[test]
    fn broadcast_layer_drops_when_no_receivers() {
        let (tx, _) = broadcast::channel::<String>(128);
        // Drop all receivers
        let layer = BroadcastLayer::new(tx);

        let subscriber = tracing_subscriber::registry().with(layer);
        let _guard = tracing::subscriber::set_default(subscriber);

        // Should not panic even with no receivers.
        tracing::info!("message with no receivers");
    }

    #[test]
    fn web_state_creation() {
        let pool = neuronprompter_db::create_in_memory_pool().expect("pool should be created");
        let (log_tx, _) = broadcast::channel(128);
        let (model_tx, _) = broadcast::channel(64);
        // Use AppState::new() because the session_token field is pub(crate) and
        // cannot be set via struct literal from outside the neuronprompter-api crate.
        let app_state = std::sync::Arc::new(neuronprompter_api::AppState::new(
            neuronprompter_api::state::AppStateConfig {
                pool,
                ollama: neuronprompter_api::state::OllamaState::new(),
                clipboard: neuronprompter_api::state::ClipboardState::new(),
                log_tx: log_tx.clone(),
                model_tx,
                cancellation: tokio_util::sync::CancellationToken::new(),
                session_token: "test-token".to_owned(),
                rate_limiter: std::sync::Arc::new(
                    neuronprompter_api::middleware::rate_limit::RateLimiter::new(120, 60),
                ),
                sessions: neuronprompter_api::session::SessionStore::new(
                    1024,
                    std::time::Duration::from_secs(86400),
                    true,
                ),
            },
        ));

        let web_state = crate::WebState::new(app_state, log_tx);
        assert!(!web_state.native_dialogs);
        assert_eq!(
            web_state
                .sse_connections
                .load(std::sync::atomic::Ordering::Relaxed),
            0
        );
    }
}
