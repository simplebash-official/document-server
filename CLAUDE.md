# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

A standalone Rust/Axum service that renders PDFs from Typst templates (embedded via `typst-as-lib`, no shelling out to a `typst` CLI). It is a sibling of `../backend` (jana2u-pos's main API) but is **independent** — its own binary, its own local SQLite database file, its own port (`8090` by default, vs. `../backend`'s `8080`, so both can run locally at once). SQLite's role here is narrow: template *metadata* and rendered-*document* records only — never the `.typ` source, fonts, or the rendered PDF bytes themselves (those stay on disk or in the response body). **There is no authentication layer anywhere in this service — every route is open.**

This codebase follows jana2u-pos's backend conventions (`core/error.rs`, `core/response.rs`, module layering, `Config::from_env()`, etc.) wherever they genuinely transferred; see the sections below for the handful of deliberate deviations, including the database itself — jana2u-pos's backend is Mongo-backed, this service is SQLite-backed via `sqlx`, chosen so it needs no separate database service to install or run.

## Commands

- `cargo build` / `cargo run` (equivalent to `cargo run --bin document_server`)
- `cargo test` — every test needs no external service: `tests/render_test.rs` opens its own throwaway SQLite file per test (see `tests/common::spawn_app()`), and `clients::render::tests::warm_up_and_compile_receipt_produces_a_pdf` (a `#[cfg(test)]` unit test in `src/clients/render.rs`) needs no database at all — it exercises the real Typst compiler against the real `templates/`/`fonts/` directories.
- `cargo fmt` / `cargo fmt --check`
- `make check` — `cargo fmt --check` → `cargo clippy --all-targets --all-features -- -D warnings` → `cargo test`, stopping at the first failure. Run this after every change.

**Every change must be tested** — extend an existing test file covering the touched area, or create one following `tests/<area>_test.rs` naming.

Config loads from `.env` (via `dotenvy`) in the binary and every integration test; `Config::from_env()` fails fast on invalid vars, though every var currently has a default (see `.env.example`) — `DATABASE_URL` defaults to `sqlite://document_server.db`, a file created automatically on first run.

## Architecture

**Binary/library split**: `src/lib.rs` re-exports `app`, `clients`, `core`, `domain`, `modules` as a library crate; `src/main.rs` is a thin binary. This exists so `tests/*.rs` (a separate crate) can reuse the same modules via `document_server::...` — unlike jana2u-pos's backend, there is no `src/bin/` here (no seed scripts needed — the startup disk-sync in `main.rs` keeps `templates` metadata in step with what's on disk).

**Request flow**: `main.rs` calls `Config::from_env()` → `clients::sqlite::connect()` (opens/creates the database file, creates the `templates`/`documents` tables if missing) → `clients::render::RenderEngine::warm_up()` (builds the embedded Typst compiler) → `modules::templates::service::sync_templates_from_disk()` (upserts one `templates` row per `.typ` file found) → assembles `AppState { config: Arc<Config>, db: SqlitePool, render: Arc<RenderEngine> }` → `app::build_router(state)`.

### No authentication layer

Unlike jana2u-pos's backend (where auth is opt-in per handler via `CurrentUser`/`AdminUser` extractors), **this service has no authentication concept at all**: no extractor, no middleware, no OpenAPI security scheme, no `PUBLIC_ROUTES` allowlist, no `authorization_test.rs` equivalent. Every route registered in `app::build_router` is reachable by any caller, by design — there is nothing to allow-list against. If auth is ever needed here, it must be a deliberate new addition (see `app.rs`'s comment above `build_router`), not something half-implemented today.

### Feature modules (`src/modules/<name>/`)

Three modules, each layered `routes.rs` → `service` → `repository` (repository = SQLite/Typst only, no `AppError`, just `Option`/`Vec`/`Result` straight from the driver/compiler — the "not found"/"compile failed" → `AppError` translation happens one layer up, in `service`):

- **`render`** — the render endpoint itself (`POST /api/render/{templateKey}`). `repository/typst.rs` owns the JSON→Typst `Dict` bridge (`json_to_dict`, hand-written — `typst-as-lib` has no built-in `serde_json` integration) and the compile/export orchestration; `service` resolves the template, compiles via `compile_pdf`, and records a `documents` entry. `repository` is private; `service` is `pub(crate)` (see "Documents as first-class resources" below for who calls in) — `routes` is the only entry point reachable from outside the crate.
- **`templates`** — template metadata. **Real as of Phase 2**: `GET /` lists every template (active and inactive), `GET /{key}` fetches one. Unlike `render`/`documents`, there's no module-status stub — the collection root is itself the real "list" endpoint, so there's no unused path left for one (same convention jana2u-pos's `suppliers` module uses).
- **`documents`** — records of rendered documents. **Real as of Phase 3**: `GET /` lists every document (newest first), `GET /{key}` fetches one, `GET /{key}/pdf` reprints. Same no-module-status-stub convention as `templates`.

**Cross-module access**: `render::service::render_template` calls `templates::service::get_active_template_by_key` and `documents::service::record_document`; `documents::service::reprint_document` calls back the other way, into `templates::service::get_template_by_key` and `render::service::compile_pdf` — every one of these reached through the sibling module's `service`, never its `repository` (same rule as jana2u-pos). This is a genuine two-way dependency between `render` and `documents` (each calls into the other's `service`), which is fine in Rust — it's just fully-qualified function calls, not a `use`-cycle problem. `templates::service` is `pub` (not `pub(crate)`) specifically because `main.rs`'s startup sync needs `sync_templates_from_disk` directly, and a `pub(crate)` item in this library crate is invisible to `main.rs` (a separate binary crate). `render::service` and `documents::service` are both `pub(crate)` — sibling-module-only, never reachable from `main.rs` or outside the crate.

**Route file organization**: same section-banner convention as jana2u-pos — `// Router`, then feature-grouped sections (e.g. `render/routes.rs`: `// Module status`, `// Render`).

### The render pipeline & `RenderEngine`

`clients::render::RenderEngine` (parallel to `clients::sqlite` — a resource connected/built once at startup and shared read-only via `AppState`) owns the Typst engine's *lifecycle*: `warm_up()` reads every font under `FONTS_DIR` into memory (a genuine preload — `.fonts()` takes owned bytes) and points the engine at `TEMPLATES_DIR` via `with_file_system_resolver` (which resolves `.typ` files, and any assets they reference, from disk by relative path). **Trap to know about**: `typst-as-lib` wraps that resolver in its own `.into_cached()`, so a file is only ever actually read from disk once per process lifetime — a `.typ` file edited on disk while the server keeps running is *not* picked up until a restart rebuilds the engine (see `documents::service::reprint_document`'s doc comment, and "Reprint reflects the template as of last boot, not live edits" below). It then runs one best-effort trial compile per known template (empty input) purely to catch outright breakage (bad syntax, a missing asset, a bad font family) before the first real request — a trial failing because the template legitimately needs real input data is the common case (true for *every* template that actually takes input) and is logged at `debug`, not `warn`, so it doesn't look like a startup problem on every run.

Per-request orchestration (JSON → `Dict`, compile, export, error translation) is **not** in `clients::render` — it lives in `modules::render::repository::typst`, since that's request-scoped business logic, not resource lifecycle. `TypstEngineError::Compile` → `AppError::unprocessable_entity(codes::RENDER_VALIDATION_FAILED, ..)` (422); `TypstEngineError::Export` → `AppError::internal_with_code(.., codes::PDF_EXPORT_FAILED)` (500).

Templates stay file-based on disk (`templates/*.typ`); the `templates` table is queryable *metadata about* each file (name, description, `data_schema`, `is_active`), never a second source of truth for rendering — the `.typ` file's own header comment documents its JSON contract. `documents` rows store the request's `data`, not the rendered PDF bytes, so a reprint recompiles rather than replaying cached bytes — see "Documents as first-class resources" below for exactly what "recompiles" is (and isn't) a guarantee of.

See `templates/CLAUDE.md` for barcode/QR (`templates/lib/`) conventions.

### Documents as first-class resources (Phase 3)

`GET /api/documents` (list, newest first), `GET /api/documents/{key}` (one record), and `GET /api/documents/{key}/pdf` (reprint) are real as of Phase 3 — `documents` drops its module-status stub the same way `templates` did in Phase 2 (see "Feature modules" above).

`documents::service::reprint_document` recompiles the stored `data` against the template rather than replaying cached PDF bytes (none are stored — see spec §6.2) — the point being that a template fix benefits every historical document's reprint, not just future renders. It looks the template up via `templates::service::get_template_by_key`, **not** `render::service::render_template`'s `_active_` variant: deactivating a template stops *future* renders, it doesn't retroactively make history unreprintable.

**"Recompiles" means "against whatever `RenderEngine` loaded at its last `warm_up()`", not literally-live disk content** — see the file-resolver caching trap noted above. In practice this means: edit a template's `.typ` file, restart the server, and every historical document referencing that template reprints with the fix; edit it *without* restarting, and reprints keep using the pre-edit compiled version until the process restarts. True hot-reload (picking up an edit without a restart) is unimplemented Phase 4 work.

`render::service::compile_pdf` is the one place the actual "compile against data, translate Typst errors to the right `AppError`" logic lives — extracted out of `render_template` specifically so `reprint_document` reuses it instead of re-implementing the same `TypstEngineError` → `AppError` mapping a second time. This is why `render::service` is `pub(crate)` rather than private: `documents::service` (a sibling module) calls into it, same cross-module rule as everywhere else in this codebase.

### Response & error envelope — the one deliberate deviation

`AppError` (`core/error.rs`) is trimmed from jana2u-pos's six variants to four: `NotFound`, `Validation`, `Internal`, `Custom` — `Unauthorized`/`Forbidden` are dropped since nothing ever constructs them (no auth). `AppError::Validation` keeps its normal jana2u-pos-inherited 400 mapping (currently unused, available for a future genuine bad-request case). The one render-specific need for 422 (a Typst compile failure) uses `AppError::unprocessable_entity(code, message)` — also ported from jana2u-pos — rather than repurposing `Validation`'s status mapping. See `tests/response_format_test.rs::test_unprocessable_entity_is_422` for the regression guard.

`ApiResponse<T>`/`ErrorResponse` are otherwise identical to jana2u-pos's shapes (`{success, data, message}` / `{success, message, code, statusCode}`, camelCase) with one omission: there is no `processingTimeMs` splice, since jana2u-pos's comes from a `core::middleware::timing` layer that doesn't exist here.

**The render endpoint's success response is the one place the envelope itself doesn't apply** (see spec's design rationale, also documented inline in `modules/render/routes.rs`): `POST /api/render/{templateKey}` returns raw PDF bytes with `Content-Type: application/pdf` on success — not `Json<ApiResponse<_>>` — because JSON-wrapping a binary payload would mean base64-inflating it. Its *errors* still go through the standard `AppError`/`ErrorResponse` path; only the success shape differs. Its `#[utoipa::path]` documents this via `content_type = "application/pdf"` on the 200 response.

### Custom Prefixed Unique Model Keys

Every row across both tables (`templates`, `documents`) gets a `key: String` in `<prefix>_<nanoid>` form via `core::id::generate_id(prefix)` (`core::constants::prefixes::{TEMPLATE, DOCUMENT}` = `tpl`/`doc`), which is also the table's primary key — there's no separate autoincrement row id. `key` is the only external identifier — path params, request bodies, and response payloads all use it. Unlike jana2u-pos's `generate_id`, there is no `local_`-prefix guard here — that assert protects an offline-sync invariant (the frontend mints provisional `local_`-prefixed keys while offline) that doesn't apply: this service has no offline-first client and only ever mints keys from its two hardcoded prefixes.

### Timestamps

Every row carries `created_at`/`updated_at` (`chrono::DateTime<Utc>`, stored by `sqlx`'s `chrono` feature as SQLite `TEXT` in RFC3339 form) — same fields/semantics as jana2u-pos's `BsonDateTime` pair, different underlying representation.

### SQLite schema & queries

No migration framework: `clients::sqlite::connect()` runs two `CREATE TABLE IF NOT EXISTS` statements (`templates`, `documents` — see that file for the exact columns) after opening the pool. The schema is small and stable enough that this is simpler than a `migrations/` directory; revisit if a real schema change (not just an additive column) is ever needed. Every query is a plain runtime-checked `sqlx::query`/`query_as` call (`.bind(...)` per placeholder) — never the compile-time-checked `query!`/`query_as!` macros, so building this crate never needs a `DATABASE_URL` set or a database reachable at compile time. `templates`/`documents` row structs (`TemplateRow`/`DocumentRow` in each module's `model.rs`) derive `sqlx::FromRow`; JSON columns (`data_schema`, `data`) use `sqlx::types::Json<serde_json::Value>` rather than manual `serde_json::to_string`/`from_str` calls.

No joins are used, and none are expected to be needed: a `documents` row's `template_key` is resolved with a plain second `SELECT` when a caller needs both (same reasoning as jana2u-pos's "no aggregation pipeline unless it's a real cross-collection need").

### `clients/sqlite.rs`

`connect(database_url)` builds a `SqlitePoolOptions` pool (`max_connections(5)`) from a `SqliteConnectOptions` with `.create_if_missing(true)` (so a fresh checkout needs no manual `touch document_server.db` step) and `.journal_mode(SqliteJournalMode::Wal)` (so a concurrent reader doesn't get "database is locked" against a writer — the pool can and does hand out more than one connection). **Trap to avoid**: don't switch to `sqlite::memory:` for convenience anywhere a pool might open more than one connection — each connection to `:memory:` without shared-cache mode is its own separate empty database, so a second pooled connection silently sees no tables. `tests/openapi_test.rs`'s comment spells this out; both it and `tests/common::spawn_app()` use a throwaway file (`tempfile::NamedTempFile`) instead.

### `core/` breakdown

`config` (env-var loading, fail-fast on invalid — nothing is currently required, everything defaults), `constants` (`codes`, `modules`, `prefixes` — no `permissions`/`roles`/`http_status`, unused without auth), `error` (`AppError`/`AppResult`), `id` (`generate_id`), `openapi` (`ApiDoc` — no `SecurityAddon`, no `bearerAuth` scheme), `response` (`ApiResponse<T>`/`ErrorResponse`), `utils` (`module_status_response` only — no `parse_object_id` equivalent, since no route here ever takes a database-assigned id from a path param, only `key` strings; no `regex_escape`/`calculate_pagination`, unused until Phase 2's search/pagination needs arise).

## API docs (OpenAPI/Swagger)

Same `utoipa`/`utoipa-axum`/`utoipa-swagger-ui` setup as jana2u-pos — Swagger at `/docs`, raw spec at `/api-docs/openapi.json`. Every handler must be registered via `routes!(...)` inside a module's `router()` — never plain `axum::routing::get/post` — since only `routes!()`-registered handlers get collected into the spec. **Exception to watch for**: `POST /api/render/{templateKey}`'s `#[utoipa::path]` declares its 200 response as `content_type = "application/pdf", body = Vec<u8>` instead of the usual `ApiResponse<T>` schema — this is deliberate (see above), do not "fix" it to look like every other handler.

See `postman/CLAUDE.md` for the Postman collection conventions.

## Testing

Five tiers, no external service needed for any of them (a deliberate improvement over jana2u-pos's Mongo-backed test suite — no database process to start before running `cargo test`):

- `tests/render_test.rs` — full-stack, against a real throwaway SQLite file per test (`tests/common::spawn_app()`, which also does a real `RenderEngine::warm_up()` and disk-sync). Exercises the whole pipeline against both seed templates: `receipt` (successful render → PDF magic bytes + matching `documents` row) and `sticker` (successful render, including the vendored barcode/QR packages, end to end through HTTP; also the one with a required-field contract, so it's `sticker`, not `receipt`, that backs the missing-field → 422 case — `receipt.typ` is a fully static design that never reads `sys.inputs`, so it has no input to fail on). Plus unknown template → 404.
- `tests/templates_test.rs` — same full-stack style, scoped to `templates` as a resource: `GET /api/templates` lists both seed templates, `GET /api/templates/{key}` fetches one, unknown key → 404.
- `tests/documents_test.rs` — same full-stack style, scoped to `documents` as a resource (Phase 3): `GET /api/documents` lists every recorded document, `GET /api/documents/{key}` fetches one, `GET /api/documents/{key}/pdf` reprints (asserts PDF magic bytes), and 404 `DOCUMENT_NOT_FOUND` for all three on an unknown key.
- `tests/openapi_test.rs` — real router, real (throwaway-file) SQLite pool, real `RenderEngine::warm_up()`. Asserts every module path is listed and that both the render and reprint endpoints are documented as `application/pdf`.
- `tests/response_format_test.rs` — no router, no database. Calls `ApiResponse`/`AppError` directly.
- `src/clients/render.rs`'s `#[cfg(test)]` unit tests — no database, no router; compile the real `receipt.typ`/`sticker.typ` with real sample data and assert real PDF bytes come out (the `sticker` one is also the fastest way to check a `tiaoma`/`zebra` upgrade still compiles). The fastest way to check a Typst/typst-as-lib API change still works.

## Code Comments

Same rules as jana2u-pos, applied whenever a file is touched:

1. **Module-level banner** on every file — even a short one — stating what it owns and what it deliberately doesn't do.
2. **Doc comments (`///`) on `pub`/`pub(crate)` items** whose purpose isn't obvious from the signature.
3. **Inline comments explain *why*, not *what*** — e.g. why `documents` stores `data` instead of PDF bytes, why templates stay file-based while metadata lives in SQLite, why `key` doubles as the primary key.

## Build Phases

- **Phase 1 — Core service** (done): health, `render` end-to-end for one template (`receipt`), `templates`/`documents` internal plumbing + module-status stubs, OpenAPI, `AppError`/`ApiResponse` envelope, the Phase 1 test suite.
- **Phase 2 — Second template + barcode/QR** (done): `templates/sticker.typ` (print-size, 50mm×30mm) using the vendored `tiaoma`/`zebra` packages for a Code128 barcode and a QR code (`templates/lib/`); `templates`'s real read routes (`GET /`, `GET /{key}`); `tests/templates_test.rs` and the sticker cases in `render_test.rs`/`clients::render`'s unit tests.
- **Phase 3 — Documents as first-class resources** (done): `documents`'s real read routes (`GET /`, `GET /{key}`), `GET /api/documents/{key}/pdf` reprint endpoint (`render::service::compile_pdf` extracted so it and `render_template` share the same compile-error mapping), `tests/documents_test.rs`.
- **Phase 4 — not started**: format negotiation (PNG/SVG), template hot-reload without a restart (see the file-resolver caching trap under "The render pipeline & `RenderEngine`" — this is the concrete gap that phase would close), render result caching keyed on `(template_key, data)`.
