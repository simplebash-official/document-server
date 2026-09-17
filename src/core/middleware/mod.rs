// The only middleware/extractor concern this service has: the internal
// caller check (see `auth::InternalCaller`). Unlike MyroLogic POS's backend,
// there's no idempotency or sync-header middleware here — nothing in this
// service's domain needs either.
pub mod auth;
