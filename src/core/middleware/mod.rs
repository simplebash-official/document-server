// The only middleware/extractor concern this service has: the internal
// caller check (see `auth::InternalCaller`). Unlike jana2u-pos's backend,
// there's no idempotency or sync-header middleware here — nothing in this
// service's domain needs either.
pub mod auth;
