# CLAUDE.md (templates/)

## Layout & discovery

- `templates/documents/*.typ` — standard printable documents (a4-invoice, thermal-receipt, credit-note); `templates/labels/*.typ` — thermal stickers (`sticker`). A bare `.typ` at the top level would also be discovered as a `document`. Only these two subdirectory names carry meaning to the scanner (`clients::render::scan_templates`): anything else becomes a `document` too.
- `templates/lib/` is skipped by the scan — vendored Typst packages, never renderable templates.

## Sidecar conventions (per template)

Every `<name>.typ` may have two sidecars, both optional and both read by `scan_templates`:

1. **`<name>.schema.json`** — JSON Schema draft-07 input contract. This is the machine-readable integration surface: it's synced into the `templates.data_schema` column and served as `Template.dataSchema` on `GET /api/templates(/{key})`, and render payloads are validated against it before compilation (`422 RENDER_VALIDATION_FAILED`). Conventions: enforce `required` + field types; keep `"additionalProperties": true` so callers can carry extra context without being rejected. When you add a required field to a `.typ`, update its schema **and** jana2u-pos's backend builder in the same change — that service pre-validates against the same schema and will refuse to send non-conforming payloads.
2. **`<name>.json`** — worked sample data, exposed as `Template.data`. Documentation only, never enforced.

Historical note: samples once lived in the `data_schema` DB column under a misleading name; the sync pass now writes the real schema there and keeps samples in `sample_data`.

## Barcode/QR (`templates/lib/`)

`templates/labels/sticker.typ` draws a Code128 barcode via the vendored `tiaoma` package and a QR code via the vendored `zebra` package, both under `templates/lib/`. Both are **vendored copies**, not `@preview` imports resolved over the network — `RenderEngine` deliberately has no package resolver, only `with_file_system_resolver`, so a template can only ever `#import` something that's actually checked into this repo. This is the same determinism argument as bundling fonts explicitly: render output must not depend on package-registry availability or on which version happened to be cached on whichever machine runs the server. See `templates/lib/README.md` for exact provenance/upgrade instructions. Both packages use a WASM plugin (`plugin("...wasm")`) for their encoding math — that resolves through the same file-system resolver as any other asset, no special engine support was needed.
