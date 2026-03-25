// =============================================================================
// Shared constants used across the NeuronPrompter workspace.
//
// Centralizes magic numbers and default values so that every crate references
// a single source of truth instead of duplicating string literals.
// =============================================================================

/// Default Ollama API endpoint URL.
pub const DEFAULT_OLLAMA_URL: &str = "http://localhost:11434";

/// Default HTTP server port.
pub const DEFAULT_PORT: u16 = 3030;

/// Broadcast channel capacity for log events streamed via SSE.
pub const LOG_CHANNEL_CAPACITY: usize = 2048;

/// Broadcast channel capacity for model operation events.
pub const MODEL_CHANNEL_CAPACITY: usize = 64;

/// Maximum requests per IP within the rate limit window.
pub const RATE_LIMIT_REQUESTS: u64 = 120;

/// Rate limit window duration in seconds.
pub const RATE_LIMIT_WINDOW_SECS: u64 = 60;

/// Maximum concurrent sessions in the session store.
pub const MAX_SESSIONS: usize = 1024;

/// Session time-to-live in seconds (24 hours).
pub const SESSION_TTL_SECS: u64 = 86_400;

/// Interval in seconds between rate limiter cleanup sweeps.
pub const RATE_LIMITER_CLEANUP_SECS: u64 = 60;

/// Interval in seconds between expired session cleanup sweeps.
pub const SESSION_CLEANUP_SECS: u64 = 300;

/// Maximum seconds to wait for in-flight connections during graceful shutdown.
pub const DRAIN_TIMEOUT_SECS: u64 = 5;

/// Number of consecutive ports to try when the preferred port is occupied.
pub const PORT_SCAN_RANGE: u16 = 20;
