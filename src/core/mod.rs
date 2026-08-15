// Cross-cutting infrastructure shared by every feature module: config
// loading, the `AppError`/`ApiResponse` envelopes, shared constants, and
// small request-handling utilities. Nothing in here is domain/business
// logic — that belongs in `modules::<name>::service`. Unlike jana2u-pos's
// backend, there is no `middleware` submodule here — this service has no
// authentication, idempotency, or sync-header concerns to gate requests on.

pub mod config;
pub mod constants;
pub mod error;
pub mod id;
pub mod openapi;
pub mod response;
pub mod utils;
