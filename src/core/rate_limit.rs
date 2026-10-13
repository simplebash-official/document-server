// Per-client request limits. In memory by design: one document-server runs
// per database, and a restart only ever errs towards letting callers back in.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Instant,
};

use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};

use crate::core::{constants::codes, error::AppError};

/// Above this many tracked keys, fully-refilled buckets are swept on the next
/// write so a stream of random keys can't grow the map without bound.
const SWEEP_THRESHOLD: usize = 10_000;

/// Token bucket per key: `burst` requests at once, refilled at `refill_per_sec`.
struct TokenBuckets {
    burst: f64,
    refill_per_sec: f64,
    buckets: Mutex<HashMap<String, (f64, Instant)>>,
}

impl TokenBuckets {
    fn new(burst: f64, refill_per_sec: f64) -> Self {
        Self {
            burst,
            refill_per_sec,
            buckets: Mutex::new(HashMap::new()),
        }
    }

    /// Takes one token for `key`; false when the bucket is empty.
    fn take(&self, key: &str, now: Instant) -> bool {
        let mut buckets = self.buckets.lock().unwrap_or_else(|e| e.into_inner());
        if buckets.len() >= SWEEP_THRESHOLD {
            let (burst, refill) = (self.burst, self.refill_per_sec);
            buckets.retain(|_, (tokens, at)| {
                *tokens + now.duration_since(*at).as_secs_f64() * refill < burst
            });
        }
        let entry = buckets.entry(key.to_string()).or_insert((self.burst, now));
        let elapsed = now.duration_since(entry.1).as_secs_f64();
        entry.0 = (entry.0 + elapsed * self.refill_per_sec).min(self.burst);
        entry.1 = now;
        if entry.0 >= 1.0 {
            entry.0 -= 1.0;
            true
        } else {
            false
        }
    }
}

/// A `general` bucket on every route plus a tighter `heavy` one on the
/// CPU-bound Typst renders (`/api/render`).
pub struct RequestLimits {
    general: TokenBuckets,
    heavy: TokenBuckets,
}

fn env_f64(name: &str, default: f64) -> f64 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.trim().parse().ok())
        .filter(|v: &f64| *v >= 0.0)
        .unwrap_or(default)
}

impl RequestLimits {
    pub fn new(general: (f64, f64), heavy: (f64, f64)) -> Self {
        Self {
            general: TokenBuckets::new(general.0, general.1),
            heavy: TokenBuckets::new(heavy.0, heavy.1),
        }
    }

    /// Defaults, overridable with `RATE_LIMIT_BURST`, `RATE_LIMIT_REFILL_PER_SEC`,
    /// `RATE_LIMIT_RENDER_BURST` and `RATE_LIMIT_RENDER_REFILL_PER_SEC`.
    pub fn from_env() -> Self {
        Self::new(
            (
                env_f64("RATE_LIMIT_BURST", 60.0),
                env_f64("RATE_LIMIT_REFILL_PER_SEC", 20.0),
            ),
            (
                env_f64("RATE_LIMIT_RENDER_BURST", 20.0),
                env_f64("RATE_LIMIT_RENDER_REFILL_PER_SEC", 5.0),
            ),
        )
    }

    fn allow(&self, client: &str, path: &str) -> bool {
        let now = Instant::now();
        self.general.take(client, now)
            && (!path.starts_with("/api/render") || self.heavy.take(&format!("r:{client}"), now))
    }
}

/// Axum middleware: answers 429 once a client exhausts its bucket. The client
/// is nginx's `X-Real-IP` (nginx overwrites it, so it can't be spoofed through
/// the proxy); a caller reaching the service directly has none and shares one
/// "local" bucket.
pub async fn limit_requests(
    State(limits): State<Arc<RequestLimits>>,
    req: Request,
    next: Next,
) -> Response {
    let client = req
        .headers()
        .get("x-real-ip")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("local");
    if limits.allow(client, req.uri().path()) {
        next.run(req).await
    } else {
        AppError::custom(
            StatusCode::TOO_MANY_REQUESTS,
            codes::RATE_LIMITED,
            "Too many requests. Please slow down and try again shortly.",
        )
        .into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn bucket_runs_dry_and_refills() {
        let b = TokenBuckets::new(2.0, 1.0);
        let t = Instant::now();
        assert!(b.take("k", t) && b.take("k", t));
        assert!(!b.take("k", t));
        assert!(b.take("k", t + Duration::from_secs(1)));
    }

    #[test]
    fn render_is_limited_harder_and_per_client() {
        let limits = RequestLimits::new((100.0, 0.0), (1.0, 0.0));
        assert!(limits.allow("a", "/api/render/invoice"));
        assert!(!limits.allow("a", "/api/render/invoice"));
        assert!(
            limits.allow("a", "/api/templates"),
            "other routes unaffected"
        );
        assert!(
            limits.allow("b", "/api/render/invoice"),
            "other clients unaffected"
        );
    }

    #[tokio::test]
    async fn middleware_answers_429_with_the_error_envelope() {
        use axum::{Router, body::Body, http::Request, routing::get};
        use tower::ServiceExt;

        let limits = Arc::new(RequestLimits::new((1.0, 0.0), (1.0, 0.0)));
        let app = Router::new()
            .route("/api/health", get(|| async { "ok" }))
            .layer(axum::middleware::from_fn_with_state(limits, limit_requests));
        let call = |ip: &'static str| {
            let app = app.clone();
            async move {
                app.oneshot(
                    Request::builder()
                        .uri("/api/health")
                        .header("x-real-ip", ip)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap()
            }
        };
        assert_eq!(call("1.1.1.1").await.status(), 200);
        let blocked = call("1.1.1.1").await;
        assert_eq!(blocked.status(), 429);
        let body = axum::body::to_bytes(blocked.into_body(), 4096)
            .await
            .unwrap();
        assert!(String::from_utf8_lossy(&body).contains("RATE_LIMITED"));
        assert_eq!(call("2.2.2.2").await.status(), 200);
    }
}
