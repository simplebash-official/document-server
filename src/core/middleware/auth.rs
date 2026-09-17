// The one piece of access control this service has: every caller of the
// `render`/`documents` routes must present the shared secret configured as
// `Config::internal_api_key` via the `X-Internal-Api-Key` header. This is
// deliberately not a general auth system (no users, no roles, no tokens) —
// SimpleBash POS's backend is the only intended caller, and this is what lets
// document-server tell "the backend" apart from "anyone on the network" (see
// spec's Auth section / CLAUDE.md's document-server integration notes).

use axum::{extract::FromRequestParts, http::request::Parts};

use crate::{app::AppState, core::constants::codes, core::error::AppError};

const HEADER_NAME: &str = "X-Internal-Api-Key";

/// Extracting this in a handler's argument list is what gates that route —
/// the same opt-in-per-handler convention SimpleBash POS's backend uses for
/// `CurrentUser`/`AdminUser`, just with one caller-class instead of many.
pub struct InternalCaller;

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
                Ok(InternalCaller)
            }
            _ => Err(AppError::unauthorized_with_code(
                format!("Missing or invalid {HEADER_NAME} header"),
                codes::INTERNAL_API_KEY_INVALID,
            )),
        }
    }
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
