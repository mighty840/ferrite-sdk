//! Simple token-bucket rate limiter middleware.
//!
//! Limits requests per IP address. Configurable via `RATE_LIMIT_RPS` env var.
//! Applied to ingest and auth endpoints to prevent abuse.

use axum::{
    extract::ConnectInfo,
    http::{Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;

/// Per-IP token bucket state.
struct Bucket {
    tokens: f64,
    last_refill: Instant,
}

/// Shared rate limiter state.
pub struct RateLimiter {
    buckets: Mutex<HashMap<std::net::IpAddr, Bucket>>,
    rate: f64,  // tokens per second
    burst: f64, // max tokens (bucket capacity)
}

impl RateLimiter {
    pub fn new(rate: f64, burst: f64) -> Self {
        Self {
            buckets: Mutex::new(HashMap::new()),
            rate,
            burst,
        }
    }

    /// Try to consume a token. Returns true if allowed, false if rate-limited.
    pub async fn try_acquire(&self, ip: std::net::IpAddr) -> bool {
        let mut buckets = self.buckets.lock().await;
        let now = Instant::now();

        let bucket = buckets.entry(ip).or_insert(Bucket {
            tokens: self.burst,
            last_refill: now,
        });

        // Refill tokens based on elapsed time.
        let elapsed = now.duration_since(bucket.last_refill).as_secs_f64();
        bucket.tokens = (bucket.tokens + elapsed * self.rate).min(self.burst);
        bucket.last_refill = now;

        if bucket.tokens >= 1.0 {
            bucket.tokens -= 1.0;
            true
        } else {
            false
        }
    }

    /// Periodically clean up stale entries (called from background task).
    pub async fn cleanup(&self) {
        let mut buckets = self.buckets.lock().await;
        let now = Instant::now();
        buckets.retain(|_, b| now.duration_since(b.last_refill).as_secs() < 300);
    }
}

/// Axum middleware that rate-limits based on client IP.
/// Only applied to /ingest/* and /auth/* paths.
pub async fn rate_limit_middleware(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    req: Request<axum::body::Body>,
    next: Next,
) -> Response {
    let path = req.uri().path();

    // Only rate-limit ingest and auth paths.
    if !path.starts_with("/ingest") && !path.starts_with("/auth") {
        return next.run(req).await;
    }

    // Extract rate limiter from extensions (set by layer).
    let limiter = req.extensions().get::<Arc<RateLimiter>>().cloned();

    if let Some(limiter) = limiter {
        if !limiter.try_acquire(addr.ip()).await {
            return (
                StatusCode::TOO_MANY_REQUESTS,
                [("retry-after", "1")],
                "Rate limit exceeded",
            )
                .into_response();
        }
    }

    next.run(req).await
}

/// Start a background task to periodically clean up stale rate-limit buckets.
pub fn spawn_cleanup_task(limiter: Arc<RateLimiter>) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(60));
        loop {
            interval.tick().await;
            limiter.cleanup().await;
        }
    });
}
