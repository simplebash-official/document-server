// Startup configuration, loaded once from environment variables. Split into
// a private `env` submodule so this file stays a stable public surface
// (`Config`, `ConfigError`) even if how the values are parsed changes.
mod env;

pub use env::{Config, ConfigError};
