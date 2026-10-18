# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

A standalone Rust/Axum service that renders PDFs from Typst templates (embedded via `typst-as-lib`, no shelling out to a `typst` CLI). It is a **shared document service**: SimpleBash POS's backend (`../backend`) is its primary caller today, and future projects are expected to integrate the same way — publish a template plus a machine-readable data contract here, then feed JSON from anywhere. It has its own binary, its own local SQLite database file, its own port (`8090` by default, vs. `../backend`'s `8080`). SQLite's role is narrow: template *metadata* (including each template's input contract), rendered-*document* records, and stored shared/static template *data* blobs — never the `.typ` source, fonts, or the rendered PDF bytes themselves.

**Access model — one shared secret, not "no auth"**: every route meant only for backend callers (`render`, `documents`, `templates`, `packages`, all of `template-data`) takes the `InternalCaller` extractor (`core/middleware/auth.rs`) and requires `X-Internal-Api-Key: $INTERNAL_API_KEY`. `INTERNAL_API_KEY` has deliberately no default — an unset secret fails startup rather than booting wide open. Genuinely public routes: `GET /api/health`, Swagger, barcodes and QR codes. There are no users/roles/tokens; a richer auth model would be a deliberate new addition.

**Tenant scoping**: SimpleBash POS's cloud deployment (`TENANT_MODE=multi`) runs *one* document-server instance shared across every tenant, so `InternalCaller` also carries an optional `tenant_key: TenantKey` parsed from `X-Tenant-Key` — `TenantKey::Single` (stored as `""`) when the header is absent, which is what every desktop/self-hosted single-shop deployment sends forever, or `TenantKey::Tenant(id)` for the cloud backend's `core::tenancy::current_tenant_id()`. Tables that carry a `tenant_key`: `template_grants` (which tenants a private template family is visible to), `template_data` (every query is scoped — two tenants may store a blob under the identical `(templateName, dataKey)` without colliding or leaking) and `documents` (recorded for audit/cleanup, but reads — `GET /api/documents`, `GET /{key}`, the reprint's `GET /{key}/pdf` — are **not** filtered by the caller's header, since a document's `key` is opaque and looked up directly; a reprint instead merges `template_data` against the *document's own recorded* `tenant_key`, never the reprint caller's, so a forged/mismatched header on a reprint call can't pull a different tenant's stored blobs into it). **Templates are tenant-scoped by visibility**: a `global` (bundled) template is visible to everyone, a `private` package only to tenants its *family* was granted to (`template_grants`). `GET /api/templates`, `GET /api/templates/{key}` and `POST /api/render/{key}` all filter by the caller's `X-Tenant-Key`; a template the tenant may not see is a plain 404. A desktop sidecar sends no header (`TenantKey::Single`, stored as `""`), so installing a package there grants its family to `""`. An `X-Tenant-Key` that doesn't look like an id (not `[A-Za-z0-9_-]`, or over 128 chars) is rejected with 400 `TENANT_KEY_INVALID`, checked only after the `X-Internal-Api-Key` check passes. See `clients/sqlite.rs::table_has_old_narrow_unique_constraint` for how a database created before this feature (with the narrower inline `UNIQUE (template_name, data_key)`) is migrated safely, and `tests/tenant_scoping_test.rs` / `tests/sqlite_migration_test.rs` for the coverage.

**Env vars** (all default except `INTERNAL_API_KEY`): see `.env.example`. `BIND_ADDR` (default `0.0.0.0`) restricts the listener to loopback (`127.0.0.1`) for the Tauri desktop bundle. Beyond the core set, `REMOTE_IMAGE_FETCH_ENABLED` / `REMOTE_IMAGE_MAX_BYTES` / `REMOTE_IMAGE_TIMEOUT_SECS` bound the render-time `logoUrl` download (see the `render` module below).

### Keep the two checkouts in sync (check before doing anything)

Every app repo exists **twice** on this machine: the working copy at `pos/<name>` (edit and commit here) and the pinned submodule at `pos/desktop/<name>` (what the desktop app and its installers build from). They drift apart, and when they do, features silently disappear from whichever one you run (the desktop's browser sign-in step once lived only in the pinned frontend, so a run from the working copy showed the old form).

**Before starting any task, compare them:**

```bash
git -C pos/<name> fetch ../desktop/<name>
git -C pos/<name> log --oneline HEAD..FETCH_HEAD     # in the pinned copy, missing here -> merge these in first
git -C pos/<name> log --oneline FETCH_HEAD..HEAD     # in the working copy, missing from the pin
git -C pos/desktop submodule status
```

- Pinned copy has commits the working copy lacks: `git merge <sha>` them into the working copy first (resolve conflicts, rerun the tests).
- Working copy is ahead: commit there, then move the pin: `git -C pos/desktop/<name> fetch ../../<name> && git -C pos/desktop/<name> checkout --detach <sha>`. Never edit files inside `pos/desktop/<name>` directly. Pushing (and the bump PR) is a separate, deliberate step.
- Never run `tauri dev` or build an installer while the working copy is behind the pin.
- Dev run of the desktop app against the local Docker stack: build with `CLOUD_API_URL=http://localhost:8082 CLOUD_SYNC_API_URL=http://localhost:8081` (read at compile time with `option_env!`; `touch pos/desktop/src-tauri/src/cloud/mod.rs` to force a rebuild). Use `:8081` for the Docker POS API: the Docker backend on `:8080` shares its port with the desktop's own sidecar, which answers first on loopback.

## Local Docker: never let a compose build replace the private designs

The local stack (`docker-compose.local.yml` at the workspace root) must run the document-server with the **real private designs**. A plain compose build copies only this repo's public example templates (the A4 invoice is 87 lines instead of 434) and silently downgrades every local invoice, receipt and quotation.

- **Trap:** `up --build backend` (or any service that depends on `document-server`) rebuilds the document-server image from the public templates too, overwriting the `ghcr.io/simplebash-official/document-server:latest` tag and recreating the container. So this applies to *any* local rebuild, not only to document-server changes.
- **Correct order:** build the private-overlay image *after* the compose build and recreate the container with `--no-build`:
  1. rsync `pos/document-server` (excluding `target`, `graphify-out`, `.git`) to a scratch dir;
  2. `<scratch>/scripts/assemble-templates.sh pos/document-templates <scratch>/templates` (never write private `.typ` files into this repo);
  3. `docker build -t ghcr.io/simplebash-official/document-server:latest <scratch>`;
  4. `docker compose -f docker-compose.local.yml --env-file .env.docker.local up -d --no-build document-server`.
- **Check every time:** `docker exec simplebash-document-server wc -l /app/templates/documents/doc_temp_vEf0Y7jQHQj2rIuO.typ` must print 434 (public example: 87), then `./scripts/local-docker.sh verify`.
- **Data:** published template packages live in the `document_packages` volume (`/app/templates/packages`) and the SQLite database in `document_data`; `docker compose down -v` deletes both. The first boot after the packages change migrates the `templates` table in place, so copy `/app/data/document_server.db` out of the container before a risky rebuild.

## Before finishing any task: test everything that can be tested

A task is not done until everything in this codebase that *can* be tested, and everything related to the feature you implemented, has been tested and passes. Not just the lines you changed.

1. **The feature itself:** add or extend automated tests for every behaviour you added or changed (happy path, failure paths, edge cases), then run them.
2. **Everything around it:** run this repo's full suite and quality gates (below), not only the new tests. A change can break a caller far away.
3. **Across repos:** when a feature spans repos (backend, frontend, desktop shell, document-server, identity-server, app-frontend, deployment), run the gates in *every* repo you touched, plus the tests that exercise the whole path end to end.
4. **For real:** what automated tests cannot reach (UI behaviour, a running stack, long-lived connections, a deploy) is verified by running it: the local Docker stack (`LOCAL_DOCKER_GUIDE.md` at the workspace root), the desktop app, or the browser.
5. **Match CI's toolchain:** CI uses the latest stable Rust/Node. A lint that passes on an older local toolchain can still fail there, so update (`rustup update`) or run the gate with CI's version before calling it done.
6. **Report honestly:** say what you ran and its result. Anything you could not test is named, with the reason, and never presented as passing. A suite that silently skips (e.g. no database configured) is not a passing suite.

**This repo's gate:** `make check` (`cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`), plus `pos/backend`'s schema-drift tests (`print_payload::schema_drift_tests`) whenever a template contract changes.

**Run all of these before you say a task is finished** (a backend feature once reached CI with clippy errors, a permission that was too wide and undocumented routes, none of which a partial run catches):

1. `cargo clippy --all-targets --all-features -- -D warnings` on the current stable Rust (`rustup update` first). This is exactly what CI runs; `cargo clippy --fix --allow-dirty` clears most of it. `cargo build`/`cargo test` alone do not run clippy.
2. The **whole** `cargo test` (`--no-fail-fast`, so one failing binary does not hide the rest), then `pos/backend`'s schema-drift tests (`print_payload::schema_drift_tests`) whenever a template contract changes.
3. New or changed routes: keep their OpenAPI/auth annotations in step with what the handler really enforces, and test the failure paths.
4. After pushing, watch the run (`gh run list --limit 3`, `gh run view <id> --log-failed`) until it is green. Pushing is a separate, deliberate step, so ask first.

# Commands

- `cargo build` / `cargo run` (equivalent to `cargo run --bin document_server`)
- `cargo test` — every test needs no external service: `tests/*_test.rs` open their own throwaway SQLite file per test (see `tests/common::spawn_app()`), and the `#[cfg(test)]` unit tests in `src/clients/render.rs` exercise the real Typst compiler against the real `templates/`/`fonts/` directories.
- `cargo fmt` / `cargo fmt --check`
- `make check` — `cargo fmt --check` → `cargo clippy --all-targets --all-features -- -D warnings` → `cargo test`, stopping at first failure. Run after every change.

**Every change must be tested** — extend the existing test file covering the touched area, or create one following `tests/<area>_test.rs` naming.

Config loads from `.env` (via `dotenvy`) in the binary and every integration test; see `.env.example`. Every var defaults except `INTERNAL_API_KEY`.

## Architecture

**Binary/library split**: `src/lib.rs` re-exports `app`, `clients`, `core`, `domain`, `modules` as a library crate; `src/main.rs` is a thin binary (config → SQLite → `RenderEngine::warm_up()` → startup disk-sync → router → serve).

### Request flow & hot reload

`main.rs`: `Config::from_env()` → `clients::sqlite::connect()` (creates/migrates the `templates`, `template_grants`, `documents`, `template_data` tables) → `clients::render::RenderEngine::warm_up()` (preloads fonts, reads the **bundled** templates in `templates/documents` and `templates/labels` — each needs `<stem>.schema.json` and `<stem>.json`, a bad one fails startup — checks every kind has exactly one `x-default` template, runs one best-effort empty-input trial compile per template, logged at `debug`) → `modules::templates::service::sync_bundled()` (upserts them as `global`/`published`, retires global rows no longer shipped) → `AppState { config, db, render: Arc<RenderEngine> }`.

The engine is built once and **never replaced**. Private packages are not known to it: each render looks up the package's `file_path` from its `templates` row and compiles through `RenderEngine::compile_once` (a throwaway engine), so publishing a package needs no rebuild or restart and a package's files never enter the shared engine's file cache. Bundled templates are compiled on the shared engine (`compile_bundled`).

### Feature modules (`src/modules/<name>/`)

Each layered `routes.rs` → `service` → `repository`; repositories return raw driver results (`Option`/`Vec`), error translation happens one layer up. Sibling modules reach each other through `pub(crate)` `service` fns, never `repository`:

- **`render`** — `POST /api/render/{templateKey}` (`InternalCaller`). The one envelope deviation: success is raw PDF bytes (`Content-Type: application/pdf`); errors use the standard `ErrorResponse`. Per request: resolve the published template the tenant may render (`templates::service::resolve_renderable`) → deep-merge stored `template-data` blobs under the payload (**request wins**) → validate merged payload against the template's schema when it ships one (`jsonschema`, 422 `RENDER_VALIDATION_FAILED` naming fields) → **if the merged payload has a non-empty `logoUrl`**, turn it into bytes — `data:image/…;base64,…` is decoded inline (`clients::http::decode_data_uri`, not gated by `REMOTE_IMAGE_FETCH_ENABLED`), an http(s) URL is downloaded (`clients::http::fetch_image`, scheme/size/type/timeout guards) — 422 `REMOTE_IMAGE_FETCH_FAILED` on any failure; write it beside the `.typ` as `.rimg_<nanoid>.<ext>`, set `logo` to that filename, and compile through a **one-shot throwaway engine** (`RenderEngine::compile_once` — so the image never enters the shared engine's unbounded file cache) with an RAII guard deleting the temp file after; otherwise compile through the shared engine as before → export → record a `documents` row holding the *request's* original payload — except a `data:` `logoUrl` is blanked first (a base64 blob would land on every row; `record_document`). `warm_up` sweeps orphan `.rimg_*` files left by a crash.
- **`templates`** — `GET /` (+`?type=document|label`, anything else is 400) lists the **latest published version of every family** the calling tenant may use; `GET /{key}` returns one. Both need the internal key and are scoped by `X-Tenant-Key`. A `Template` has `key`, `name` (opaque generated id), `family`, `version`, `type`, `kind` (schema `x-document-kind`), `description` (schema `title`), `status` (`published`|`retired`), `scope` (`global`|`private`), `isDefault` (schema `x-default`), `options` (schema `x-options`), `dataSchema`, `data` (sample), `sha256`, `minAppVersion`. There is no create/sync route any more — publishing is `packages`.
- **`packages`** — template packages (all `InternalCaller`). A package is a zip: `manifest.json` (`format: 1`, `type`, `minAppVersion`), `template.typ`, `schema.json`, `sample.json`, optional `assets/*.{png,jpg,jpeg,svg}`. `POST /api/packages[?family=fam_…]` (raw zip body, `MAX_PACKAGE_BYTES`) validates it (`modules::packages::archive` + `domain::templates::validate_template_schema`: valid JSON Schema with `title` and `x-document-kind`, well-formed `x-options` that the schema's `properties.options` and `required` back up, no `x-default`, imports only `../../lib/…`, no path traversal/symlinks/hidden files, unique title across families, a new version keeps kind/type), assigns the identity (`doc_temp_/lbl_temp_<nanoid>`, `fam_<nanoid>` for a new design, `version = max+1`), smoke-compiles the sample with remote images disabled, writes the folder `templates/packages/<name>/` (`template.typ`, `schema.json`, `sample.json`, `assets/`, `package.zip`) via a staging folder + rename, and inserts a `private`/`published` row. **A published package is never edited**: a change is a new version, which is why a reprint always uses the design the document was rendered with. `GET /` lists everything with grants, `GET /{name}`, `GET /{name}/archive` (stored zip; the tenant must have the family granted), `PUT /{name}?sha256=` installs an exported archive under its own identity and grants the family to the caller's tenant (idempotent; different content under an installed name is 409), `POST /{name}/preview` renders the package's own sample, `PUT /families/{family}/grants` replaces the tenants a family is visible to (`""` is not accepted — only installs add it), `POST /families/{family}/retire` retires every version (renders stop, reprints keep working). Bundled families cannot be granted, retired or versioned. **Authoring without a server:** `cargo run --bin validate_package -- <zip> [--pdf out.pdf]` (`modules::packages::dry_run`) runs the exact publish validation and renders the sample on a throwaway engine — no database, nothing published. The owner's coding agents use it through `../document-templates/packages/tools/pkg.py check|build` to produce zips for upload. Packages are trusted code from the platform owner: the path lint is defence in depth, the real boundary is the resolver root `TEMPLATES_DIR`.
- **`documents`** — render records: `GET /` newest-first, `GET /{key}`, `GET /{key}/pdf` reprint (`InternalCaller`). Reprint resolves the stored `template_key` through `templates::service::resolve_reprintable` (status ignored, tenant visibility enforced against the document's own tenant) and recompiles the stored `data` via `render::service::compile_pdf` (the shared validate + `logoUrl`-fetch + compile + error-mapping fn) and re-applies the stored-data merge.
- **`template_data`** — stored shared/static per-template JSON blobs (shop profile, bank details…): `PUT/GET/DELETE /api/template-data/{templateName}/{dataKey}` + `GET /api/template-data/{templateName}` (all `InternalCaller`). PUT requires the template name to exist; repeated PUT overwrites; DELETE echoes the removed blob. Blobs are keyed by template *name* — the generated `<type>_temp_<nanoid>` id, not the human description.
- **`barcodes` / `qrcodes`** — SVG barcode/QR generation utilities (no DB).

### Data contracts (schema sidecars)

Every template that wants validated input ships `<name>.schema.json` next to its `.typ` (JSON Schema draft-07; drafts auto-detected from `$schema`). Convention: `required` + types enforced, `additionalProperties: true` (unknown fields allowed so callers can carry extra context). Validation happens **twice by design**: document-server validates before compiling, and SimpleBash POS's backend pre-validates against the same published schema before sending — both answer 422 `RENDER_VALIDATION_FAILED`, so callers treat them identically. Templates without a sidecar behave exactly as before (Typst errors are the only failure signal).

The schema's **`title`** doubles as the template's human name: it is copied into the `templates.description` column (the `.typ` filename is an opaque generated id — see "Template identity" below), and it's what `GET /api/templates` and integration clients show. Two more schema keys matter: **`x-document-kind`** (required: what the template is for — `a4-invoice`, `thermal-receipt`, `credit-note`, `quotation`, `analytics-report`, `product-label`) and **`x-default`** (bundled templates only: exactly one per kind must set it; it replaces the old default-by-title lookup in the backend).

The `<name>.json` sidecar is mandatory *sample data* (exposed as `Template.data`); it must satisfy the schema and is the preview and smoke-compile payload.

### Integration contract with `../backend` (and any future client)

1. Client fetches `GET /api/templates` (with `X-Tenant-Key` in the cloud) → caches `{family, version, kind, key, dataSchema, options}` per template; a design is identified by its `family`, the version only tells designs of one family apart.
2. Client builds a payload per `dataSchema`, pre-validates, then `POST /api/render/{templateKey}` with `X-Internal-Api-Key`. A payload field typed as an image URL (e.g. `logoUrl`) is resolved server-side at render time and dropped after — the client sends an http(s) URL or a `data:` image URI.
3. Optional: `PUT /api/template-data/{name}/{key}` to store shared/static fields server-side instead of resending them (and sharing them across projects). `{name}` here is the template's real (generated) `name`, not its description.
4. Adding a design for specific tenants = `POST /api/packages` then `PUT /api/packages/families/{family}/grants`; changing it = `POST /api/packages?family=…` (a new version). A bundled (everyone's) template is a file in this repo's `templates/documents|labels` plus its two sidecars, shipped with the image/app. Never a restart.
5. **Options**: a template may declare `x-options` (toggle/select/color/text controls, each with a default) in its schema. The client shows them as a form, stores the values per shop, and sends them as the payload's `options` object; the schema requires every declared option, so a missing one is a 422.

### Template identity

The **Product Sticker Label** (`lbl_temp_qklcWIolwoFFN3xk`) is **data-driven**, not a fixed design: the payload carries the label layout (`widthMm`, `heightMm`, `elements[]` of `text` | `barcode` | `qr` | `line`, each absolutely placed in mm with every field required) plus `labels[]` (`name`, `sku`, `category`, `priceCents|null`, `code`, `copies`), and the template draws one page per sticker copy, each page exactly the label size (max 500 pages, asserted in the template). The POS backend stores shop-designed layouts and renders them through this one template (there is deliberately no private override of it in `document-templates`). Typst details that matter: Code128 is called through `tiaoma` directly with `show-hrt: false` (Zint's own caption is a font we do not control) and `output-options: (barcode-quiet-zones: true)` (the quiet zone is inside the element's box), stretched with `fit: "stretch"`; QR uses `zebra` with a 2-module quiet zone (ECC M). Text elements shrink to fit their box (`fitted-text`: 0.5pt steps measured in `context`, never below 4pt, which is the designer's `minFontPt`); only text that still overflows at 4pt is clipped, so long item names print whole (unit tests `sticker_text_*` in `clients/render.rs` read the laid-out font sizes). `templates/lib/barcode.typ` maps `EAN13`/`EAN8` to symbology names the vendored Zint plugin does not have (`EANX`/`EANXChk`), so it is not used for labels. The contract is mirrored by the backend's `modules::labels::service::payload` (its drift tests read this repo's schema).

A template's `name` (its `.typ` stem, its `templates.name`, and the scope key for `template-data`) is a generated id: `doc_temp_<nanoid>` for a `documents/` template, `lbl_temp_<nanoid>` for a `labels/` one (`core::id::generate_template_name`, prefix constants in `core::constants::prefixes`). `POST /api/packages` mints these. The `_temp_` infix keeps a template *name* from being read as a `doc_<nanoid>` document *key*. Renaming the four original seed templates to ids was a one-time change; an operator with an existing DB that has `template-data` blobs or a name-keyed backend cache must re-point them (`UPDATE template_data SET template_name = …`).

## Logging

`src/core/logging/` mirrors the backend's module. `main.rs` calls `logging::init` (default filter `document_server=info,tower_http=info,info`, overridable via `RUST_LOG`). The outermost `logging::request::log_requests` layer honours the backend's `X-Request-Id` (else mints `req_…`), echoes it, opens a `request` span (`request_id`, `caller` = `internal`/`public`) and logs `http/request` + `http/response`; with `LOG_HTTP_BODIES=true` textual bodies are logged redacted (`logging::redact`) and capped, PDFs only by size. Renders log `render/start`, `render/done` (PDF bytes, duration), `render/failed`, `render/schema_invalid` and `render/typst_error`; `AppError` logs 4xx at WARN and 5xx at ERROR, and the `sqlx`/`serde_json` conversions log their error. `LOG_FORMAT=json` (set only by the Tauri desktop shell) emits local-offset JSON lines the shell ingests into the unified desktop activity log; `LOG_SQL` (`all`/`slow`/`off`) controls statement logging in `clients::sqlite`. In that mode `core::logging::control` reads **stdin** for `{"cmd":"log_mode",…}` lines so the desktop benchmark can switch logging at runtime; `LogSettings` holds `enabled`/`http_bodies`/`sql` in atomics read per event, so don't cache them. Bodies over 4x the cap are scrubbed as text rather than parsed. Keep new log lines structured (`category = "…", event = "…"` fields) — contract: `docs/logging.md` in the pos-compose repo. Covered by `tests/logging_test.rs`.

## SQLite schema & queries

No migration framework — `CREATE TABLE IF NOT EXISTS` statements in `clients/sqlite.rs` plus additive guarded `ALTER TABLE ... ADD COLUMN` lines for columns added later (`tenant_key`). `templates` is the exception that was rebuilt: a database from before packages (detected by its `is_active` column) is rewritten once into the family/version/status/scope shape (`migrate_templates_to_packages`; every old row becomes a one-version global family, `is_active` becomes `status`), covered by `tests/sqlite_migration_test.rs`. `template_grants(family, tenant_key)` is the visibility table. `template_data`'s `tenant_key` is the one exception that needed more than `ADD COLUMN`: its old inline `UNIQUE (template_name, data_key)` constraint can't be widened by `ALTER TABLE`, so `create_schema` detects it (via the table's stored `sqlite_master` text) and rebuilds the table (create/copy-across/drop/rename) — see the "Tenant scoping" section above. All queries are runtime-checked `sqlx::query[_as]` calls with bound parameters (never compile-time macros, so no `DATABASE_URL` needed to build). JSON columns use `sqlx::types::Json<T>`. **Trap**: never use `sqlite::memory:` where the pool may hand out >1 connection — each connection gets its own empty DB; tests use throwaway files (`tempfile::NamedTempFile`).

## Custom keys, timestamps, response envelope

Same conventions as `../backend`: `key: <prefix>_<nanoid>` primary key via `core::id::generate_id` (prefixes `tpl_`/`doc_`/`tdat_`); a template's *name* (not a key) is `<prefix>_temp_<nanoid>` via `core::id::generate_template_name` (prefixes `doc`/`lbl`); `created_at`/`updated_at` on every row, `ApiResponse<T>` success envelope `{success, data, message}` and `AppError` → `ErrorResponse` `{success, message, code, statusCode}` (variants NotFound/Validation/Internal/Custom; 422 via `unprocessable_entity(code, message)`).

## Code Comments

1. Module-level banner on every file stating what it owns and doesn't.
2. Doc comments on `pub`/`pub(crate)` items whose purpose isn't obvious.
3. Inline comments explain *why*, not *what*.

## Testing map

- `tests/render_test.rs` — full-stack renders of all four seed templates incl. schema rejection paths, plus the A4 Invoice's `logoUrl` path: a `data:` URI logo (and assert the recorded row blanked it), plus http(s) against `common::spawn_image_server()` (downloads + cleans up, back-to-back renders don't accumulate state, bad/oversized/non-image/`file://` URLs → 422, reprint re-fetches). Templates are resolved by `description`, not name.
- `tests/templates_test.rs` — list/get incl. `dataSchema`/`description`/`kind`/`isDefault` exposure, type filter (unknown type = 400), the internal key requirement, retired templates hidden, plus the bundled-schema contract guards (valid schema rules, exactly one `x-default` per kind, titles unique, samples valid).
- `tests/dry_run_test.rs` — the offline `validate_package` check: valid package renders and leaves no scratch folder, publish rules apply (import lint, compile failure, missing file, identity in an upload).
- `tests/packages_test.rs` — publish/version/grant/retire/install/export/preview, tenant isolation of lists and renders, every invalid-package case (no files left behind), options, assets, reprint keeping the original design.
- `tests/template_data_test.rs` — CRUD roundtrip, unknown-template 404, merge semantics; resolves the seed template's real name via `tr_name`/`tr_key` helpers.
- `tests/documents_test.rs`, `tests/barcodes_test.rs`, `tests/qrcodes_test.rs`, `tests/openapi_test.rs` (asserts every module path is in the spec incl. `/api/packages/*`; both PDF endpoints documented as `application/pdf`), `tests/response_format_test.rs`.
- `tests/common/mod.rs` — `spawn_app_isolated_templates()` (private writable copy of `templates/`, for tests that write into the tree) and `spawn_image_server()` (localhost `image/png` / `text/html` / 6 MiB routes).
- `src/clients/render.rs` unit tests — fastest check that a Typst/typst-as-lib upgrade still compiles the real templates.
- `tests/tenant_scoping_test.rs` — two tenants (and the no-header single-shop bucket) storing under the identical `template_data` key never see each other's blob, an invalid `X-Tenant-Key` is rejected, a render records the calling tenant on its `documents` row, and a reprint merges against the *document's own* tenant regardless of the reprint caller's header.
- `tests/sqlite_migration_test.rs` — connects to a hand-built pre-tenant-scoping database (the old inline `UNIQUE (template_name, data_key)` constraint) and proves the rebuild migration lets a second tenant reuse the same key, is idempotent across a second connect, and that `documents`' simpler `ADD COLUMN`-only migration defaults existing rows correctly.

## API docs & Postman

Swagger at `/docs`, spec at `/api-docs/openapi.json` — every handler registered via `routes!()` only. `postman/document-server.postman_collection.json` is hand-maintained: update it whenever a route changes (same rules as `../backend`'s collection).

## Build status

Done: tenant-scoped, versioned template packages (see the `packages` module), core service, second template + barcode/QR packages, documents as first-class resources, internal-API-key auth, labels/documents split with credit-note template, **schema sidecars + strict validation**, **runtime sync/hot reload (ArcSwap engine)**, **stored shared/static template data with render-time merge**, barcodes/qrcodes modules, **generated template identity (`<type>_temp_<nanoid>`)**, **A4 Invoice restyled as a branded/modern layout**, **render-time `logoUrl` (http(s) download or `data:` URI) → one-shot compile → cleanup**. Not started: format negotiation (PNG/SVG), render-result caching keyed on `(template_key, data)`.

## graphify

This project has a knowledge graph at graphify-out/. For codebase questions run `graphify query "<question>"` first; `graphify path "<A>" "<B>"` for relationships; `graphify explain "<concept>"` for focused concepts; read `GRAPH_REPORT.md` only when those fall short. After modifying code, run `graphify update .` (AST-only, no API cost).

## Recent Features & Evolution (Last 30 Days)

### Landed Capabilities
- **Typst In-Process Engine with ArcSwap Hot Reload**: Embedded `typst-as-lib` compiler wrapped in `Arc<ArcSwap<RenderEngine>>`. Supports runtime template reloading (`POST /api/templates/sync`) and template upload (`POST /api/templates`) without service restarts, eliminating typst-as-lib's static file-caching limitation.
- **Dynamic Branded Documents & Logo Resolution**: High-fidelity A4 invoices and thermal receipt templates supporting runtime `logoUrl` inputs. Downloads external HTTP/HTTPS images or decodes `data:` URIs into temporary `.rimg_<nanoid>.<ext>` files, compiling through a throwaway one-shot engine (`RenderEngine::compile_once`) with RAII file cleanup so external blobs never enter the shared font/file cache.
- **Sidecar Data Contracts (`<name>.schema.json`)**: Formal JSON Schema draft-07 contract definitions for every document and label template. Enforces schema validation before compilation (returning structured 422 `RENDER_VALIDATION_FAILED`), mirrored by pre-validation in calling clients (`backend`).
- **Analytics & Financial Report Templates**: Typst templates for sales summaries, category breakdowns, and audit reports rendered directly to vector PDF.
- **Desktop Loopback Binding (`BIND_ADDR`)**: Desktop sidecar deployment support via `BIND_ADDR=127.0.0.1` configuration, ensuring the service restricts local listening in Tauri bundles.

### Invariants & Rules for Future Implementations
- **Strict Sidecar Accompaniment**: Every bundled `.typ` template under `templates/documents/` or `templates/labels/` must ship a corresponding `<name>.schema.json` contract (with `x-document-kind`) and `<name>.json` sample data, or startup fails. The schema `title` must be human-readable, as it acts as the template's descriptive name.
- **Zero Cache Bloat on Dynamic Assets**: Never compile dynamic images, private packages or user-supplied asset buffers through the shared long-lived `RenderEngine`. Always route them through `RenderEngine::compile_once` with cleanup guards.
- **Published Means Immutable**: never edit a package's files or its `templates` row after publishing; publish a new version.
- **Internal Authentication Guard**: Every mutating or document-rendering route must require `InternalCaller` and validate `X-Internal-Api-Key`. Never introduce default/fallback API keys in production or testing configuration.
- **Opaque Template Identity**: Template names must follow generated prefix formats (`doc_temp_<nanoid>` or `lbl_temp_<nanoid>`). Never expose filesystem paths or raw stems as public user IDs.

### How AI Agents Can Help & Verification
- **Automated Test Validation**: Run `cargo test` to execute all integration tests (which spin up isolated SQLite instances and mock image servers) and real Typst compilation unit tests.
- **Strict Static Analysis**:
  ```bash
  cargo fmt --check
  cargo clippy --all-targets --all-features -- -D warnings
  make check           # Formats, clips, and tests in sequence
  ```
- **Template Smoke Compilation**: When editing or adding `.typ` files, agents can run `src/clients/render.rs` unit tests to instantly verify that the Typst compiler successfully parses and renders the templates against sample payloads.
- **Contract & Spec Sync**: Agents must verify that new routes are added to `tests/openapi_test.rs` and documented in `postman/document-server.postman_collection.json`.

