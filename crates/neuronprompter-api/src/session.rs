// =============================================================================
// Session store for multi-user authentication.
//
// Provides an in-memory, bounded session store backed by DashMap. Each session
// maps a cryptographic token to a user context, enabling per-client user
// isolation for LAN deployments. Sessions expire after a configurable TTL and
// are evicted LRU when the maximum session count is reached.
//
// Session expiry uses an absolute TTL measured from the session's creation time
// (the `created_at` field). The TTL is not extended by subsequent requests --
// once the elapsed time since creation exceeds `session_ttl`, the session is
// invalid regardless of recent activity. The `last_active` field tracks the
// most recent request timestamp but is only used for LRU eviction when the
// store reaches capacity; it does not influence the session's lifetime.
// =============================================================================

use std::net::IpAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use dashmap::DashMap;

/// 256-bit session token (32 bytes).
type TokenBytes = [u8; 32];

/// A single authenticated session.
pub struct Session {
    /// The user ID associated with this session, or `None` if the user has
    /// not yet selected a profile (session created but no user chosen).
    pub user_id: std::sync::atomic::AtomicI64,
    /// Timestamp when the session was created.
    pub created_at: Instant,
    /// Last activity timestamp as seconds since UNIX epoch (for atomic updates).
    /// This field is only used for LRU eviction ordering, not for extending
    /// the session lifetime.
    last_active: AtomicU64,
    /// IP address that created the session.
    pub ip: IpAddr,
}

/// Sentinel value indicating no user is selected within a session.
const NO_USER: i64 = i64::MIN;

impl Session {
    /// Returns the user ID if one is set, or `None`.
    #[must_use]
    pub fn user_id(&self) -> Option<i64> {
        match self.user_id.load(Ordering::Acquire) {
            NO_USER => None,
            id => Some(id),
        }
    }

    /// Sets the user ID for this session.
    pub fn set_user_id(&self, user_id: Option<i64>) {
        self.user_id
            .store(user_id.unwrap_or(NO_USER), Ordering::Release);
    }

    /// Updates the last-active timestamp to now. This timestamp is used for
    /// LRU eviction ordering only; it does not extend the session TTL.
    fn touch(&self) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        self.last_active.store(now, Ordering::Release);
    }

    /// Returns the last-active timestamp as seconds since UNIX epoch.
    fn last_active_secs(&self) -> u64 {
        self.last_active.load(Ordering::Acquire)
    }
}

/// In-memory session store with bounded capacity and TTL-based expiry.
pub struct SessionStore {
    sessions: DashMap<TokenBytes, Session>,
    max_sessions: usize,
    session_ttl: Duration,
    /// Whether the server is running in localhost mode (enables auto-session).
    pub is_localhost: bool,
}

impl SessionStore {
    /// Creates a session store with the given capacity, TTL, and localhost flag.
    #[must_use]
    pub fn new(max_sessions: usize, session_ttl: Duration, is_localhost: bool) -> Self {
        Self {
            sessions: DashMap::new(),
            max_sessions,
            session_ttl,
            is_localhost,
        }
    }

    /// Creates a session and returns the hex-encoded token string.
    ///
    /// If the store is at capacity, the least-recently-active session is evicted.
    pub fn create_session(&self, ip: IpAddr, user_id: Option<i64>) -> String {
        // Evict if at capacity.
        if self.sessions.len() >= self.max_sessions {
            self.evict_lru();
        }

        let mut token_bytes = [0u8; 32];
        {
            use rand::Rng;
            rand::rng().fill_bytes(&mut token_bytes);
        }

        let now_epoch = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let session = Session {
            user_id: std::sync::atomic::AtomicI64::new(user_id.unwrap_or(NO_USER)),
            created_at: Instant::now(),
            last_active: AtomicU64::new(now_epoch),
            ip,
        };

        self.sessions.insert(token_bytes, session);
        hex_encode(&token_bytes)
    }

    /// Looks up a session by hex-encoded token, returning a reference if valid.
    ///
    /// Returns `None` if the token is invalid, the session has expired, or
    /// the token cannot be decoded.
    ///
    /// Session lookup uses DashMap's hash-based key retrieval rather than
    /// constant-time comparison. This is acceptable because session tokens are
    /// 256-bit cryptographic random values (2^256 possible values), making
    /// timing-based brute force infeasible -- an attacker cannot meaningfully
    /// narrow the key space by observing hash lookup timing differences.
    pub fn get_session(
        &self,
        token_hex: &str,
    ) -> Option<dashmap::mapref::one::Ref<'_, TokenBytes, Session>> {
        let token_bytes = hex_decode(token_hex)?;
        let entry = self.sessions.get(&token_bytes)?;

        // Check TTL against creation time. This is an absolute expiry check:
        // the session becomes invalid once `created_at.elapsed()` exceeds the
        // configured TTL, regardless of intervening activity.
        if entry.created_at.elapsed() > self.session_ttl {
            drop(entry);
            self.sessions.remove(&token_bytes);
            return None;
        }

        // Update last-active timestamp for LRU eviction ordering.
        entry.touch();
        Some(entry)
    }

    /// Sets the user ID on an existing session. Returns `false` if the
    /// session does not exist.
    pub fn set_session_user(&self, token_hex: &str, user_id: i64) -> bool {
        if let Some(token_bytes) = hex_decode(token_hex) {
            if let Some(entry) = self.sessions.get(&token_bytes) {
                entry.set_user_id(Some(user_id));
                entry.touch();
                return true;
            }
        }
        false
    }

    /// Removes a session by hex-encoded token.
    pub fn remove_session(&self, token_hex: &str) {
        if let Some(token_bytes) = hex_decode(token_hex) {
            self.sessions.remove(&token_bytes);
        }
    }

    /// Removes all sessions associated with a given user ID.
    /// Called when a user is deleted to invalidate their sessions.
    pub fn remove_sessions_for_user(&self, user_id: i64) {
        self.sessions
            .retain(|_, session| session.user_id() != Some(user_id));
    }

    /// Removes all expired sessions. Called periodically by a background task.
    pub fn cleanup_expired(&self) {
        let ttl = self.session_ttl;
        self.sessions
            .retain(|_, session| session.created_at.elapsed() <= ttl);
    }

    /// Evicts the least-recently-active session to make room for a new one.
    fn evict_lru(&self) {
        let mut oldest_key: Option<TokenBytes> = None;
        let mut oldest_time = u64::MAX;

        for entry in &self.sessions {
            let last = entry.value().last_active_secs();
            if last < oldest_time {
                oldest_time = last;
                oldest_key = Some(*entry.key());
            }
        }

        if let Some(key) = oldest_key {
            self.sessions.remove(&key);
        }
    }

    /// Returns the number of active sessions.
    #[must_use]
    pub fn len(&self) -> usize {
        self.sessions.len()
    }

    /// Returns true if there are no active sessions.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.sessions.is_empty()
    }

    /// Returns the configured session TTL.
    #[must_use]
    pub fn session_ttl(&self) -> Duration {
        self.session_ttl
    }
}

/// Hex-encodes a 32-byte token to a 64-character lowercase string.
fn hex_encode(bytes: &[u8; 32]) -> String {
    let mut s = String::with_capacity(64);
    for b in bytes {
        use std::fmt::Write;
        let _ = write!(s, "{b:02x}");
    }
    s
}

/// Hex-decodes a 64-character string to a 32-byte token.
/// Returns `None` if the string is not exactly 64 hex characters.
fn hex_decode(hex: &str) -> Option<TokenBytes> {
    if hex.len() != 64 {
        return None;
    }
    let mut bytes = [0u8; 32];
    for (i, chunk) in hex.as_bytes().chunks(2).enumerate() {
        let high = hex_digit(chunk[0])?;
        let low = hex_digit(chunk[1])?;
        bytes[i] = (high << 4) | low;
    }
    Some(bytes)
}

/// Converts a single ASCII hex digit to its numeric value.
const fn hex_digit(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}
