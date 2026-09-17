// Cross-cutting infrastructure shared by every feature module: config
// loading, the `AppError`/`ApiResponse` envelopes, shared constants, and
// small request-handling utilities. Nothing in here is domain/business
// logic — that belongs in `modules::<name>::service`. Unlike SimpleBash POS's
// backend, there is no idempotency or sync-header middleware here — this
// service's `middleware` submodule holds exactly one thing, the internal
// caller check (see `middleware::auth::InternalCaller`). Request logging
// lives in `logging`.

pub mod config;
pub mod constants;
pub mod error;
pub mod id;
pub mod logging;
pub mod middleware;
pub mod openapi;
pub mod response;
pub mod utils;
