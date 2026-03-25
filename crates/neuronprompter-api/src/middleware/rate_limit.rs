use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::Instant;

use axum::extract::{ConnectInfo, Request, State};
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use dashmap::DashMap;

use crate::state::AppState;

/// Maximum number of distinct IP entries tracked before refusing new IPs.
/// Prevents unbounded memory growth from spoofed source addresses.
const MAX_IPS: usize = 10_000;

/// A token bucket for a single IP address.
struct TokenBucket {
    tokens: f64,
    last_refill: Instant,
}

/// Per-IP token-bucket rate limiter backed by a concurrent `DashMap`.
///
/// Each check is O(1) — no vec scanning or retention needed.
pub struct RateLimiter {
    buckets: DashMap<IpAddr, TokenBucket>,
    max_requests: f64,
    window_secs: f64,
}

impl RateLimiter {
    /// Creates a new rate limiter allowing `max_requests` within `window_secs`.
    #[allow(clippy::cast_precision_loss)]
    pub fn new(max_requests: u64, window_secs: u64) -> Self {
        Self {
            buckets: DashMap::new(),
            max_requests: max_requests as f64,
            window_secs: window_secs as f64,
        }
    }

    /// Returns `true` if the request should be allowed, `false` if rate-limited.
    pub fn check(&self, ip: IpAddr) -> bool {
        let now = Instant::now();

        // Bound the number of tracked IPs to prevent memory exhaustion.
        if !self.buckets.contains_key(&ip) && self.buckets.len() >= MAX_IPS {
            self.cleanup();
            if self.buckets.len() >= MAX_IPS {
                return false;
            }
        }

        let mut entry = self.buckets.entry(ip).or_insert_with(|| TokenBucket {
            tokens: self.max_requests,
            last_refill: now,
        });
        let bucket = entry.value_mut();

        // Refill tokens based on elapsed time.
        let elapsed = now.duration_since(bucket.last_refill).as_secs_f64();
        let rate = self.max_requests / self.window_secs;
        bucket.tokens = (bucket.tokens + elapsed * rate).min(self.max_requests);
        bucket.last_refill = now;

        if bucket.tokens >= 1.0 {
            bucket.tokens -= 1.0;
            true
        } else {
            false
        }
    }

    /// Removes stale entries that have fully refilled (idle for at least one window).
    pub fn cleanup(&self) {
        let now = Instant::now();
        self.buckets.retain(|_, bucket| {
            let elapsed = now.duration_since(bucket.last_refill).as_secs_f64();
            let rate = self.max_requests / self.window_secs;
            let projected = bucket.tokens + elapsed * rate;
            // Keep only entries that haven't fully refilled yet (still rate-limited).
            projected < self.max_requests
        });
    }
}

/// Axum middleware layer that applies per-IP rate limiting.
/// Returns 429 Too Many Requests when the limit is exceeded.
pub async fn rate_limit_layer(
    State(state): State<Arc<AppState>>,
    req: Request,
    next: Next,
) -> Response {
    // Extract IP from ConnectInfo if available, fall back to loopback.
    let ip = req
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map_or_else(
            || {
                tracing::warn!(
                    "ConnectInfo not available in request extensions; \
                     falling back to 127.0.0.1 for rate limiting"
                );
                IpAddr::from([127, 0, 0, 1])
            },
            |ci| ci.0.ip(),
        );
    if !state.rate_limiter.check(ip) {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            [(axum::http::header::CONTENT_TYPE, "application/json")],
            r#"{"code":"RATE_LIMITED","message":"Too many requests"}"#,
        )
            .into_response();
    }
    next.run(req).await
}
