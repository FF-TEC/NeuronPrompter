// =============================================================================
// Shared application state for the axum server.
//
// Holds the database connection pool, active user tracking, Ollama connection
// state, clipboard history, and broadcast channels for SSE event streams.
// =============================================================================

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::sync::{Mutex, RwLock};

use neuronprompter_db::DbPool;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

use crate::middleware::rate_limit::RateLimiter;
use crate::session::SessionStore;

/// Core application state shared across all axum handlers via `Arc<AppState>`.
pub struct AppState {
    /// r2d2 SQLite connection pool.
    pub pool: DbPool,
    /// Ollama connection state.
    pub ollama: OllamaState,
    /// In-memory clipboard history ring buffer, keyed by user_id.
    pub clipboard: ClipboardState,
    /// Broadcast sender for log events (consumed by SSE `/api/v1/events/logs`).
    pub log_tx: broadcast::Sender<String>,
    /// Broadcast sender for model operation events (consumed by SSE `/api/v1/events/models`).
    pub model_tx: broadcast::Sender<String>,
    /// Cancellation token for graceful shutdown of background tasks.
    pub cancellation: CancellationToken,
    /// Session token for authenticating shutdown requests.
    /// Kept `pub(crate)` to prevent accidental exposure across crate boundaries.
    // NOTE: Debug impl should not print this field -- it is a secret.
    pub(crate) session_token: String,
    /// Per-IP rate limiter available for handlers and middleware.
    pub rate_limiter: Arc<RateLimiter>,
    /// In-memory session store for multi-user authentication.
    pub sessions: SessionStore,
}

/// Configuration bundle for constructing `AppState`.
/// Groups required dependencies to avoid a long parameter list.
pub struct AppStateConfig {
    /// r2d2 SQLite connection pool.
    pub pool: DbPool,
    /// Ollama connection state.
    pub ollama: OllamaState,
    /// In-memory clipboard history ring buffer, keyed by user_id.
    pub clipboard: ClipboardState,
    /// Broadcast sender for log events.
    pub log_tx: broadcast::Sender<String>,
    /// Broadcast sender for model operation events.
    pub model_tx: broadcast::Sender<String>,
    /// Cancellation token for graceful shutdown.
    pub cancellation: CancellationToken,
    /// Session token for authenticating shutdown requests.
    pub session_token: String,
    /// Per-IP rate limiter.
    pub rate_limiter: Arc<RateLimiter>,
    /// In-memory session store.
    pub sessions: SessionStore,
}

impl AppState {
    /// Constructs a fully-initialized `AppState` from the provided configuration.
    /// The `session_token` is stored as a `pub(crate)` field and only accessible
    /// through the `session_token()` accessor from outside this crate.
    pub fn new(config: AppStateConfig) -> Self {
        Self {
            pool: config.pool,
            ollama: config.ollama,
            clipboard: config.clipboard,
            log_tx: config.log_tx,
            model_tx: config.model_tx,
            cancellation: config.cancellation,
            session_token: config.session_token,
            rate_limiter: config.rate_limiter,
            sessions: config.sessions,
        }
    }

    /// Returns a reference to the shutdown authentication token.
    /// The token is kept non-public to prevent accidental exposure across crate boundaries.
    pub fn session_token(&self) -> &str {
        &self.session_token
    }
}

/// Ollama connection state tracking reachability and cached model list.
pub struct OllamaState {
    /// Whether the Ollama server responded to the last health check.
    pub connected: RwLock<bool>,
    /// Model names returned by the last successful `/api/tags` call.
    pub models: RwLock<Vec<String>>,
    /// Shared `reqwest` client for connection pooling across Ollama requests.
    pub http: reqwest::Client,
}

impl OllamaState {
    #[must_use]
    pub fn new() -> Self {
        Self {
            connected: RwLock::new(false),
            models: RwLock::new(Vec::new()),
            http: reqwest::Client::new(),
        }
    }
}

impl Default for OllamaState {
    fn default() -> Self {
        Self::new()
    }
}

/// A single clipboard history entry recording what was copied and when.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ClipboardEntry {
    pub content: String,
    pub prompt_title: String,
    pub copied_at: String,
}

/// In-memory ring buffer of recent clipboard operations, **keyed by user_id**
/// for user isolation. Each user gets their own ring buffer with a fixed
/// capacity of 50 entries, newest first. Not persisted to the database.
pub struct ClipboardState {
    per_user: Mutex<HashMap<i64, VecDeque<ClipboardEntry>>>,
}

/// Maximum number of clipboard history entries retained in memory per user.
const CLIPBOARD_HISTORY_CAPACITY: usize = 50;

impl ClipboardState {
    #[must_use]
    pub fn new() -> Self {
        Self {
            per_user: Mutex::new(HashMap::new()),
        }
    }

    /// Pushes a new entry to the front of the specified user's history,
    /// evicting the oldest entry if the buffer is at capacity.
    pub fn push(&self, user_id: i64, entry: ClipboardEntry) {
        let mut map = match self.per_user.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                tracing::error!("clipboard mutex poisoned, recovering");
                poisoned.into_inner()
            }
        };
        let entries = map
            .entry(user_id)
            .or_insert_with(|| VecDeque::with_capacity(CLIPBOARD_HISTORY_CAPACITY));
        entries.push_front(entry);
        while entries.len() > CLIPBOARD_HISTORY_CAPACITY {
            entries.pop_back();
        }
    }

    /// Returns a snapshot of all clipboard history entries for a given user,
    /// newest first.
    #[must_use]
    pub fn entries(&self, user_id: i64) -> Vec<ClipboardEntry> {
        let map = match self.per_user.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                tracing::error!("clipboard mutex poisoned, recovering");
                poisoned.into_inner()
            }
        };
        map.get(&user_id)
            .map(|entries| entries.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Removes all entries from the specified user's clipboard history.
    pub fn clear(&self, user_id: i64) {
        let mut map = match self.per_user.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                tracing::error!("clipboard mutex poisoned, recovering");
                poisoned.into_inner()
            }
        };
        if let Some(entries) = map.get_mut(&user_id) {
            entries.clear();
        }
    }
}

impl Default for ClipboardState {
    fn default() -> Self {
        Self::new()
    }
}

// Compile-time verification that AppState satisfies the thread-safety requirements
// imposed by axum's State extractor (which wraps AppState in Arc<T>).
const _: () = {
    fn assert_send_sync<T: Send + Sync>() {}
    fn check() {
        assert_send_sync::<AppState>();
    }
};
