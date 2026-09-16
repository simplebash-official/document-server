# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

A standalone Rust/Axum service that renders PDFs from Typst templates (embedded via `typst-as-lib`, no shelling out to a `typst` CLI). It is a **shared document service**: jana2u-pos's backend (`../backend`) is its primary caller today, and future projects are expected to integrate the same way — publish a template plus a machine-readable data contract here, then feed JSON from anywhere. It has its own binary, its own local SQLite database file, its own port (`8090` by default, vs. `../backend`'s `8080`). SQLite's role is narrow: template *metadata* (including each template's input contract), rendered-*document* records, and stored shared/static template *data* blobs — never the `.typ` source, fonts, or the rendered PDF bytes themselves.

**Access model — one shared secret, not "no auth"**: every route meant only for backend callers (`render`, `documents`, `POST /api/templates` + `templates/sync`, all of `template-data`) takes the `InternalCaller` extractor (`core/middleware/auth.rs`) and requires `X-Internal-Api-Key: $INTERNAL_API_KEY`. `INTERNAL_API_KEY` has deliberately no default — an unset secret fails startup rather than booting wide open. Genuinely public reads: `GET /api/health`, Swagger, `GET /api/templates`(list + single). There are no users/roles/tokens; a richer auth model would be a deliberate new addition.

**Env vars** (all default except `INTERNAL_API_KEY`): see `.env.example`. `BIND_ADDR` (default `0.0.0.0`) restricts the listener to loopback (`127.0.0.1`) for the Tauri desktop bundle. Beyond the core set, `REMOTE_IMAGE_FETCH_ENABLED` / `REMOTE_IMAGE_MAX_BYTES` / `REMOTE_IMAGE_TIMEOUT_SECS` bound the render-time `logoUrl` download (see the `render` module below).

## Commands

- `cargo build` / `cargo run` (equivalent to `cargo run --bin document_server`)
- `cargo test` — every test needs no external service: `tests/*_test.rs` open their own throwaway SQLite file per test (see `tests/common::spawn_app()`), and the `#[cfg(test)]` unit tests in `src/clients/render.rs` exercise the real Typst compiler against the real `templates/`/`fonts/` directories.
- `cargo fmt` / `cargo fmt --check`
- `make check` — `cargo fmt --check` → `cargo clippy --all-targets --all-features -- -D warnings` → `cargo test`, stopping at first failure. Run after every change.

**Every change must be tested** — extend the existing test file covering the touched area, or create one following `tests/<area>_test.rs` naming.

Config loads from `.env` (via `dotenvy`) in the binary and every integration test; see `.env.example`. Every var defaults except `INTERNAL_API_KEY`.

## Architecture

**Binary/library split**: `src/lib.rs` re-exports `app`, `clients`, `core`, `domain`, `modules` as a library crate; `src/main.rs` is a thin binary (config → SQLite → `RenderEngine::warm_up()` → startup disk-sync → router → serve).

### Request flow & hot reload

`main.rs`: `Config::from_env()` → `clients::sqlite::connect()` (creates `templates`/`documents`/`template_data` tables if missing) → `clients::render::RenderEngine::warm_up()` (preloads fonts, scans `templates/` for `.typ` files with their `.json` sample-data and `.schema.json` sidecar contracts, runs one best-effort empty-input trial compile per template — logged at `debug`, since failing on real-input templates is expected) → `modules::templates::service::sync_templates_from_disk()` → `AppState { config, db, render: Arc<ArcSwap<RenderEngine>> }`.

`AppState.render` is an **`ArcSwap`**, not a plain `Arc`: `POST /api/templates/sync` rebuilds the engine from disk at runtime and swaps it in atomically, so new/edited templates go live **without a restart**. Handlers snapshot with `load_full()`. This closes the old typst-as-lit cached-file-resolver trap ("edits need a restart") — don't reintroduce it by making the engine immutable again.

### Feature modules (`src/modules/<name>/`)

Each layered `routes.rs` → `service` → `repository`; repositories return raw driver results (`Option`/`Vec`), error translation happens one layer up. Sibling modules reach each other through `pub(crate)` `service` fns, never `repository`:

- **`render`** — `POST /api/render/{templateKey}` (`InternalCaller`). The one envelope deviation: success is raw PDF bytes (`Content-Type: application/pdf`); errors use the standard `ErrorResponse`. Per request: resolve active template → deep-merge stored `template-data` blobs under the payload (**request wins**) → validate merged payload against the template's schema when it ships one (`jsonschema`, 422 `RENDER_VALIDATION_FAILED` naming fields) → **if the merged payload has a non-empty `logoUrl`**, turn it into bytes — `data:image/…;base64,…` is decoded inline (`clients::http::decode_data_uri`, not gated by `REMOTE_IMAGE_FETCH_ENABLED`), an http(s) URL is downloaded (`clients::http::fetch_image`, scheme/size/type/timeout guards) — 422 `REMOTE_IMAGE_FETCH_FAILED` on any failure; write it beside the `.typ` as `.rimg_<nanoid>.<ext>`, set `logo` to that filename, and compile through a **one-shot throwaway engine** (`RenderEngine::compile_once` — so the image never enters the shared engine's unbounded file cache) with an RAII guard deleting the temp file after; otherwise compile through the shared engine as before → export → record a `documents` row holding the *request's* original payload — except a `data:` `logoUrl` is blanked first (a base64 blob would land on every row; `record_document`). `warm_up` sweeps orphan `.rimg_*` files left by a crash.
- **`templates`** — `GET /` (+`?type=document|label`) and `GET /{key}` are open reads exposing `key`, `name` (opaque generated id), `description` (human label, from the schema `title`), `type`, `dataSchema` and `data` (a worked sample). `POST /` (`InternalCaller`) creates a template: body is `{type, source, schema?, sample?}`; the server mints a `<type>_temp_<nanoid>` name, writes `.typ` + sidecars into `documents/`/`labels/`, re-syncs, smoke-compiles against `sample` when given, and hot-swaps the engine — rolling the files back on any failure (422 `TEMPLATE_CREATE_FAILED`). `POST /sync` (`InternalCaller`) = runtime rescan: warm up fresh engine, upsert metadata (incl. `description`), **deactivate rows whose `.typ` vanished** (never delete — keys are stable and history references them), swap engine.
- **`documents`** — render records: `GET /` newest-first, `GET /{key}`, `GET /{key}/pdf` reprint (`InternalCaller`). Reprint recompiles stored `data` via `render::service::compile_pdf` (the shared validate + `logoUrl`-fetch + compile + error-mapping fn) and re-applies the stored-data merge.
- **`template_data`** — stored shared/static per-template JSON blobs (shop profile, bank details…): `PUT/GET/DELETE /api/template-data/{templateName}/{dataKey}` + `GET /api/template-data/{templateName}` (all `InternalCaller`). PUT requires the template name to exist; repeated PUT overwrites; DELETE echoes the removed blob. Blobs are keyed by template *name* — the generated `<type>_temp_<nanoid>` id, not the human description.
- **`barcodes` / `qrcodes`** — SVG barcode/QR generation utilities (no DB).

### Data contracts (schema sidecars)

Every template that wants validated input ships `<name>.schema.json` next to its `.typ` (JSON Schema draft-07; drafts auto-detected from `$schema`). Convention: `required` + types enforced, `additionalProperties: true` (unknown fields allowed so callers can carry extra context). Validation happens **twice by design**: document-server validates before compiling, and jana2u-pos's backend pre-validates against the same published schema before sending — both answer 422 `RENDER_VALIDATION_FAILED`, so callers treat them identically. Templates without a sidecar behave exactly as before (Typst errors are the only failure signal).

The schema's **`title`** doubles as the template's human name: the sync pass copies it into the `templates.description` column (the `.typ` filename is now an opaque generated id — see "Template identity" below), and it's what `GET /api/templates` and integration clients read to tell templates apart.

The `<name>.json` sidecar remains optional *sample data* (exposed as `Template.data`) — documentation, never enforced. Historical note: the `templates.data_schema` column once held samples under a misleading name; the sync pass now writes the real schema there and keeps samples in the additive `sample_data` column.

### Integration contract with `../backend` (and any future client)

1. Client fetches `GET /api/templates` → caches `{description → key, dataSchema}` (`name` is an opaque id; identify a template by its `description`, i.e. the schema `title`).
2. Client builds a payload per `dataSchema`, pre-validates, then `POST /api/render/{templateKey}` with `X-Internal-Api-Key`. A payload field typed as an image URL (e.g. `logoUrl`) is resolved server-side at render time and dropped after — the client sends an http(s) URL or a `data:` image URI.
3. Optional: `PUT /api/template-data/{name}/{key}` to store shared/static fields server-side instead of resending them (and sharing them across projects). `{name}` here is the template's real (generated) `name`, not its description.
4. Adding a template = `POST /api/templates` (`.typ` source + optional schema/sample; server mints the id, writes the files, hot-reloads) **or** drop files on disk by hand following the `<type>_temp_<nanoid>` convention then `POST /api/templates/sync`. Editing = overwrite the files + `POST /api/templates/sync`. Never a restart.

### Template identity

A template's `name` (its `.typ` stem, its `templates.name`, and the scope key for `template-data`) is a generated id: `doc_temp_<nanoid>` for a `documents/` template, `lbl_temp_<nanoid>` for a `labels/` one (`core::id::generate_template_name`, prefix constants in `core::constants::prefixes`). `POST /api/templates` mints these. The `_temp_` infix keeps a template *name* from being read as a `doc_<nanoid>` document *key*. Renaming the four original seed templates to ids was a one-time change; an operator with an existing DB that has `template-data` blobs or a name-keyed backend cache must re-point them (`UPDATE template_data SET template_name = …`).

## Logging

`src/core/logging/` mirrors the backend's module. `main.rs` calls `logging::init` (default filter `document_server=info,tower_http=info,info`, overridable via `RUST_LOG`). The outermost `logging::request::log_requests` layer honours the backend's `X-Request-Id` (else mints `req_…`), echoes it, opens a `request` span (`request_id`, `caller` = `internal`/`public`) and logs `http/request` + `http/response`; with `LOG_HTTP_BODIES=true` textual bodies are logged redacted (`logging::redact`) and capped, PDFs only by size. Renders log `render/start`, `render/done` (PDF bytes, duration), `render/failed`, `render/schema_invalid` and `render/typst_error`; `AppError` logs 4xx at WARN and 5xx at ERROR, and the `sqlx`/`serde_json` conversions log their error. `LOG_FORMAT=json` (set only by the Tauri desktop shell) emits local-offset JSON lines the shell ingests into the unified desktop activity log; `LOG_SQL` (`all`/`slow`/`off`) controls statement logging in `clients::sqlite`. In that mode `core::logging::control` reads **stdin** for `{"cmd":"log_mode",…}` lines so the desktop benchmark can switch logging at runtime; `LogSettings` holds `enabled`/`http_bodies`/`sql` in atomics read per event, so don't cache them. Bodies over 4x the cap are scrubbed as text rather than parsed. Keep new log lines structured (`category = "…", event = "…"` fields) — contract: `docs/logging.md` in the compose repo. Covered by `tests/logging_test.rs`.

## SQLite schema & queries

No migration framework — three `CREATE TABLE IF NOT EXISTS` statements in `clients/sqlite.rs` plus additive guarded `ALTER TABLE ... ADD COLUMN` lines for columns added later (`type`, `sample_data`). Revisit only for a genuinely breaking change. All queries are runtime-checked `sqlx::query[_as]` calls with bound parameters (never compile-time macros, so no `DATABASE_URL` needed to build). JSON columns use `sqlx::types::Json<T>`. **Trap**: never use `sqlite::memory:` where the pool may hand out >1 connection — each connection gets its own empty DB; tests use throwaway files (`tempfile::NamedTempFile`).

## Custom keys, timestamps, response envelope

Same conventions as `../backend`: `key: <prefix>_<nanoid>` primary key via `core::id::generate_id` (prefixes `tpl_`/`doc_`/`tdat_`); a template's *name* (not a key) is `<prefix>_temp_<nanoid>` via `core::id::generate_template_name` (prefixes `doc`/`lbl`); `created_at`/`updated_at` on every row, `ApiResponse<T>` success envelope `{success, data, message}` and `AppError` → `ErrorResponse` `{success, message, code, statusCode}` (variants NotFound/Validation/Internal/Custom; 422 via `unprocessable_entity(code, message)`).

## Code Comments

1. Module-level banner on every file stating what it owns and doesn't.
2. Doc comments on `pub`/`pub(crate)` items whose purpose isn't obvious.
3. Inline comments explain *why*, not *what*.

## Testing map

- `tests/render_test.rs` — full-stack renders of all four seed templates incl. schema rejection paths, plus the A4 Invoice's `logoUrl` path: a `data:` URI logo (and assert the recorded row blanked it), plus http(s) against `common::spawn_image_server()` (downloads + cleans up, back-to-back renders don't accumulate state, bad/oversized/non-image/`file://` URLs → 422, reprint re-fetches). Templates are resolved by `description`, not name.
- `tests/templates_test.rs` — list/get/single incl. `dataSchema`/`description` exposure, type filter, the `/sync` endpoint, and `POST /api/templates` create (401 without key; document + label happy paths with `doc_temp_`/`lbl_temp_` names; empty source / invalid schema / non-compiling `.typ` → 422 with files rolled back).
- `tests/template_data_test.rs` — CRUD roundtrip, unknown-template 404, merge semantics; resolves the seed template's real name via `tr_name`/`tr_key` helpers.
- `tests/documents_test.rs`, `tests/barcodes_test.rs`, `tests/qrcodes_test.rs`, `tests/openapi_test.rs` (asserts every module path is in the spec incl. `POST` on `/api/templates`; both PDF endpoints documented as `application/pdf`), `tests/response_format_test.rs`.
- `tests/common/mod.rs` — `spawn_app_isolated_templates()` (private writable copy of `templates/`, for tests that write into the tree) and `spawn_image_server()` (localhost `image/png` / `text/html` / 6 MiB routes).
- `src/clients/render.rs` unit tests — fastest check that a Typst/typst-as-lib upgrade still compiles the real templates.

## API docs & Postman

Swagger at `/docs`, spec at `/api-docs/openapi.json` — every handler registered via `routes!()` only. `postman/document-server.postman_collection.json` is hand-maintained: update it whenever a route changes (same rules as `../backend`'s collection).

## Build status

Done: core service, second template + barcode/QR packages, documents as first-class resources, internal-API-key auth, labels/documents split with credit-note template, **schema sidecars + strict validation**, **runtime sync/hot reload (ArcSwap engine)**, **stored shared/static template data with render-time merge**, barcodes/qrcodes modules, **generated template identity (`<type>_temp_<nanoid>`) + `POST /api/templates` create endpoint**, **A4 Invoice restyled as a branded/modern layout**, **render-time `logoUrl` (http(s) download or `data:` URI) → one-shot compile → cleanup**. Not started: format negotiation (PNG/SVG), render-result caching keyed on `(template_key, data)`.

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
- **Strict Sidecar Accompaniment**: Every new `.typ` template placed under `templates/documents/` or `templates/labels/` must ship a corresponding `<name>.schema.json` contract and `<name>.json` sample data. The schema `title` must be human-readable, as it acts as the template's descriptive name.
- **Zero Cache Bloat on Dynamic Assets**: Never compile dynamic images or user-supplied asset buffers through the shared long-lived `RenderEngine`. Always route external or payload-provided assets through `RenderEngine::compile_once` with cleanup guards.
- **Internal Authentication Guard**: Every mutating or document-rendering route must require `InternalCaller` and validate `X-Internal-Api-Key`. Never introduce default/fallback API keys in production or testing configuration.
- **Opaque Template Identity**: Template stems must follow generated prefix formats (`doc_temp_<nanoid>` or `lbl_temp_<nanoid>`). Never expose filesystem paths or raw stems as public user IDs.

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

