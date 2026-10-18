// Full-stack, real SQLite (throwaway per-test file via `common::spawn_app`).
// Covers `GET /api/templates` / `GET /api/templates/{key}`: what each tenant
// sees, and the contract guards over the bundled schemas. Publishing and
// granting templates is covered in `packages_test.rs`.

mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

async fn get(
    app: &common::TestApp,
    uri: &str,
    tenant: Option<&str>,
) -> (StatusCode, serde_json::Value) {
    let mut request = Request::builder()
        .uri(uri)
        .header("X-Internal-Api-Key", &app.config.internal_api_key);
    if let Some(tenant) = tenant {
        request = request.header("X-Tenant-Key", tenant);
    }
    let response = app
        .router
        .clone()
        .oneshot(request.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, serde_json::from_slice(&body).unwrap_or_default())
}

#[tokio::test]
async fn list_templates_returns_every_bundled_template_with_type_and_expected_data() {
    let app = common::spawn_app().await;
    let (status, json) = get(&app, "/api/templates", None).await;
    assert_eq!(status, StatusCode::OK);

    let templates = json["data"]["templates"].as_array().unwrap();
    // Names are opaque generated ids (`doc_temp_…` / `lbl_temp_…`); a
    // template is identified by its `description` (the schema `title`).
    let mut descriptions: Vec<&str> = templates
        .iter()
        .map(|t| t["description"].as_str().unwrap())
        .collect();
    descriptions.sort_unstable();
    assert_eq!(
        descriptions,
        vec![
            "A4 Invoice",
            "A4 Invoice Modern",
            "Analytics Report",
            "Credit Note",
            "Product Sticker Label",
            "Quotation",
            "Thermal Receipt",
        ]
    );
    assert!(templates.iter().all(|t| t["status"] == "published"));
    assert!(templates.iter().all(|t| t["scope"] == "global"));
    assert!(templates.iter().all(|t| t["version"] == 1));
    assert!(templates.iter().all(|t| t["family"] == t["name"]));
    assert!(templates.iter().all(|t| {
        let name = t["name"].as_str().unwrap();
        name.starts_with("doc_temp_") || name.starts_with("lbl_temp_")
    }));
    assert!(
        templates
            .iter()
            .all(|t| t["options"].as_array().unwrap().is_empty())
    );

    let by_desc = |d: &str| -> serde_json::Value {
        templates
            .iter()
            .find(|t| t["description"] == d)
            .unwrap()
            .clone()
    };

    let invoice_tpl = by_desc("A4 Invoice");
    assert_eq!(invoice_tpl["type"], "document");
    assert_eq!(invoice_tpl["kind"], "a4-invoice");
    assert_eq!(invoice_tpl["isDefault"], true);
    assert_eq!(by_desc("A4 Invoice Modern")["isDefault"], false);
    assert_eq!(invoice_tpl["data"]["invoiceNumber"], "INV-000123");
    assert!(invoice_tpl["data"]["items"].is_array());
    assert_eq!(invoice_tpl["data"]["totalCents"], 1100000);
    // The machine-readable input contract, parsed from the `.schema.json`
    // sidecar and exposed alongside the sample data.
    let required = invoice_tpl["dataSchema"]["required"].as_array().unwrap();
    assert!(required.iter().any(|r| r == "invoiceNumber"));
    assert!(required.iter().any(|r| r == "totalCents"));

    let credit_note_tpl = by_desc("Credit Note");
    assert_eq!(credit_note_tpl["type"], "document");
    assert_eq!(credit_note_tpl["data"]["creditNoteNumber"], "CN-000042");
    assert!(credit_note_tpl["data"]["items"].is_array());
    assert_eq!(credit_note_tpl["data"]["refundCashCents"], 700000);

    let receipt_tpl = by_desc("Thermal Receipt");
    assert_eq!(receipt_tpl["type"], "document");
    assert_eq!(receipt_tpl["data"]["invoiceNumber"], "INV-000123");
    assert!(receipt_tpl["data"]["items"].is_array());
    assert_eq!(receipt_tpl["data"]["totalCents"], 1100000);
    assert_eq!(receipt_tpl["data"]["paperWidthMm"], 80);

    let sticker_tpl = by_desc("Product Sticker Label");
    assert_eq!(sticker_tpl["type"], "label");
    assert_eq!(sticker_tpl["kind"], "product-label");
    assert_eq!(sticker_tpl["data"]["shopName"], "SimpleBash Mobile");
    assert_eq!(sticker_tpl["data"]["widthMm"], 50);
    let required = sticker_tpl["dataSchema"]["required"].as_array().unwrap();
    for field in ["widthMm", "heightMm", "shopName", "elements", "labels"] {
        assert!(required.iter().any(|r| r == field), "{field} is required");
    }
}

#[tokio::test]
async fn list_templates_filters_by_type() {
    let app = common::spawn_app().await;

    let (status, json) = get(&app, "/api/templates?type=document", None).await;
    assert_eq!(status, StatusCode::OK);
    let templates = json["data"]["templates"].as_array().unwrap();
    assert_eq!(templates.len(), 6);
    assert!(templates.iter().all(|t| t["type"] == "document"));

    let (status, json) = get(&app, "/api/templates?type=label", None).await;
    assert_eq!(status, StatusCode::OK);
    let templates = json["data"]["templates"].as_array().unwrap();
    assert_eq!(templates.len(), 1);
    assert_eq!(templates[0]["description"], "Product Sticker Label");
    assert!(
        templates[0]["name"]
            .as_str()
            .unwrap()
            .starts_with("lbl_temp_")
    );
    assert_eq!(templates[0]["type"], "label");
}

#[tokio::test]
async fn an_unknown_type_filter_is_rejected_not_ignored() {
    let app = common::spawn_app().await;
    let (status, _) = get(&app, "/api/templates?type=poster", None).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn templates_require_the_internal_api_key() {
    let app = common::spawn_app().await;
    for uri in ["/api/templates", "/api/templates/tpl_x"] {
        let response = app
            .router
            .clone()
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{uri}");
    }
}

#[tokio::test]
async fn get_template_by_key_returns_that_template_with_data() {
    let app = common::spawn_app().await;

    let sticker_key: String = sqlx::query_scalar("SELECT key FROM templates WHERE description = ?")
        .bind("Product Sticker Label")
        .fetch_one(&app.db)
        .await
        .expect("sticker template should be synced from disk by spawn_app");

    let (status, json) = get(&app, &format!("/api/templates/{sticker_key}"), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["data"]["key"], sticker_key);
    assert_eq!(json["data"]["description"], "Product Sticker Label");
    assert!(
        json["data"]["name"]
            .as_str()
            .unwrap()
            .starts_with("lbl_temp_")
    );
    assert_eq!(json["data"]["type"], "label");
    assert_eq!(json["data"]["data"]["shopName"], "SimpleBash Mobile");
    // Single-template fetch exposes the schema contract too — this is what
    // an integration client reads to learn how to feed a render.
    let required = json["data"]["dataSchema"]["required"].as_array().unwrap();
    assert_eq!(required.len(), 5);
}

#[tokio::test]
async fn get_template_unknown_key_returns_404() {
    let app = common::spawn_app().await;
    let (status, json) = get(&app, "/api/templates/tpl_does_not_exist", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(json["code"], "TEMPLATE_NOT_FOUND");
}

#[tokio::test]
async fn a_retired_bundled_template_is_hidden_from_lists_and_lookups() {
    let app = common::spawn_app().await;
    let (key, name): (String, String) =
        sqlx::query_as("SELECT key, name FROM templates WHERE description = 'Quotation'")
            .fetch_one(&app.db)
            .await
            .unwrap();
    sqlx::query("UPDATE templates SET status = 'retired' WHERE key = ?")
        .bind(&key)
        .execute(&app.db)
        .await
        .unwrap();

    let (_, json) = get(&app, "/api/templates", None).await;
    assert!(
        json["data"]["templates"]
            .as_array()
            .unwrap()
            .iter()
            .all(|t| t["name"] != name.as_str())
    );
    let (status, _) = get(&app, &format!("/api/templates/{key}"), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

// The backend feeds both A4 designs from one payload builder, so their input
// contracts must stay identical — only the human `title`/`description` (and which one is the default) differ.
#[test]
fn a4_invoice_modern_schema_matches_a4_invoice_schema() {
    let load = |name: &str| -> serde_json::Value {
        let path = format!("templates/documents/{name}.schema.json");
        let mut schema: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).expect(&path)).unwrap();
        let obj = schema.as_object_mut().unwrap();
        obj.remove("title");
        obj.remove("description");
        obj.remove("x-default");
        schema
    };
    assert_eq!(
        load("doc_temp_ZahcoYg8xCQjuc8c"),
        load("doc_temp_vEf0Y7jQHQj2rIuO")
    );
}

// ---------------------------------------------------------------------------
// Template contract guards. The POS backend builds its document dropdowns from
// each schema's `x-document-kind`, finds templates by title and renders their
// samples as previews, so these three facts must hold for every template.
// ---------------------------------------------------------------------------

/// `(subdir, file stem, parsed schema, parsed sample)` for every template that
/// ships a `.schema.json`.
fn schema_sidecars() -> Vec<(
    &'static str,
    String,
    serde_json::Value,
    Option<serde_json::Value>,
)> {
    let mut found = Vec::new();
    for subdir in ["documents", "labels"] {
        let dir = format!("templates/{subdir}");
        for entry in std::fs::read_dir(&dir).expect("templates dir") {
            let path = entry.unwrap().path();
            let file = path.file_name().unwrap().to_string_lossy().to_string();
            let Some(stem) = file.strip_suffix(".schema.json") else {
                continue;
            };
            let schema = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
            let sample = std::fs::read_to_string(format!("{dir}/{stem}.json"))
                .ok()
                .map(|s| serde_json::from_str(&s).unwrap());
            found.push((subdir, stem.to_string(), schema, sample));
        }
    }
    found
}

#[test]
fn every_bundled_schema_passes_the_template_schema_rules() {
    for (_, stem, schema, _) in schema_sidecars() {
        document_server::domain::templates::validate_template_schema(&schema)
            .unwrap_or_else(|err| panic!("{stem}.schema.json: {err}"));
    }
}

#[test]
fn every_kind_has_exactly_one_default_template() {
    let mut defaults: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    for (_, _, schema, _) in schema_sidecars() {
        let kind = schema["x-document-kind"].as_str().unwrap().to_string();
        let entry = defaults.entry(kind).or_insert(0);
        if schema["x-default"] == true {
            *entry += 1;
        }
    }
    for (kind, count) in defaults {
        assert_eq!(count, 1, "kind {kind} needs exactly one x-default template");
    }
}

#[test]
fn template_titles_are_unique() {
    let mut seen = std::collections::HashMap::new();
    for (_, stem, schema, _) in schema_sidecars() {
        let title = schema["title"].as_str().expect("schema title").to_string();
        if let Some(other) = seen.insert(title.clone(), stem.clone()) {
            panic!("title {title:?} is used by both {other} and {stem}; titles identify templates");
        }
    }
}

#[test]
fn every_sample_validates_against_its_own_schema() {
    for (_, stem, schema, sample) in schema_sidecars() {
        let Some(sample) = sample else {
            panic!("{stem} has no .json sample")
        };
        let validator = jsonschema::validator_for(&schema).expect("schema compiles");
        let errors: Vec<String> = validator
            .iter_errors(&sample)
            .map(|e| format!("{} at {}", e, e.instance_path()))
            .collect();
        assert!(
            errors.is_empty(),
            "{stem}.json does not satisfy {stem}.schema.json: {errors:?}"
        );
    }
}
