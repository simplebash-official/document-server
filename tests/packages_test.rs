// Full-stack tests for `/api/packages`: publish, versioning, grants,
// tenant visibility, retire, archive export and install. Each test gets a
// private writable copy of `templates/` so packages land in a temp tree.

mod common;

use std::io::Write;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

const KEY_HEADER: &str = "X-Internal-Api-Key";

struct Pkg {
    manifest: String,
    typ: String,
    schema: serde_json::Value,
    sample: serde_json::Value,
    assets: Vec<(&'static str, Vec<u8>)>,
}

impl Pkg {
    /// A one-page design; `width_mm` makes versions tell apart in the PDF.
    fn new(title: &str, width_mm: u32) -> Self {
        Pkg {
            manifest: r#"{"format":1,"type":"document","minAppVersion":null,"name":null,"family":null,"version":null,"createdAt":null}"#.into(),
            typ: format!(
                "#import sys: inputs\n#set page(width: {width_mm}mm, height: 50mm)\n= #inputs.title\n"
            ),
            schema: serde_json::json!({
                "$schema": "http://json-schema.org/draft-07/schema#",
                "title": title,
                "x-document-kind": "a4-invoice",
                "type": "object",
                "required": ["title"],
                "properties": { "title": { "type": "string" } }
            }),
            sample: serde_json::json!({ "title": "Hello" }),
            assets: vec![],
        }
    }

    fn zip(&self) -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default();
        let mut put = |name: &str, data: &[u8]| {
            writer.start_file(name, options).unwrap();
            writer.write_all(data).unwrap();
        };
        put("manifest.json", self.manifest.as_bytes());
        put("template.typ", self.typ.as_bytes());
        put("schema.json", self.schema.to_string().as_bytes());
        put("sample.json", self.sample.to_string().as_bytes());
        for (name, data) in &self.assets {
            put(&format!("assets/{name}"), data);
        }
        writer.finish().unwrap().into_inner()
    }
}

fn request(
    app: &common::TestApp,
    method: &str,
    uri: &str,
    tenant: Option<&str>,
    body: Vec<u8>,
    content_type: &str,
) -> Request<Body> {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(KEY_HEADER, &app.config.internal_api_key)
        .header("content-type", content_type);
    if let Some(tenant) = tenant {
        builder = builder.header("X-Tenant-Key", tenant);
    }
    builder.body(Body::from(body)).unwrap()
}

async fn send(app: &common::TestApp, req: Request<Body>) -> (StatusCode, Vec<u8>) {
    let response = app.router.clone().oneshot(req).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap()
        .to_vec();
    (status, bytes)
}

async fn json(app: &common::TestApp, req: Request<Body>) -> (StatusCode, serde_json::Value) {
    let (status, bytes) = send(app, req).await;
    (status, serde_json::from_slice(&bytes).unwrap_or_default())
}

async fn publish(
    app: &common::TestApp,
    pkg: &Pkg,
    family: Option<&str>,
) -> (StatusCode, serde_json::Value) {
    let uri = match family {
        Some(family) => format!("/api/packages?family={family}"),
        None => "/api/packages".to_string(),
    };
    json(
        app,
        request(app, "POST", &uri, None, pkg.zip(), "application/zip"),
    )
    .await
}

async fn grant(app: &common::TestApp, family: &str, tenants: &[&str]) -> StatusCode {
    let body = serde_json::json!({ "tenantKeys": tenants })
        .to_string()
        .into_bytes();
    send(
        app,
        request(
            app,
            "PUT",
            &format!("/api/packages/families/{family}/grants"),
            None,
            body,
            "application/json",
        ),
    )
    .await
    .0
}

async fn templates_for(app: &common::TestApp, tenant: Option<&str>) -> Vec<serde_json::Value> {
    let (_, body) = json(
        app,
        request(
            app,
            "GET",
            "/api/templates",
            tenant,
            vec![],
            "application/json",
        ),
    )
    .await;
    body["data"]["templates"].as_array().unwrap().clone()
}

async fn render(app: &common::TestApp, key: &str, tenant: Option<&str>) -> (StatusCode, Vec<u8>) {
    let body = serde_json::json!({ "title": "Hi" })
        .to_string()
        .into_bytes();
    send(
        app,
        request(
            app,
            "POST",
            &format!("/api/render/{key}"),
            tenant,
            body,
            "application/json",
        ),
    )
    .await
}

#[tokio::test]
async fn packages_require_the_internal_api_key() {
    let app = common::spawn_app_isolated_templates().await;
    for (method, uri) in [
        ("GET", "/api/packages"),
        ("POST", "/api/packages"),
        ("GET", "/api/packages/doc_temp_aaaaaaaaaaaaaaaa"),
        ("GET", "/api/packages/doc_temp_aaaaaaaaaaaaaaaa/archive"),
        ("PUT", "/api/packages/families/fam_aaaaaaaaaaaaaaaa/grants"),
        ("POST", "/api/packages/families/fam_aaaaaaaaaaaaaaaa/retire"),
    ] {
        let response = app
            .router
            .clone()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(uri)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "{method} {uri}"
        );
    }
}

#[tokio::test]
async fn a_published_package_is_private_until_its_family_is_granted() {
    let app = common::spawn_app_isolated_templates().await;
    let (status, body) = publish(&app, &Pkg::new("Client Invoice", 100), None).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let data = &body["data"];
    let (name, family, key) = (
        data["name"].as_str().unwrap().to_string(),
        data["family"].as_str().unwrap().to_string(),
        data["key"].as_str().unwrap().to_string(),
    );
    assert!(name.starts_with("doc_temp_"));
    assert!(family.starts_with("fam_"));
    assert_eq!(
        (
            data["version"].as_i64(),
            data["scope"].as_str(),
            data["status"].as_str()
        ),
        (Some(1), Some("private"), Some("published"))
    );
    assert_eq!(data["kind"], "a4-invoice");
    assert_eq!(data["isDefault"], false);
    assert_eq!(data["grants"], serde_json::json!([]));
    let root = app
        .templates_dir
        .as_ref()
        .unwrap()
        .path()
        .join("packages")
        .join(&name);
    for file in ["template.typ", "schema.json", "sample.json", "package.zip"] {
        assert!(root.join(file).exists(), "{file}");
    }

    // Nobody sees it yet.
    for tenant in [None, Some("tnt_a"), Some("tnt_b")] {
        assert!(
            templates_for(&app, tenant)
                .await
                .iter()
                .all(|t| t["name"] != name.as_str())
        );
        assert_eq!(render(&app, &key, tenant).await.0, StatusCode::NOT_FOUND);
    }

    assert_eq!(
        grant(&app, &family, &["tnt_a"]).await,
        StatusCode::NO_CONTENT
    );
    assert!(
        templates_for(&app, Some("tnt_a"))
            .await
            .iter()
            .any(|t| t["name"] == name.as_str())
    );
    assert!(
        templates_for(&app, Some("tnt_b"))
            .await
            .iter()
            .all(|t| t["name"] != name.as_str())
    );
    assert!(
        templates_for(&app, None)
            .await
            .iter()
            .all(|t| t["name"] != name.as_str())
    );

    let (status, pdf) = render(&app, &key, Some("tnt_a")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(pdf.starts_with(b"%PDF-"));
    assert_eq!(
        render(&app, &key, Some("tnt_b")).await.0,
        StatusCode::NOT_FOUND
    );

    // Detail view carries the grants; a single lookup is tenant scoped too.
    let (_, detail) = json(
        &app,
        request(
            &app,
            "GET",
            &format!("/api/packages/{name}"),
            None,
            vec![],
            "application/json",
        ),
    )
    .await;
    assert_eq!(detail["data"]["grants"], serde_json::json!(["tnt_a"]));
    let (status, _) = json(
        &app,
        request(
            &app,
            "GET",
            &format!("/api/templates/{key}"),
            Some("tnt_b"),
            vec![],
            "application/json",
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // Revoking removes access again.
    assert_eq!(grant(&app, &family, &[]).await, StatusCode::NO_CONTENT);
    assert_eq!(
        render(&app, &key, Some("tnt_a")).await.0,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn a_new_version_replaces_the_old_in_lists_but_old_documents_keep_their_design() {
    let app = common::spawn_app_isolated_templates().await;
    let (_, v1) = publish(&app, &Pkg::new("Client Invoice", 100), None).await;
    let family = v1["data"]["family"].as_str().unwrap().to_string();
    let key1 = v1["data"]["key"].as_str().unwrap().to_string();
    grant(&app, &family, &["tnt_a"]).await;

    // Render one document with v1 and remember its key.
    let (status, pdf1) = render(&app, &key1, Some("tnt_a")).await;
    assert_eq!(status, StatusCode::OK);
    let (_, docs) = json(
        &app,
        request(
            &app,
            "GET",
            "/api/documents",
            Some("tnt_a"),
            vec![],
            "application/json",
        ),
    )
    .await;
    let doc_key = docs["data"]["documents"][0]["key"]
        .as_str()
        .unwrap()
        .to_string();

    // Version 2 is a wider page, same family.
    let (status, v2) = publish(&app, &Pkg::new("Client Invoice", 200), Some(&family)).await;
    assert_eq!(status, StatusCode::CREATED, "{v2}");
    assert_eq!(v2["data"]["version"], 2);
    assert_eq!(v2["data"]["family"], family.as_str());
    let key2 = v2["data"]["key"].as_str().unwrap().to_string();

    // The list shows only the latest version of the family, with its grants.
    let listed: Vec<_> = templates_for(&app, Some("tnt_a"))
        .await
        .into_iter()
        .filter(|t| t["family"] == family.as_str())
        .collect();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0]["version"], 2);
    let (_, detail) = json(
        &app,
        request(
            &app,
            "GET",
            &format!("/api/packages/{}", v2["data"]["name"].as_str().unwrap()),
            None,
            vec![],
            "application/json",
        ),
    )
    .await;
    assert_eq!(detail["data"]["grants"], serde_json::json!(["tnt_a"]));

    // Rendering is deterministic, so identical bytes mean the same design.
    let (_, pdf2) = render(&app, &key2, Some("tnt_a")).await;
    assert_ne!(pdf1, pdf2, "version 2 is a different design");
    assert_eq!(render(&app, &key1, Some("tnt_a")).await.1, pdf1);

    // The old document still reprints with the design it was made with.
    let (status, reprint) = send(
        &app,
        request(
            &app,
            "GET",
            &format!("/api/documents/{doc_key}/pdf"),
            None,
            vec![],
            "application/json",
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(reprint, pdf1);
}

#[tokio::test]
async fn retiring_a_family_stops_renders_but_not_reprints() {
    let app = common::spawn_app_isolated_templates().await;
    let (_, v1) = publish(&app, &Pkg::new("Client Invoice", 100), None).await;
    let family = v1["data"]["family"].as_str().unwrap().to_string();
    let key = v1["data"]["key"].as_str().unwrap().to_string();
    grant(&app, &family, &["tnt_a"]).await;
    assert_eq!(render(&app, &key, Some("tnt_a")).await.0, StatusCode::OK);
    let (_, docs) = json(
        &app,
        request(
            &app,
            "GET",
            "/api/documents",
            Some("tnt_a"),
            vec![],
            "application/json",
        ),
    )
    .await;
    let doc_key = docs["data"]["documents"][0]["key"]
        .as_str()
        .unwrap()
        .to_string();

    let (status, body) = json(
        &app,
        request(
            &app,
            "POST",
            &format!("/api/packages/families/{family}/retire"),
            None,
            vec![],
            "application/json",
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["retiredVersions"], 1);

    assert!(
        templates_for(&app, Some("tnt_a"))
            .await
            .iter()
            .all(|t| t["family"] != family.as_str())
    );
    assert_eq!(
        render(&app, &key, Some("tnt_a")).await.0,
        StatusCode::NOT_FOUND
    );
    let (status, _) = send(
        &app,
        request(
            &app,
            "GET",
            &format!("/api/documents/{doc_key}/pdf"),
            None,
            vec![],
            "application/json",
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn invalid_packages_are_rejected_and_leave_nothing_behind() {
    let app = common::spawn_app_isolated_templates().await;
    let packages_dir = app.templates_dir.as_ref().unwrap().path().join("packages");
    let leftovers = || {
        std::fs::read_dir(&packages_dir)
            .map(|d| d.count())
            .unwrap_or(0)
    };

    let mut cases: Vec<(&str, Pkg)> = Vec::new();
    let mut not_compiling = Pkg::new("Broken", 100);
    not_compiling.typ = "#let x = ( // unterminated\n".into();
    cases.push(("does not compile", not_compiling));
    let mut bad_import = Pkg::new("Bad Import", 100);
    bad_import.typ = "#import \"../lib/money.typ\": *\n= Hi\n".into();
    cases.push(("import outside ../../lib", bad_import));
    let mut other_package = Pkg::new("Peek", 100);
    other_package.typ = "#let x = read(\"../doc_temp_other/schema.json\")\n".into();
    cases.push(("reads another package", other_package));
    let mut default = Pkg::new("Sneaky Default", 100);
    default.schema["x-default"] = true.into();
    cases.push(("x-default", default));
    let mut no_kind = Pkg::new("No Kind", 100);
    no_kind
        .schema
        .as_object_mut()
        .unwrap()
        .remove("x-document-kind");
    cases.push(("no kind", no_kind));
    let mut bad_sample = Pkg::new("Bad Sample", 100);
    bad_sample.sample = serde_json::json!({ "title": 7 });
    cases.push(("sample violates schema", bad_sample));
    let mut bad_asset = Pkg::new("Bad Asset", 100);
    bad_asset.assets = vec![("run.exe", vec![1, 2, 3])];
    cases.push(("asset type", bad_asset));
    let mut preset_identity = Pkg::new("Preset", 100);
    preset_identity.manifest =
        r#"{"format":1,"type":"document","name":"doc_temp_aaaaaaaaaaaaaaaa"}"#.into();
    cases.push(("identity in an upload", preset_identity));
    let mut wrong_format = Pkg::new("Format", 100);
    wrong_format.manifest = r#"{"format":2,"type":"document"}"#.into();
    cases.push(("format", wrong_format));
    let mut bad_options = Pkg::new("Opts", 100);
    bad_options.schema["x-options"] = serde_json::json!([{"key": "showSign", "type": "toggle", "label": "Sign", "default": true}]);
    cases.push(("option missing from properties.options", bad_options));

    for (label, pkg) in cases {
        let (status, body) = publish(&app, &pkg, None).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{label}: {body}");
        assert_eq!(body["code"], "PACKAGE_INVALID", "{label}");
    }
    // Not a zip at all.
    let (status, body) = json(
        &app,
        request(
            &app,
            "POST",
            "/api/packages",
            None,
            b"nope".to_vec(),
            "application/zip",
        ),
    )
    .await;
    assert_eq!(
        (status, body["code"].as_str()),
        (StatusCode::UNPROCESSABLE_ENTITY, Some("PACKAGE_INVALID"))
    );
    // A path-traversal entry.
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    writer
        .start_file("../evil.typ", zip::write::SimpleFileOptions::default())
        .unwrap();
    writer.write_all(b"x").unwrap();
    let evil = writer.finish().unwrap().into_inner();
    let (status, _) = json(
        &app,
        request(&app, "POST", "/api/packages", None, evil, "application/zip"),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

    assert_eq!(leftovers(), 0, "rejected packages must leave no files");
    let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM templates WHERE scope = 'private'")
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(rows, 0);
}

#[tokio::test]
async fn titles_must_be_unique_across_designs_and_versions_must_keep_their_kind() {
    let app = common::spawn_app_isolated_templates().await;
    let (_, a) = publish(&app, &Pkg::new("Client Invoice", 100), None).await;
    let family = a["data"]["family"].as_str().unwrap().to_string();

    let (status, body) = publish(&app, &Pkg::new("Client Invoice", 100), None).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    let (status, _) = publish(&app, &Pkg::new("A4 Invoice", 100), None).await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "a bundled title is taken too"
    );

    // A new version may keep the title, but may not change the kind.
    let (status, _) = publish(&app, &Pkg::new("Client Invoice", 120), Some(&family)).await;
    assert_eq!(status, StatusCode::CREATED);
    let mut other_kind = Pkg::new("Client Invoice", 100);
    other_kind.schema["x-document-kind"] = "quotation".into();
    let (status, _) = publish(&app, &other_kind, Some(&family)).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

    // Unknown or bundled families cannot be versioned.
    let (status, _) = publish(&app, &Pkg::new("Ghost", 100), Some("fam_aaaaaaaaaaaaaaaa")).await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "an unknown but well-formed family id starts that family"
    );
    let bundled: String =
        sqlx::query_scalar("SELECT family FROM templates WHERE description = 'Quotation'")
            .fetch_one(&app.db)
            .await
            .unwrap();
    let (status, _) = publish(&app, &Pkg::new("Quote Two", 100), Some(&bundled)).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn grants_validate_their_input() {
    let app = common::spawn_app_isolated_templates().await;
    let (_, a) = publish(&app, &Pkg::new("Client Invoice", 100), None).await;
    let family = a["data"]["family"].as_str().unwrap().to_string();
    assert_eq!(
        grant(&app, &family, &["bad key!"]).await,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(grant(&app, &family, &[""]).await, StatusCode::BAD_REQUEST);
    assert_eq!(
        grant(&app, "fam_zzzzzzzzzzzzzzzz", &["tnt_a"]).await,
        StatusCode::NOT_FOUND
    );
    let bundled: String =
        sqlx::query_scalar("SELECT family FROM templates WHERE description = 'Quotation'")
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(
        grant(&app, &bundled, &["tnt_a"]).await,
        StatusCode::BAD_REQUEST
    );
    let (status, _) = json(
        &app,
        request(
            &app,
            "POST",
            &format!("/api/packages/families/{bundled}/retire"),
            None,
            vec![],
            "application/json",
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn the_archive_installs_on_another_server_under_the_same_identity() {
    let source = common::spawn_app_isolated_templates().await;
    let (_, published) = publish(&source, &Pkg::new("Client Invoice", 100), None).await;
    let name = published["data"]["name"].as_str().unwrap().to_string();
    let family = published["data"]["family"].as_str().unwrap().to_string();
    let sha = published["data"]["sha256"].as_str().unwrap().to_string();
    grant(&source, &family, &["tnt_a"]).await;

    // Only a tenant the family is granted to can export it.
    let (status, _) = send(
        &source,
        request(
            &source,
            "GET",
            &format!("/api/packages/{name}/archive"),
            Some("tnt_b"),
            vec![],
            "application/json",
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, archive) = send(
        &source,
        request(
            &source,
            "GET",
            &format!("/api/packages/{name}/archive"),
            Some("tnt_a"),
            vec![],
            "application/json",
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(document_server::clients::render::sha256_hex(&archive), sha);

    // Install on a second server (a desktop sidecar: no tenant header).
    let desktop = common::spawn_app_isolated_templates().await;
    let install = |bytes: Vec<u8>, sha: &str| {
        request(
            &desktop,
            "PUT",
            &format!("/api/packages/{name}?sha256={sha}"),
            None,
            bytes,
            "application/zip",
        )
    };
    let (status, body) = json(&desktop, install(archive.clone(), &sha)).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["data"]["name"], name.as_str());
    assert_eq!(body["data"]["family"], family.as_str());
    assert_eq!(body["data"]["version"], 1);
    assert_eq!(body["data"]["sha256"], sha.as_str());
    let key = body["data"]["key"].as_str().unwrap().to_string();
    // Granted to the sidecar's single-shop bucket: it renders without a tenant header.
    assert_eq!(render(&desktop, &key, None).await.0, StatusCode::OK);
    assert!(
        templates_for(&desktop, None)
            .await
            .iter()
            .any(|t| t["name"] == name.as_str())
    );

    // Installing the same bytes again is a no-op.
    let (status, _) = json(&desktop, install(archive.clone(), &sha)).await;
    assert_eq!(status, StatusCode::OK);
    // A wrong digest is refused before anything is read.
    let (status, body) = json(&desktop, install(archive.clone(), &"0".repeat(64))).await;
    assert_eq!(
        (status, body["code"].as_str()),
        (StatusCode::UNPROCESSABLE_ENTITY, Some("PACKAGE_INVALID"))
    );
    // Different content under an installed name is a conflict.
    let mut tampered = Pkg::new("Client Invoice", 300);
    tampered.manifest = serde_json::json!({
        "format": 1, "type": "document", "minAppVersion": null,
        "name": name, "family": family, "version": 1, "createdAt": "2026-01-01T00:00:00Z"
    })
    .to_string();
    let tampered_bytes = tampered.zip();
    let tampered_sha = document_server::clients::render::sha256_hex(&tampered_bytes);
    let (status, body) = json(&desktop, install(tampered_bytes, &tampered_sha)).await;
    assert_eq!(
        (status, body["code"].as_str()),
        (StatusCode::CONFLICT, Some("PACKAGE_CONFLICT"))
    );

    // The manifest must name the URL's name.
    let other_name = "doc_temp_zzzzzzzzzzzzzzzz";
    let (status, _) = json(
        &desktop,
        request(
            &desktop,
            "PUT",
            &format!("/api/packages/{other_name}?sha256={sha}"),
            None,
            archive.clone(),
            "application/zip",
        ),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

    // A desktop drops a revoked package by replacing its grants with none.
    let (_, detail) = json(
        &desktop,
        request(
            &desktop,
            "GET",
            &format!("/api/packages/{name}"),
            None,
            vec![],
            "application/json",
        ),
    )
    .await;
    assert_eq!(detail["data"]["grants"], serde_json::json!([""]));
    assert_eq!(grant(&desktop, &family, &[]).await, StatusCode::NO_CONTENT);
    assert_eq!(render(&desktop, &key, None).await.0, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn preview_renders_the_packages_own_sample_without_recording_a_document() {
    let app = common::spawn_app_isolated_templates().await;
    let (_, published) = publish(&app, &Pkg::new("Client Invoice", 100), None).await;
    let name = published["data"]["name"].as_str().unwrap().to_string();
    let (status, pdf) = send(
        &app,
        request(
            &app,
            "POST",
            &format!("/api/packages/{name}/preview"),
            None,
            vec![],
            "application/json",
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(pdf.starts_with(b"%PDF-"));
    let documents: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM documents")
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(documents, 0);
    let (status, _) = send(
        &app,
        request(
            &app,
            "POST",
            "/api/packages/doc_temp_zzzzzzzzzzzzzzzz/preview",
            None,
            vec![],
            "application/json",
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn options_are_declared_validated_and_required_in_the_payload() {
    let app = common::spawn_app_isolated_templates().await;
    let mut pkg = Pkg::new("Optional Invoice", 100);
    pkg.typ = "#import sys: inputs\n#set page(width: 100mm, height: 50mm)\n= #inputs.title\n#if inputs.options.showSign [Signed]\n".into();
    pkg.schema["required"] = serde_json::json!(["title", "options"]);
    pkg.schema["properties"]["options"] = serde_json::json!({
        "type": "object",
        "required": ["showSign", "accent"],
        "properties": { "showSign": { "type": "boolean" }, "accent": { "type": "string" } }
    });
    pkg.schema["x-options"] = serde_json::json!([
        { "key": "showSign", "type": "toggle", "label": "Signature box", "default": true },
        { "key": "accent", "type": "select", "label": "Accent", "default": "blue",
          "choices": [{ "value": "blue", "label": "Blue" }, { "value": "red", "label": "Red" }] }
    ]);
    pkg.sample =
        serde_json::json!({ "title": "Hello", "options": { "showSign": true, "accent": "blue" } });
    let (status, body) = publish(&app, &pkg, None).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let options = body["data"]["options"].as_array().unwrap();
    assert_eq!(options.len(), 2);
    assert_eq!(options[0]["type"], "toggle");
    assert_eq!(options[1]["choices"][1]["value"], "red");

    // A payload without the options object is rejected by the contract.
    let family = body["data"]["family"].as_str().unwrap();
    let key = body["data"]["key"].as_str().unwrap();
    grant(&app, family, &["tnt_a"]).await;
    let (status, _) = render(&app, key, Some("tnt_a")).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

    // A select whose default is not a choice is an invalid package.
    let mut broken = Pkg::new("Broken Options", 100);
    broken.schema["required"] = serde_json::json!(["title", "options"]);
    broken.schema["properties"]["options"] =
        serde_json::json!({ "type": "object", "properties": { "accent": { "type": "string" } } });
    broken.schema["x-options"] = serde_json::json!([
        { "key": "accent", "type": "select", "label": "Accent", "default": "green",
          "choices": [{ "value": "blue", "label": "Blue" }] }
    ]);
    let (status, _) = publish(&app, &broken, None).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn assets_ship_with_the_package_and_are_usable_by_the_template() {
    let app = common::spawn_app_isolated_templates().await;
    let mut pkg = Pkg::new("Stamped Invoice", 100);
    pkg.typ = "#import sys: inputs\n#set page(width: 100mm, height: 50mm)\n#image(\"assets/stamp.png\", width: 10mm)\n= #inputs.title\n".into();
    pkg.assets = vec![("stamp.png", common::TINY_PNG.to_vec())];
    let (status, body) = publish(&app, &pkg, None).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let name = body["data"]["name"].as_str().unwrap();
    assert!(
        app.templates_dir
            .as_ref()
            .unwrap()
            .path()
            .join("packages")
            .join(name)
            .join("assets/stamp.png")
            .exists()
    );
}
