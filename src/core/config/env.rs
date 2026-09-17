use std::env;

/// Resolved application configuration. Built once in `main.rs` via
/// `Config::from_env()` and shared through `AppState` — nothing downstream
/// reads environment variables directly.
#[derive(Debug, Clone)]
pub struct Config {
    /// An `sqlx` SQLite connection string, e.g. `sqlite://document_server.db`
    /// (relative to the working directory the binary is run from) or
    /// `sqlite:///absolute/path/to.db`. The file is created automatically
    /// if it doesn't exist — see `clients::sqlite::connect`.
    pub database_url: String,
    pub port: u16,
    /// Address the HTTP listener binds to. Defaults to `0.0.0.0` (all
    /// interfaces) for container deployments; the Tauri desktop bundle sets
    /// `127.0.0.1` to keep this service on loopback only.
    pub bind_addr: String,
    pub templates_dir: String,
    pub fonts_dir: String,
    pub max_render_body_bytes: usize,
    /// Shared secret every caller must present as `X-Internal-Api-Key` to
    /// reach the `render`/`documents` routes (see
    /// `core::middleware::auth::InternalCaller`). Unlike every other field
    /// on this struct, there is deliberately no default — this service has
    /// no other access control, so an accidentally-unset secret must fail
    /// startup, not silently boot wide open.
    pub internal_api_key: String,
    /// Largest remote image (a render payload's `logoUrl`) the server will
    /// download, in bytes. Default 5 MiB.
    pub remote_image_max_bytes: usize,
    /// Per-request timeout for that download, in seconds. Default 10.
    pub remote_image_timeout_secs: u64,
    /// Master switch for remote-image fetching. When `false`, a payload's
    /// `logoUrl` is ignored and the template renders with its built-in
    /// fallback. Default `true`.
    pub remote_image_fetch_enabled: bool,
}

/// Why startup configuration failed to load. `main.rs` logs this and exits
/// rather than letting the process start half-configured. Every field
/// except `internal_api_key` has a default (see `Config::from_env`), so
/// `Missing` only ever fires for that one; `Invalid` covers any value that
/// doesn't parse.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("missing required env var {0}")]
    Missing(&'static str),
    #[error("invalid value for env var {0}")]
    Invalid(&'static str),
}

impl Config {
    /// Parses and validates all required configuration once, at startup.
    /// Fails fast so a misconfigured deployment never reaches request-serving code.
    pub fn from_env() -> Result<Self, ConfigError> {
        // Defaulted, not required: unlike a networked database, a local
        // SQLite file needs no separate service to point at, so a fresh
        // checkout works with zero `.env` setup.
        let database_url =
            env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite://document_server.db".to_string());

        // Deliberately not 8080 (SimpleBash POS's backend default) — so this
        // service and that one can both run locally at once without a port
        // collision.
        let port = env::var("PORT")
            .unwrap_or_else(|_| "8090".to_string())
            .parse::<u16>()
            .map_err(|_| ConfigError::Invalid("PORT"))?;

        let bind_addr = env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0".to_string());

        let templates_dir = env::var("TEMPLATES_DIR").unwrap_or_else(|_| "templates".to_string());
        let fonts_dir = env::var("FONTS_DIR").unwrap_or_else(|_| "fonts".to_string());

        let max_render_body_bytes = env::var("MAX_RENDER_BODY_BYTES")
            .unwrap_or_else(|_| "5242880".to_string())
            .parse::<usize>()
            .map_err(|_| ConfigError::Invalid("MAX_RENDER_BODY_BYTES"))?;

        let internal_api_key =
            env::var("INTERNAL_API_KEY").map_err(|_| ConfigError::Missing("INTERNAL_API_KEY"))?;

        let remote_image_max_bytes = env::var("REMOTE_IMAGE_MAX_BYTES")
            .unwrap_or_else(|_| "5242880".to_string())
            .parse::<usize>()
            .map_err(|_| ConfigError::Invalid("REMOTE_IMAGE_MAX_BYTES"))?;

        let remote_image_timeout_secs = env::var("REMOTE_IMAGE_TIMEOUT_SECS")
            .unwrap_or_else(|_| "10".to_string())
            .parse::<u64>()
            .map_err(|_| ConfigError::Invalid("REMOTE_IMAGE_TIMEOUT_SECS"))?;

        let remote_image_fetch_enabled = env::var("REMOTE_IMAGE_FETCH_ENABLED")
            .unwrap_or_else(|_| "true".to_string())
            .parse::<bool>()
            .map_err(|_| ConfigError::Invalid("REMOTE_IMAGE_FETCH_ENABLED"))?;

        Ok(Self {
            database_url,
            port,
            bind_addr,
            templates_dir,
            fonts_dir,
            max_render_body_bytes,
            internal_api_key,
            remote_image_max_bytes,
            remote_image_timeout_secs,
            remote_image_fetch_enabled,
        })
    }
}
