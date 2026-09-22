// The one piece of access control this service has: every caller of the
// `render`/`documents` routes must present the shared secret configured as
// `Config::internal_api_key` via the `X-Internal-Api-Key` header. This is
// deliberately not a general auth system (no users, no roles, no tokens) —
// SimpleBash POS's backend is the only intended caller, and this is what lets
// document-server tell "the backend" apart from "anyone on the network" (see
// spec's Auth section / CLAUDE.md's document-server integration notes).
//
// One caller-class, but potentially many TENANTS behind it: the cloud POS
// backend runs `TENANT_MODE=multi` with every tenant sharing one
// document-server instance, so `InternalCaller` also carries an optional
// `X-Tenant-Key` — see `TenantKey` below and CLAUDE.md's "Tenant scoping"
// section.

use axum::{extract::FromRequestParts, http::request::Parts};

use crate::{app::AppState, core::constants::codes, core::error::AppError};

const HEADER_NAME: &str = "X-Internal-Api-Key";
const TENANT_HEADER_NAME: &str = "X-Tenant-Key";
const MAX_TENANT_KEY_LEN: usize = 128;

/// Which tenant a request is scoped to. `Single` — no `X-Tenant-Key` header
/// at all — is what every desktop/self-hosted single-shop deployment sends
/// (forever; that caller has no concept of tenants), and stays the default
/// even in a multi-tenant deployment for a call that genuinely isn't
/// tenant-owned. `Tenant` is the cloud backend's `core::tenancy::
/// current_tenant_id()` value, forwarded verbatim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TenantKey {
    Single,
    Tenant(String),
}

impl TenantKey {
    /// The value stored in / matched against a `tenant_key` column.
    /// `Single` maps to `""`, not SQL `NULL` — SQLite's unique index treats
    /// every `NULL` as distinct from every other `NULL`, which would silently
    /// defeat the single-shop `(tenant_key, template_name, data_key)`
    /// dedup/overwrite behaviour that existed before tenants did.
    pub fn as_column(&self) -> &str {
        match self {
            TenantKey::Single => "",
            TenantKey::Tenant(key) => key,
        }
    }
}

/// Extracting this in a handler's argument list is what gates that route —
/// the same opt-in-per-handler convention SimpleBash POS's backend uses for
/// `CurrentUser`/`AdminUser`, just with one caller-class instead of many.
pub struct InternalCaller {
    pub tenant_key: TenantKey,
}

impl FromRequestParts<AppState> for InternalCaller {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let provided = parts
            .headers
            .get(HEADER_NAME)
            .and_then(|value| value.to_str().ok());

        match provided {
            Some(key)
                if constant_time_eq(key.as_bytes(), state.config.internal_api_key.as_bytes()) =>
            {
                // The tenant header is only meaningful once the caller has
                // already proven it's the backend — parsing it first would
                // hand an unauthenticated caller a format-validation oracle.
                Ok(InternalCaller {
                    tenant_key: parse_tenant_key(parts)?,
                })
            }
            _ => Err(AppError::unauthorized_with_code(
                format!("Missing or invalid {HEADER_NAME} header"),
                codes::INTERNAL_API_KEY_INVALID,
            )),
        }
    }
}

/// Absent or blank header -> `Single`; otherwise the trimmed value if it
/// looks like an id (`core::id::generate_id`-shaped: ASCII alphanumeric,
/// `_`/`-`, bounded length) — a malformed value is rejected outright rather
/// than silently folded into `Single`, since that would let a typo'd tenant
/// header quietly leak into (or read out of) the single-shop bucket.
fn parse_tenant_key(parts: &Parts) -> Result<TenantKey, AppError> {
    let Some(raw) = parts
        .headers
        .get(TENANT_HEADER_NAME)
        .and_then(|value| value.to_str().ok())
    else {
        return Ok(TenantKey::Single);
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(TenantKey::Single);
    }
    let looks_like_an_id = trimmed.len() <= MAX_TENANT_KEY_LEN
        && trimmed
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if !looks_like_an_id {
        return Err(AppError::validation_with_code(
            format!(
                "{TENANT_HEADER_NAME} must be alphanumeric/underscore/hyphen, at most {MAX_TENANT_KEY_LEN} characters"
            ),
            codes::TENANT_KEY_INVALID,
        ));
    }
    Ok(TenantKey::Tenant(trimmed.to_string()))
}

/// Manual constant-time byte comparison — avoids pulling in a dedicated
/// crate (e.g. `subtle`) for one comparison. Always walks the full length of
/// the shorter input against a same-length probe rather than short-circuiting
/// on the first mismatch, so response timing doesn't leak how many leading
/// bytes of a guessed key were correct.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}
