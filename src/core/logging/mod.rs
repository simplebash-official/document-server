// Structured logging for document-server: subscriber setup, the
// per-request logging middleware and secret redaction. Mirrors the
// backend's `core::logging` so both sidecars emit the same shape.
//
// Two output modes, picked by `LOG_FORMAT`:
// - `text` (default, web/Docker): the human-readable `fmt` output as before.
// - `json` (set by the Tauri desktop shell): one JSON object per line on
//   stdout with a local-offset timestamp, which the shell ingests into the
//   unified desktop activity log. Contract: `docs/logging.md` in the
//   compose repo.

pub mod control;
pub mod redact;
pub mod request;

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::time::ChronoLocal;

/// Logging knobs. The three marked *runtime* can be changed while the process
/// runs, by a `log_mode` control line on stdin (see `control.rs`) — that is
/// how the desktop System Benchmark measures logging on vs off without a
/// restart. Env config is the one place a default is allowed (see CLAUDE.md),
/// and every variable is optional: the web deployment sets none of them.
#[derive(Debug)]
pub struct LogSettings {
    /// `LOG_FORMAT=json` → JSON lines on stdout. Startup only.
    pub json: bool,
    /// `LOG_BODY_CAP_BYTES` → per-body cap when bodies are logged. Startup only.
    pub body_cap_bytes: usize,
    /// *Runtime*: record anything at all.
    enabled: AtomicBool,
    /// *Runtime*: `LOG_HTTP_BODIES` → log request/response bodies.
    http_bodies: AtomicBool,
    /// *Runtime*: `LOG_SQL` → SQLite statement logging.
    sql: AtomicU8,
    /// Whether sqlx was wired for statement logging at connect time; when the
    /// process started with `LOG_SQL=off` no runtime change can turn it on.
    pub sql_at_startup: SqlLogging,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SqlLogging {
    All,
    Slow,
    Off,
}

impl SqlLogging {
    pub fn parse(raw: &str) -> SqlLogging {
        match raw.trim().to_ascii_lowercase().as_str() {
            "all" => SqlLogging::All,
            "off" => SqlLogging::Off,
            _ => SqlLogging::Slow,
        }
    }

    fn code(self) -> u8 {
        match self {
            SqlLogging::Off => 0,
            SqlLogging::Slow => 1,
            SqlLogging::All => 2,
        }
    }

    fn from_code(code: u8) -> SqlLogging {
        match code {
            0 => SqlLogging::Off,
            2 => SqlLogging::All,
            _ => SqlLogging::Slow,
        }
    }
}

impl LogSettings {
    fn from_env() -> Self {
        let var = |key: &str| {
            std::env::var(key)
                .ok()
                .map(|v| v.trim().to_ascii_lowercase())
        };
        let sql = var("LOG_SQL").map_or(SqlLogging::Slow, |v| SqlLogging::parse(&v));
        LogSettings {
            json: var("LOG_FORMAT").as_deref() == Some("json"),
            body_cap_bytes: var("LOG_BODY_CAP_BYTES")
                .and_then(|v| v.parse().ok())
                .unwrap_or(32 * 1024),
            enabled: AtomicBool::new(true),
            http_bodies: AtomicBool::new(matches!(
                var("LOG_HTTP_BODIES").as_deref(),
                Some("true" | "1" | "yes")
            )),
            sql: AtomicU8::new(sql.code()),
            sql_at_startup: sql,
        }
    }

    pub fn enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    pub fn http_bodies(&self) -> bool {
        self.enabled() && self.http_bodies.load(Ordering::Relaxed)
    }

    pub fn sql(&self) -> SqlLogging {
        SqlLogging::from_code(self.sql.load(Ordering::Relaxed))
    }

    /// Apply a runtime change (from a stdin control line).
    pub fn apply(&self, enabled: bool, http_bodies: bool, sql: SqlLogging) {
        self.enabled.store(enabled, Ordering::Relaxed);
        self.http_bodies.store(http_bodies, Ordering::Relaxed);
        self.sql.store(sql.code(), Ordering::Relaxed);
    }
}

pub fn settings() -> &'static LogSettings {
    static SETTINGS: OnceLock<LogSettings> = OnceLock::new();
    SETTINGS.get_or_init(LogSettings::from_env)
}

/// Install the global subscriber. `default_filter` applies when `RUST_LOG`
/// is unset.
pub fn init(default_filter: &str) {
    let filter =
        EnvFilter::try_from_env("RUST_LOG").unwrap_or_else(|_| EnvFilter::new(default_filter));
    if settings().json {
        use tracing_subscriber::Layer;
        use tracing_subscriber::layer::SubscriberExt;
        use tracing_subscriber::util::SubscriberInitExt;

        let fmt_layer = tracing_subscriber::fmt::layer()
            .json()
            .flatten_event(true)
            // The full span list (not just the innermost span) keeps the
            // request span's `request_id` on lines emitted inside nested spans.
            .with_current_span(false)
            .with_span_list(true)
            .with_target(true)
            .with_timer(ChronoLocal::rfc_3339())
            .with_ansi(false)
            // Checked per event, so a `log_mode` control line takes effect
            // immediately and costs nothing when logging is on.
            .with_filter(tracing_subscriber::filter::filter_fn(|meta| {
                let settings = settings();
                if !settings.enabled() {
                    return false;
                }
                if meta.target().starts_with("sqlx::query") {
                    return match settings.sql() {
                        SqlLogging::All => true,
                        // sqlx logs slow statements at WARN, the rest at INFO.
                        SqlLogging::Slow => *meta.level() <= tracing::Level::WARN,
                        SqlLogging::Off => false,
                    };
                }
                true
            }));
        tracing_subscriber::registry()
            .with(filter)
            .with(fmt_layer)
            .init();
    } else {
        tracing_subscriber::fmt().with_env_filter(filter).init();
    }
}
