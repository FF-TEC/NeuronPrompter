// =============================================================================
// Server-Sent Event (SSE) endpoints for real-time streaming.
//
// Provides SSE streams for:
// - `/api/v1/events/logs` -- real-time log messages from the BroadcastLayer
// - `/api/v1/events/models` -- Ollama model operation progress events
//
// Each stream uses a tokio broadcast channel with connection counting to
// prevent resource exhaustion from too many concurrent SSE clients.
// =============================================================================

use std::convert::Infallible;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::sse::{Event, KeepAlive, Sse};
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;

use crate::{MAX_SSE_CONNECTIONS, WebState};

/// SSE keep-alive interval to prevent proxy/browser timeouts.
const KEEP_ALIVE_INTERVAL: Duration = Duration::from_secs(15);

/// SSE stream for real-time log events.
///
/// # Errors
///
/// Returns `StatusCode::UNAUTHORIZED` if the request has no valid session
/// cookie or no user is selected. Returns `StatusCode::SERVICE_UNAVAILABLE`
/// if the maximum number of concurrent SSE connections has been reached.
pub async fn logs_stream(
    State(web_state): State<Arc<WebState>>,
    headers: HeaderMap,
) -> Result<Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>>, axum::http::StatusCode>
{
    // Extract and validate session from cookie.
    let token = neuronprompter_api::middleware::session::extract_cookie_token(&headers);
    let has_valid_session = token
        .and_then(|t| web_state.app_state.sessions.get_session(&t))
        .is_some_and(|s| s.user_id().is_some());
    if !has_valid_session {
        return Err(axum::http::StatusCode::UNAUTHORIZED);
    }

    let prev = web_state.sse_connections.fetch_add(1, Ordering::AcqRel);
    if prev >= MAX_SSE_CONNECTIONS {
        web_state.sse_connections.fetch_sub(1, Ordering::AcqRel);
        return Err(axum::http::StatusCode::SERVICE_UNAVAILABLE);
    }

    let rx = web_state.log_tx.subscribe();
    let ws = Arc::clone(&web_state);

    let stream = BroadcastStream::new(rx).filter_map(|result| match result {
        Ok(data) => Some(Ok(Event::default().event("log").data(data))),
        Err(tokio_stream::wrappers::errors::BroadcastStreamRecvError::Lagged(_)) => None,
    });

    // Wrap in a stream that decrements the connection count when dropped.
    let guarded = SseGuardedStream {
        inner: stream,
        web_state: ws,
        decremented: false,
    };

    Ok(Sse::new(guarded).keep_alive(KeepAlive::new().interval(KEEP_ALIVE_INTERVAL)))
}

/// SSE stream for model operation events (pull progress, completion, errors).
///
/// # Errors
///
/// Returns `StatusCode::UNAUTHORIZED` if the request has no valid session
/// cookie or no user is selected. Returns `StatusCode::SERVICE_UNAVAILABLE`
/// if the maximum number of concurrent SSE connections has been reached.
pub async fn models_stream(
    State(web_state): State<Arc<WebState>>,
    headers: HeaderMap,
) -> Result<Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>>, axum::http::StatusCode>
{
    // Extract and validate session from cookie.
    let token = neuronprompter_api::middleware::session::extract_cookie_token(&headers);
    let has_valid_session = token
        .and_then(|t| web_state.app_state.sessions.get_session(&t))
        .is_some_and(|s| s.user_id().is_some());
    if !has_valid_session {
        return Err(axum::http::StatusCode::UNAUTHORIZED);
    }

    let prev = web_state.sse_connections.fetch_add(1, Ordering::AcqRel);
    if prev >= MAX_SSE_CONNECTIONS {
        web_state.sse_connections.fetch_sub(1, Ordering::AcqRel);
        return Err(axum::http::StatusCode::SERVICE_UNAVAILABLE);
    }

    let rx = web_state.model_tx.subscribe();
    let ws = Arc::clone(&web_state);

    let stream = BroadcastStream::new(rx).filter_map(|result| match result {
        Ok(data) => {
            // Parse inner event type from JSON data so the frontend's
            // listeners for "ollama_pull_progress", "ollama_pull_complete",
            // "ollama_pull_error" receive the correct SSE event type.
            let event_type = serde_json::from_str::<serde_json::Value>(&data)
                .ok()
                .and_then(|v| v.get("event").and_then(|e| e.as_str().map(String::from)))
                .unwrap_or_else(|| "model".to_string());
            Some(Ok(Event::default().event(event_type).data(data)))
        }
        Err(tokio_stream::wrappers::errors::BroadcastStreamRecvError::Lagged(_)) => None,
    });

    let guarded = SseGuardedStream {
        inner: stream,
        web_state: ws,
        decremented: false,
    };

    Ok(Sse::new(guarded).keep_alive(KeepAlive::new().interval(KEEP_ALIVE_INTERVAL)))
}

/// A stream wrapper that decrements the SSE connection count when dropped.
use std::pin::Pin;
use std::task::{Context, Poll};

struct SseGuardedStream<S> {
    inner: S,
    web_state: Arc<WebState>,
    decremented: bool,
}

impl<S> Drop for SseGuardedStream<S> {
    fn drop(&mut self) {
        if !self.decremented {
            let _ = self.web_state.sse_connections.fetch_update(
                Ordering::AcqRel,
                Ordering::Acquire,
                |val| val.checked_sub(1),
            );
            self.decremented = true;
        }
    }
}

impl<S> tokio_stream::Stream for SseGuardedStream<S>
where
    S: tokio_stream::Stream + Unpin,
{
    type Item = S::Item;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.inner).poll_next(cx)
    }
}
