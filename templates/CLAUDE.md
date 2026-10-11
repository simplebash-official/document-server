# CLAUDE.md (templates/)

## Layout & discovery

- `templates/documents/*.typ` — standard printable documents; `templates/labels/*.typ` — thermal stickers. A bare `.typ` at the top level would also be discovered as a `document`. Only these two subdirectory names carry meaning to the scanner (`clients::render::scan_templates`): anything else becomes a `document` too.
- `templates/lib/` is skipped by the scan — vendored Typst packages, never renderable templates.

## Public examples vs private designs

The `documents/*.typ` and `labels/*.typ` files in this repo are **plain public
examples**. The real SimpleBash designs live in the private
`simplebash-official/document-templates` repo (`../document-templates` locally)
with the **same filenames**, and `scripts/assemble-templates.sh` overlays them at
build time (prod `deploy.yml`, desktop `build-sidecars.sh`). Schemas, samples and
`lib/` are public and shared by both. Never copy a private `.typ` into this repo:
assemble into a directory outside it and set `TEMPLATES_DIR`. A schema change
must keep both the example and the private `.typ` compiling.

## Naming

A template's filename stem **is** its identity (`templates.name`, and the scope key for
stored `template-data` blobs). It is an opaque generated id, not a human word:
`doc_temp_<nanoid>` for a `documents/` template, `lbl_temp_<nanoid>` for a `labels/` one
(`core::id::generate_template_name`). `POST /api/templates` mints these; adding a
template by hand means following the same convention (then `POST /api/templates/sync`).

The **human name** of a template lives in its `<name>.schema.json` `title` field. The
sync pass copies that `title` into the `templates.description` column, which is what
`GET /api/templates` and integration clients read to tell templates apart. Seed
templates today: `doc_temp_vEf0Y7jQHQj2rIuO` = "A4 Invoice" (a branded/modern layout
with an optional `logoUrl` — see below), `doc_temp_ZahcoYg8xCQjuc8c` = "A4 Invoice Modern"
(the same input contract as A4 Invoice, a different design; its schema/sample are copies
with only the `title` changed — keep the two contracts identical), `doc_temp_4pz79z5iba7TcEIp` = "Thermal
Receipt", `doc_temp_5DHl8hUQTX3oLBSR` = "Credit Note", `lbl_temp_qklcWIolwoFFN3xk` =
"Product Sticker Label".

Both A4 invoices' `logoUrl` field accepts an **http(s) URL** (downloaded at render
time) **or** a `data:image/…;base64,…` URI (decoded inline, no network) — SimpleBash POS
sends its shop logo the second way. The server turns either into a local file for
that one compile and discards it; a `data:` `logoUrl` is also blanked from the
recorded `documents` row so the table doesn't carry the blob.

## Sidecar conventions (per template)

Every `<name>.typ` may have two sidecars, both optional and both read by `scan_templates`:

1. **`<name>.schema.json`** — JSON Schema draft-07 input contract. This is the machine-readable integration surface: it's synced into the `templates.data_schema` column and served as `Template.dataSchema` on `GET /api/templates(/{key})`, and render payloads are validated against it before compilation (`422 RENDER_VALIDATION_FAILED`). Conventions: enforce `required` + field types; keep `"additionalProperties": true` so callers can carry extra context without being rejected. When you add a required field to a `.typ`, update its schema **and** SimpleBash POS's backend builder in the same change — that service pre-validates against the same schema and will refuse to send non-conforming payloads.
2. **`<name>.json`** — worked sample data, exposed as `Template.data`. Documentation only, never enforced.

Historical note: samples once lived in the `data_schema` DB column under a misleading name; the sync pass now writes the real schema there and keeps samples in `sample_data`.

## Document kinds & the POS template dropdowns

Every `documents/*.schema.json` declares what the template is *for* with a top-level
`"x-document-kind"` (unknown keywords are harmless annotations to the `jsonschema` validator;
the marker rides along in `dataSchema` on `GET /api/templates`). Kinds in use: `a4-invoice`,
`thermal-receipt`, `credit-note`, `analytics-report`. SimpleBash POS's backend lists the
**active** templates of a kind in its "Document Designs" dropdowns (Settings and the print
preview), shows each schema `title` as the label, and renders the template's `.json` sample
as the preview — so a new template needs no backend or frontend change.

Rules that keep that safe (guarded by `tests/templates_test.rs` and, in the backend,
`print_payload::schema_drift_tests`):

- **All templates of one kind accept the kind's standard payload** (the one the backend's
  builder sends). Extra optional fields are fine (`additionalProperties: true`); extra
  *required* fields are not. A `thermal-receipt` must support both `paperWidthMm` 58 and 80.
- **Titles are unique** across documents and labels: the title is the dropdown label and the
  backend's lookup key for the built-in default of each kind.
- **The sample validates against its own schema** and compiles; keep `logoUrl` out of it or
  expect the backend to drop it for previews.
- The list endpoint returns inactive rows too and sorts by id — consumers must filter on
  `isActive` and sort by title themselves.

### How to add a template (the whole checklist)

1. Draft it in `document-templates/lab/…` and compile it with `typst`.
2. Promote it: private `document-templates/documents/doc_temp_<16 alnum>.typ` (self-contained,
   fonts from `fonts/`, imports only from `../lib/`).
3. Here: `documents/doc_temp_<same id>.schema.json` (unique `title`, `x-document-kind`, the
   kind's standard contract), `.json` sample, and a plain public example `.typ`.
4. `cargo test` here and in `pos/backend` (schema drift), then deploy / build the desktop
   (`assemble-templates.sh` overlays the private `.typ`). It appears in the dropdowns.

## Barcode/QR (`templates/lib/`)

The Product Sticker Label template (`templates/labels/lbl_temp_qklcWIolwoFFN3xk.typ`) draws a Code128 barcode via the vendored `tiaoma` package and a QR code via the vendored `zebra` package, both under `templates/lib/`. Both are **vendored copies**, not `@preview` imports resolved over the network — `RenderEngine` deliberately has no package resolver, only `with_file_system_resolver`, so a template can only ever `#import` something that's actually checked into this repo. This is the same determinism argument as bundling fonts explicitly: render output must not depend on package-registry availability or on which version happened to be cached on whichever machine runs the server. See `templates/lib/README.md` for exact provenance/upgrade instructions. Both packages use a WASM plugin (`plugin("...wasm")`) for their encoding math — that resolves through the same file-system resolver as any other asset, no special engine support was needed.
