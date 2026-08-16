// HTTP layer for the documents module. `GET /` (list), `GET /{key}` (single
// record), and `GET /{key}/pdf` (reprint) are all real as of Phase 3.
// Unlike `render`, there's no placeholder module-status stub here — the
// collection root is itself the real "list documents" endpoint, so there's
// no unused path left for one (same convention `templates` adopted in
// Phase 2).

use axum::{
    Json,
    body::Body,
    extract::{Path, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    app::AppState,
    core::{
        constants::modules,
        error::AppResult,
        response::{ApiResponse, ErrorResponse},
    },
    domain::documents::{Document, DocumentsResponse},
    modules::documents::service,
};

// ============================================================================
// Router
// ============================================================================

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_documents))
        .routes(routes!(get_document))
        .routes(routes!(reprint_document))
}

// ============================================================================
// Documents
// ============================================================================

#[utoipa::path(get, path = "/", tag = modules::DOCUMENTS, responses(
    (status = 200, description = "List rendered documents, newest first", body = ApiResponse<DocumentsResponse>)
))]
async fn list_documents(
    State(state): State<AppState>,
) -> AppResult<Json<ApiResponse<DocumentsResponse>>> {
    let response = service::list_documents(&state.db).await?;

    Ok(Json(ApiResponse::success(
        response,
        "Documents retrieved successfully",
    )))
}

#[utoipa::path(get, path = "/{key}", tag = modules::DOCUMENTS,
    params(("key" = String, Path, description = "The document's key (doc_...)")),
    responses(
        (status = 200, description = "Get a document record", body = ApiResponse<Document>),
        (status = 404, description = "Document not found", body = ErrorResponse),
    )
)]
async fn get_document(
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> AppResult<Json<ApiResponse<Document>>> {
    let document = service::get_document_by_key(&state.db, &key).await?;

    Ok(Json(ApiResponse::success(
        document,
        "Document retrieved successfully",
    )))
}

// Binary response, same deliberate envelope deviation as
// `POST /api/render/{templateKey}` (see spec §4 / CLAUDE.md) — a reprint's
// success payload is a PDF, not `Json<ApiResponse<_>>`. Its errors still go
// through the standard envelope.
#[utoipa::path(get, path = "/{key}/pdf", tag = modules::DOCUMENTS,
    params(("key" = String, Path, description = "The document's key (doc_...)")),
    responses(
        (status = 200, description = "Reprinted PDF", content_type = "application/pdf", body = Vec<u8>),
        (status = 404, description = "Document not found, or its template row no longer exists", body = ErrorResponse),
        (status = 422, description = "Stored data no longer satisfies the current template", body = ErrorResponse),
        (status = 500, description = "PDF export failed", body = ErrorResponse),
    )
)]
async fn reprint_document(
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> AppResult<Response> {
    let pdf_bytes = service::reprint_document(&state.db, &state.render, &key).await?;

    Ok((
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/pdf")],
        Body::from(pdf_bytes),
    )
        .into_response())
}
