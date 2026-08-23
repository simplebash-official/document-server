# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

A standalone Rust/Axum service that renders PDFs from Typst templates (embedded via `typst-as-lib`, no shelling out to a `typst` CLI). It is a **shared document service**: jana2u-pos's backend (`../backend`) is its primary caller today, and future projects are expected to integrate the same way — publish a template plus a machine-readable data contract here, then feed JSON from anywhere. It has its own binary, its own local SQLite database file, its own port (`8090` by default, vs. `../backend`'s `8080`). SQLite's role is narrow: template *metadata* (including each template's input contract), rendered-*document* records, and stored shared/static template *data* blobs — never the `.typ` source, fonts, or the rendered PDF bytes themselves.

**Access model — one shared secret, not "no auth"**: every route meant only for backend callers (`render`, `documents`, `templates/sync`, all of `template-data`) takes the `InternalCaller` extractor (`core/middleware/auth.rs`) and requires `X-Internal-Api-Key: $INTERNAL_API_KEY`. `INTERNAL_API_KEY` has deliberately no default — an unset secret fails startup rather than booting wide open. Genuinely public reads: `GET /api/health`, Swagger, `GET /api/templates`(list + single). There are no users/roles/tokens; a richer auth model would be a deliberate new addition.

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

- **`render`** — `POST /api/render/{templateKey}` (`InternalCaller`). The one envelope deviation: success is raw PDF bytes (`Content-Type: application/pdf`); errors use the standard `ErrorResponse`. Per request: resolve active template → deep-merge stored `template-data` blobs under the payload (**request wins**) → validate merged payload against the template's schema when it ships one (`jsonschema`, 422 `RENDER_VALIDATION_FAILED` naming fields) → compile/export → record a `documents` row holding the *request's* original payload (so reprints re-apply the merge against current stored state).
- **`templates`** — `GET /` (+`?type=document|label`) and `GET /{key}` are open reads exposing `key`, `name`, `type`, `dataSchema` (the machine-readable input contract — what integration clients fetch) and `data` (a worked sample). `POST /sync` (`InternalCaller`) = runtime rescan: warm up fresh engine, upsert metadata, **deactivate rows whose `.typ` vanished** (never delete — keys are stable and history references them), swap engine.
- **`documents`** — render records: `GET /` newest-first, `GET /{key}`, `GET /{key}/pdf` reprint (`InternalCaller`). Reprint recompiles stored `data` via `render::service::compile_pdf` (the shared compile+error-mapping fn) and re-applies the stored-data merge.
- **`template_data`** — stored shared/static per-template JSON blobs (shop profile, bank details…): `PUT/GET/DELETE /api/template-data/{templateName}/{dataKey}` + `GET /api/template-data/{templateName}` (all `InternalCaller`). PUT requires the template name to exist; repeated PUT overwrites; DELETE echoes the removed blob. Blobs are keyed by template *name* (the stable disk identity).
- **`barcodes` / `qrcodes`** — SVG barcode/QR generation utilities (no DB).

### Data contracts (schema sidecars)

Every template that wants validated input ships `<name>.schema.json` next to its `.typ` (JSON Schema draft-07; drafts auto-detected from `$schema`). Convention: `required` + types enforced, `additionalProperties: true` (unknown fields allowed so callers can carry extra context). Validation happens **twice by design**: document-server validates before compiling, and jana2u-pos's backend pre-validates against the same published schema before sending — both answer 422 `RENDER_VALIDATION_FAILED`, so callers treat them identically. Templates without a sidecar behave exactly as before (Typst errors are the only failure signal).

The `<name>.json` sidecar remains optional *sample data* (exposed as `Template.data`) — documentation, never enforced. Historical note: the `templates.data_schema` column once held samples under a misleading name; the sync pass now writes the real schema there and keeps samples in the additive `sample_data` column.

### Integration contract with `../backend` (and any future client)

1. Client fetches `GET /api/templates` → caches `{name → key, dataSchema}`.
2. Client builds a payload per `dataSchema`, pre-validates, then `POST /api/render/{templateKey}` with `X-Internal-Api-Key`.
3. Optional: `PUT /api/template-data/{name}/{key}` to store shared/static fields server-side instead of resending them (and sharing them across projects).
4. Adding/editing a template = drop files on disk here, then `POST /api/templates/sync` — no restart.

## SQLite schema & queries

No migration framework — three `CREATE TABLE IF NOT EXISTS` statements in `clients/sqlite.rs` plus additive guarded `ALTER TABLE ... ADD COLUMN` lines for columns added later (`type`, `sample_data`). Revisit only for a genuinely breaking change. All queries are runtime-checked `sqlx::query[_as]` calls with bound parameters (never compile-time macros, so no `DATABASE_URL` needed to build). JSON columns use `sqlx::types::Json<T>`. **Trap**: never use `sqlite::memory:` where the pool may hand out >1 connection — each connection gets its own empty DB; tests use throwaway files (`tempfile::NamedTempFile`).

## Custom keys, timestamps, response envelope

Same conventions as `../backend`: `key: <prefix>_<nanoid>` primary key via `core::id::generate_id` (prefixes `tpl_`/`doc_`/`tdat_`), `created_at`/`updated_at` on every row, `ApiResponse<T>` success envelope `{success, data, message}` and `AppError` → `ErrorResponse` `{success, message, code, statusCode}` (variants NotFound/Validation/Internal/Custom; 422 via `unprocessable_entity(code, message)`).

## Code Comments

1. Module-level banner on every file stating what it owns and doesn't.
2. Doc comments on `pub`/`pub(crate)` items whose purpose isn't obvious.
3. Inline comments explain *why*, not *what*.

## Testing map

- `tests/render_test.rs` — full-stack renders of all four seed templates incl. schema rejection paths (missing required field names the field; mistyped field rejected; unknown extra field allowed).
- `tests/templates_test.rs` — list/get/single incl. `dataSchema` exposure, type filter, and the `/sync` endpoint (401 without key; deactivates rows missing from disk; picks up a new `.typ` in a temp dir and renders it without restart, then deactivates on removal).
- `tests/template_data_test.rs` — CRUD roundtrip, unknown-template 404, merge semantics (stored blob supplies required fields; request wins over stored bad values; reprint re-applies current stored state).
- `tests/documents_test.rs`, `tests/barcodes_test.rs`, `tests/qrcodes_test.rs`, `tests/openapi_test.rs` (asserts every module path incl. `/api/templates/sync` is in the spec; both PDF endpoints documented as `application/pdf`), `tests/response_format_test.rs`.
- `src/clients/render.rs` unit tests — fastest check that a Typst/typst-as-lib upgrade still compiles the real templates.

## API docs & Postman

Swagger at `/docs`, spec at `/api-docs/openapi.json` — every handler registered via `routes!()` only. `postman/document-server.postman_collection.json` is hand-maintained: update it whenever a route changes (same rules as `../backend`'s collection).

## Build status

Done: core service, second template + barcode/QR packages, documents as first-class resources, internal-API-key auth, labels/documents split with credit-note template, **schema sidecars + strict validation**, **runtime sync/hot reload (ArcSwap engine)**, **stored shared/static template data with render-time merge**, barcodes/qrcodes modules. Not started: format negotiation (PNG/SVG), render-result caching keyed on `(template_key, data)`.

## graphify

This project has a knowledge graph at graphify-out/. For codebase questions run `graphify query "<question>"` first; `graphify path "<A>" "<B>"` for relationships; `graphify explain "<concept>"` for focused concepts; read `GRAPH_REPORT.md` only when those fall short. After modifying code, run `graphify update .` (AST-only, no API cost).
