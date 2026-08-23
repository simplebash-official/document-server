// Business rules for stored template data: CRUD over the per-template
// blobs, plus the render-time deep merge that makes them matter. The blobs
// are shared/static input (shop profile, bank details, branding) that
// clients would otherwise have to resend — and duplicate across projects —
// on every render.

use sqlx::SqlitePool;

use crate::{
    core::{
        constants::{codes, prefixes},
        error::{AppError, AppResult},
        id::generate_id,
    },
    domain::template_data::{TemplateData, TemplateDataListResponse},
    modules::{template_data::repository, templates},
};

/// Stores (or replaces) one blob for `template_name`. The template must
/// exist first — a typo'd name should fail loudly here rather than silently
/// create an orphan blob no render will ever read. Matched by *name* (any
/// active state): a deactivated template's stored data stays editable, same
/// as its history stays reprintable.
pub(crate) async fn set_template_data(
    db: &SqlitePool,
    template_name: &str,
    data_key: &str,
    data: serde_json::Value,
) -> AppResult<TemplateData> {
    templates::service::get_template_by_name(db, template_name)
        .await
        .map_err(|_| {
            AppError::not_found_with_code(
                format!("Template '{template_name}' not found"),
                codes::TEMPLATE_NOT_FOUND,
            )
        })?;

    let row = repository::upsert(
        db,
        generate_id(prefixes::TEMPLATE_DATA),
        template_name,
        data_key,
        &data,
    )
    .await?;

    Ok(row.into_template_data())
}

/// Fetches one blob by `(template_name, data_key)` — 404 with
/// `TEMPLATE_DATA_NOT_FOUND` if absent.
pub(crate) async fn get_template_data(
    db: &SqlitePool,
    template_name: &str,
    data_key: &str,
) -> AppResult<TemplateData> {
    repository::find_by_template_and_key(db, template_name, data_key)
        .await?
        .map(|row| row.into_template_data())
        .ok_or_else(|| {
            AppError::not_found_with_code(
                format!("No data key '{data_key}' for template '{template_name}'"),
                codes::TEMPLATE_DATA_NOT_FOUND,
            )
        })
}

/// Every blob stored for one template — backs
/// `GET /api/template-data/{templateName}`.
pub(crate) async fn list_template_data(
    db: &SqlitePool,
    template_name: &str,
) -> AppResult<TemplateDataListResponse> {
    let rows = repository::list_by_template(db, template_name).await?;
    Ok(TemplateDataListResponse {
        items: rows.into_iter().map(|row| row.into_template_data()).collect(),
    })
}

/// Removes one blob and returns it — 404 `TEMPLATE_DATA_NOT_FOUND` if there
/// was nothing to delete (delete-of-absent is a caller mistake here, not a
/// success). Returning the row doubles as the OpenAPI-friendly payload and
/// confirms *what* was removed, which a bare ok cannot.
pub(crate) async fn delete_template_data(
    db: &SqlitePool,
    template_name: &str,
    data_key: &str,
) -> AppResult<TemplateData> {
    let existing = get_template_data(db, template_name, data_key).await?;
    repository::delete_by_template_and_key(db, template_name, data_key).await?;

    Ok(existing)
}

/// Deep-merges every blob stored for `template_name` under `payload`,
/// returning the combined render input. Merge order is deterministic
/// (`data_key` ASC), and the request payload wins over any stored field —
/// so a caller can always override per-request what the server stores as a
/// default (this is what keeps jana2u-pos's per-invoice shop snapshots
/// authoritative over anything stored here).
///
/// Objects merge recursively; arrays and scalars replace wholesale. An
/// empty stored set returns the payload untouched — zero overhead and
/// exactly the pre-feature behavior for templates without stored data.
pub(crate) async fn merge_into_payload(
    db: &SqlitePool,
    template_name: &str,
    payload: serde_json::Value,
) -> AppResult<serde_json::Value> {
    let rows = repository::list_by_template(db, template_name).await?;
    if rows.is_empty() {
        return Ok(payload);
    }

    let mut merged = serde_json::Value::Object(serde_json::Map::new());
    for row in rows {
        deep_merge(&mut merged, &row.data.0);
    }
    deep_merge(&mut merged, &payload);

    Ok(merged)
}

fn deep_merge(base: &mut serde_json::Value, overlay: &serde_json::Value) {
    match (base, overlay) {
        (serde_json::Value::Object(base_map), serde_json::Value::Object(overlay_map)) => {
            for (key, value) in overlay_map {
                match base_map.get_mut(key) {
                    Some(existing) => deep_merge(existing, value),
                    None => {
                        base_map.insert(key.clone(), value.clone());
                    }
                }
            }
        }
        // Arrays and scalars: the later value wins wholesale.
        (base, overlay) => *base = overlay.clone(),
    }
}
