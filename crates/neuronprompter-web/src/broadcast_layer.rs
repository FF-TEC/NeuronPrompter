// =============================================================================
// Tracing BroadcastLayer for SSE log forwarding.
//
// A custom `tracing::Layer` that serializes tracing events as JSON and sends
// them through a `tokio::sync::broadcast` channel. SSE consumers subscribe
// to this channel to receive real-time log messages in the browser.
// =============================================================================

use tokio::sync::broadcast;
use tracing::Subscriber;
use tracing_subscriber::Layer;
use tracing_subscriber::layer::Context;

/// A tracing `Layer` that broadcasts structured log events as JSON strings.
///
/// Each tracing event (INFO, WARN, ERROR, DEBUG, TRACE) is serialized to:
/// ```json
/// {"level":"INFO","target":"module::path","message":"log text"}
/// ```
///
/// Dropped messages (channel full) are silently ignored to prevent backpressure
/// from slow SSE consumers from blocking the application.
pub struct BroadcastLayer {
    tx: broadcast::Sender<String>,
}

impl BroadcastLayer {
    /// Creates a new `BroadcastLayer` backed by the given broadcast sender.
    #[must_use]
    pub fn new(tx: broadcast::Sender<String>) -> Self {
        Self { tx }
    }
}

impl<S: Subscriber> Layer<S> for BroadcastLayer {
    fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
        // Skip serialization entirely when no SSE clients are connected.
        if self.tx.receiver_count() == 0 {
            return;
        }

        let metadata = event.metadata();
        let level = metadata.level().as_str();
        let target = metadata.target();

        // Extract the message from the event fields.
        let mut visitor = MessageVisitor::default();
        event.record(&mut visitor);

        let json = serde_json::json!({
            "level": level,
            "target": target,
            "message": visitor.message,
        });

        // Fire-and-forget: if the channel is full or has no receivers,
        // the message is silently dropped.
        let _ = self.tx.send(json.to_string());
    }
}

/// Visitor that extracts the "message" field from a tracing event.
#[derive(Default)]
struct MessageVisitor {
    message: String,
}

impl tracing::field::Visit for MessageVisitor {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.message = format!("{value:?}");
        }
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "message" {
            value.clone_into(&mut self.message);
        }
    }
}
