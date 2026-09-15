use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};

use crate::core::response::ErrorResponse;

/// The single error type every handler's `AppResult<T>` returns through.
/// Four named variants (`NotFound`, `Validation`, `Internal`, `Unauthorized`)
/// cover the common HTTP statuses and default to a generic error `code` (see
/// `status_code_code_and_message`) when none is given via the `*_with_code`
/// constructors; `Custom` exists as an escape hatch for statuses/codes that
/// don't fit those (e.g. 422 Unprocessable Entity, used by `modules::render`
/// for a Typst compile failure — see `unprocessable_entity`). `Forbidden` is
/// still dropped, since nothing in this service constructs it — there is no
/// per-caller permission concept, only the single internal-caller secret
/// checked by `core::middleware::auth::InternalCaller`, which is what
/// constructs `Unauthorized`. Implements `IntoResponse` directly, so
/// `?`-propagating one of these from a handler is enough to produce the
/// right HTTP response — no separate mapping step.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{message}")]
    NotFound {
        message: String,
        code: Option<String>,
    },

    #[error("{message}")]
    Validation {
        message: String,
        code: Option<String>,
    },

    #[error("{message}")]
    Internal {
        message: String,
        code: Option<String>,
    },

    #[error("{message}")]
    Unauthorized {
        message: String,
        code: Option<String>,
    },

    #[error("{message}")]
    Custom {
        status: StatusCode,
        code: String,
        message: String,
        details: Option<serde_json::Value>,
    },
}

impl AppError {
    // Each of the three named variants gets a plain constructor (uses the
    // variant name upper-cased as the default `code`, e.g. `NOT_FOUND`) and
    // a `_with_code` constructor (for a module-specific code like
    // `TEMPLATE_NOT_FOUND`). Handlers reach for the plain form unless a
    // caller needs to distinguish this particular failure by `code` in the
    // response JSON.
    pub fn not_found(message: impl Into<String>) -> Self {
        AppError::NotFound {
            message: message.into(),
            code: None,
        }
    }

    pub fn not_found_with_code(message: impl Into<String>, code: impl Into<String>) -> Self {
        AppError::NotFound {
            message: message.into(),
            code: Some(code.into()),
        }
    }

    pub fn validation(message: impl Into<String>) -> Self {
        AppError::Validation {
            message: message.into(),
            code: None,
        }
    }

    pub fn validation_with_code(message: impl Into<String>, code: impl Into<String>) -> Self {
        AppError::Validation {
            message: message.into(),
            code: Some(code.into()),
        }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        AppError::Internal {
            message: message.into(),
            code: None,
        }
    }

    pub fn internal_with_code(message: impl Into<String>, code: impl Into<String>) -> Self {
        AppError::Internal {
            message: message.into(),
            code: Some(code.into()),
        }
    }

    pub fn unauthorized(message: impl Into<String>) -> Self {
        AppError::Unauthorized {
            message: message.into(),
            code: None,
        }
    }

    pub fn unauthorized_with_code(message: impl Into<String>, code: impl Into<String>) -> Self {
        AppError::Unauthorized {
            message: message.into(),
            code: Some(code.into()),
        }
    }

    /// Escape hatch for any (status, code, message) combination the three
    /// named variants don't cover.
    pub fn custom(status: StatusCode, code: impl Into<String>, message: impl Into<String>) -> Self {
        AppError::Custom {
            status,
            code: code.into(),
            message: message.into(),
            details: None,
        }
    }

    pub fn custom_with_details(
        status: StatusCode,
        code: impl Into<String>,
        message: impl Into<String>,
        details: serde_json::Value,
    ) -> Self {
        AppError::Custom {
            status,
            code: code.into(),
            message: message.into(),
            details: Some(details),
        }
    }

    /// 422 Unprocessable Entity — used specifically for a render request
    /// whose data fails Typst compilation (see spec §4/§6.2: "Compile
    /// errors → 422"). Kept as its own `Custom`-backed helper rather than
    /// changing `Validation`'s default status mapping, so `Validation`
    /// keeps its normal 400 semantics for any future genuine bad-request
    /// case.
    pub fn unprocessable_entity(code: impl Into<String>, message: impl Into<String>) -> Self {
        AppError::custom(StatusCode::UNPROCESSABLE_ENTITY, code, message)
    }

    /// The single place a variant maps to its wire representation. Centralizing
    /// this (rather than matching in `IntoResponse` directly) keeps the
    /// default-code-per-variant logic in one spot instead of duplicated
    /// wherever an `AppError` needs to be inspected outside of a response.
    pub fn status_code_code_message_and_details(
        &self,
    ) -> (StatusCode, String, String, Option<serde_json::Value>) {
        match self {
            AppError::NotFound { message, code } => (
                StatusCode::NOT_FOUND,
                code.clone()
                    .unwrap_or_else(|| crate::core::constants::codes::NOT_FOUND.to_string()),
                message.clone(),
                None,
            ),
            AppError::Validation { message, code } => (
                StatusCode::BAD_REQUEST,
                code.clone()
                    .unwrap_or_else(|| crate::core::constants::codes::VALIDATION_ERROR.to_string()),
                message.clone(),
                None,
            ),
            AppError::Internal { message, code } => (
                StatusCode::INTERNAL_SERVER_ERROR,
                code.clone().unwrap_or_else(|| {
                    crate::core::constants::codes::INTERNAL_SERVER_ERROR.to_string()
                }),
                message.clone(),
                None,
            ),
            AppError::Unauthorized { message, code } => (
                StatusCode::UNAUTHORIZED,
                code.clone()
                    .unwrap_or_else(|| crate::core::constants::codes::UNAUTHORIZED.to_string()),
                message.clone(),
                None,
            ),
            AppError::Custom {
                status,
                code,
                message,
                details,
            } => (*status, code.clone(), message.clone(), details.clone()),
        }
    }

    pub fn status_code_code_and_message(&self) -> (StatusCode, String, String) {
        let (status, code, message, _) = self.status_code_code_message_and_details();
        (status, code, message)
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code, message, details) = self.status_code_code_message_and_details();

        // 5xx failures are unexpected (SQLite unavailable, PDF export bug) — log
        // them server-side since the client only sees the generic message,
        // not the internal detail that ended up in `message` here.
        if status.is_server_error() {
            tracing::error!(code = %code, error = %message, "internal server error");
        } else {
            // 4xx (bad payload, unknown template, missing key) is still part
            // of what happened to a render.
            tracing::warn!(
                category = "error",
                event = "client_error",
                status = status.as_u16(),
                code = %code,
                error = %message,
                "request rejected"
            );
        }

        let body = ErrorResponse {
            success: false,
            message,
            code,
            status_code: status.as_u16(),
            details,
        };

        (status, Json(body)).into_response()
    }
}

// Lets handlers `?`-propagate a SQLite driver error directly into an
// `AppResult` instead of matching on it at every call site. Collapsed to
// `Internal` because a raw driver error (connection issue, query error) is
// never something the caller can act on — it's always a 500.
impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        tracing::error!(category = "db", event = "error", error = %err, "SQLite error");
        AppError::internal(err.to_string())
    }
}

// Same idea for JSON serialization of a value we constructed ourselves. A
// failure here is a bug in our own types, never bad caller input.
impl From<serde_json::Error> for AppError {
    fn from(err: serde_json::Error) -> Self {
        tracing::error!(category = "error", event = "serialization", error = %err, "JSON serialization error");
        AppError::internal(err.to_string())
    }
}

/// Handler and service-layer return type: every fallible operation in this
/// codebase resolves to either a domain value or an `AppError` that already
/// knows how to render itself as the right HTTP response.
pub type AppResult<T> = Result<T, AppError>;
