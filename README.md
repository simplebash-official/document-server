# SimpleBash Document Server

[![CI](https://github.com/simplebash-official/document-server/actions/workflows/ci.yml/badge.svg)](https://github.com/simplebash-official/document-server/actions/workflows/ci.yml)
[![License: AGPL v3](https://img.shields.io/badge/License-AGPL_v3-blue.svg)](LICENSE)

A small Rust service that turns JSON into **PDF documents** using
[Typst](https://typst.app) templates, plus **barcodes and QR codes** as SVG.
It renders the invoices, receipts, credit notes, reports and product labels for
[SimpleBash POS](https://github.com/simplebash-official/pos-backend), but it
isn't tied to the POS: any app can publish a template here and send it data.

- **Fast and self-contained** — Typst is compiled in-process (no external
  `typst` binary, no headless browser). Templates and fonts ship with the image.
- **Templates hot-reload** — add or edit a template and call
  `POST /api/templates/sync`; no restart.
- **Every template has a contract** — a JSON Schema next to the `.typ` file.
  Payloads are validated before rendering and rejected with field-level errors.

## The SimpleBash POS family

| Repository | What it is |
|---|---|
| [pos-backend](https://github.com/simplebash-official/pos-backend) | REST API — Rust / Axum, SQLite or MongoDB |
| [pos-frontend](https://github.com/simplebash-official/pos-frontend) | The cashier & back-office UI — React / Vite / Mantine |
| **document-server** (this repo) | Renders invoices, receipts, reports and labels to PDF — Rust / Typst |
| [pos-desktop](https://github.com/simplebash-official/pos-desktop) | Windows / macOS / Linux app (Tauri) that bundles all three |

## Included templates

| Template | Kind | Use |
|---|---|---|
| A4 Invoice | document | Branded A4 invoice, optional shop logo |
| Thermal Receipt | document | 58 mm / 80 mm till receipts |
| Credit Note | document | Returns and refunds |
| Analytics Report | document | Sales / profit report for printing |
| Product Sticker Label | label | Shelf / product stickers with barcode |

They live in [`templates/documents/`](templates/documents) and
[`templates/labels/`](templates/labels): each is a `.typ` file, a
`.schema.json` contract and a `.json` sample.

The `.typ` files here are **plain examples**. SimpleBash's own designs are
kept in a private repo and laid over these at build time (same filenames):
`scripts/assemble-templates.sh <your-templates-dir> <out-dir>`, then point
`TEMPLATES_DIR` at `<out-dir>`. Use the same mechanism for your own designs.

## Getting started

### Prerequisites

- [Rust](https://rustup.rs) stable (1.85 or newer)

### Run it locally

```bash
git clone https://github.com/simplebash-official/document-server.git
cd document-server
cp .env.example .env
# set INTERNAL_API_KEY, e.g. to the output of: openssl rand -hex 32
cargo run                 # http://localhost:8090
```

Open **http://localhost:8090/docs** for the interactive API reference.

### Or with Docker

```bash
export INTERNAL_API_KEY=$(openssl rand -hex 32)
docker compose up -d      # http://localhost:8090
```

### Render something

```bash
# List templates and copy a template key (tpl_…) from the response
curl http://localhost:8090/api/templates

# Render it to a PDF
curl -X POST "http://localhost:8090/api/render/<templateKey>" \
  -H "X-Internal-Api-Key: $INTERNAL_API_KEY" \
  -H "Content-Type: application/json" \
  -d @templates/documents/<template-name>.json \
  -o out.pdf

# Barcodes and QR codes need no key
curl "http://localhost:8090/api/barcodes/generate?content=SKU-9900&symbology=code128" -o code.svg
```

## API

| Route | Auth | Purpose |
|---|---|---|
| `GET /api/health` | — | Health and version |
| `GET /api/templates`, `GET /api/templates/{key}` | — | List templates with their schema and sample data |
| `POST /api/templates` | key | Upload a new template (`.typ` + schema + sample) |
| `POST /api/templates/sync` | key | Re-scan the templates folder and hot-reload |
| `POST /api/render/{templateKey}` | key | Render JSON to a PDF |
| `GET /api/documents`, `GET /api/documents/{key}`, `GET /api/documents/{key}/pdf` | key | Render history and reprints |
| `PUT / GET / DELETE /api/template-data/{template}/{dataKey}` | key | Store shared data (shop profile, bank details) merged into every render |
| `POST/GET /api/barcodes/generate`, `POST/GET /api/qrcodes/generate` | — | SVG barcodes (Code128, Code39, EAN-8, EAN-13) and QR codes |
| `GET /docs`, `GET /api-docs/openapi.json` | — | Swagger UI and OpenAPI spec |

"key" means the `X-Internal-Api-Key` header, equal to the server's
`INTERNAL_API_KEY`. Multi-tenant callers may also send `X-Tenant-Key` so each
tenant's stored template data stays separate.

A payload field named `logoUrl` can be an `http(s)` URL or a `data:image/…`
URI. Remote images are size-capped, time-limited, must really be images, and
by default may not point at private or internal addresses.

## Configuration

| Variable | Default | Purpose |
|---|---|---|
| `INTERNAL_API_KEY` | **required** | Shared secret (16+ characters, not a placeholder) |
| `PORT` / `BIND_ADDR` | `8090` / `0.0.0.0` | Listener. The desktop app uses `127.0.0.1` |
| `DATABASE_URL` | `sqlite://document_server.db` | SQLite file for template metadata and render history |
| `TEMPLATES_DIR` / `FONTS_DIR` | `templates` / `fonts` | Where templates and fonts are read from |
| `MAX_RENDER_BODY_BYTES` | `5242880` (5 MiB) | Largest render request |
| `REMOTE_IMAGE_FETCH_ENABLED` | `true` | Allow `logoUrl` downloads |
| `REMOTE_IMAGE_MAX_BYTES` / `REMOTE_IMAGE_TIMEOUT_SECS` | 5 MiB / `10` | Limits for those downloads |
| `ALLOW_PRIVATE_IP_IMAGES` | `false` | Allow downloads from private / internal addresses (local development only) |
| `APP_ENV` | `development` | Reported by `/api/health` |
| `RUST_LOG`, `LOG_FORMAT`, `LOG_HTTP_BODIES`, `LOG_SQL` | — | Logging |

See [`.env.example`](.env.example).

## Adding a template

1. Write `templates/documents/<name>.typ` (or `templates/labels/`). Inputs
   arrive as Typst `sys.inputs`.
2. Add `<name>.schema.json` (JSON Schema; its `title` becomes the template's
   display name) and a `<name>.json` sample.
3. `POST /api/templates/sync` (or restart). Or upload all three with
   `POST /api/templates`, which also test-compiles against the sample.

## Development

```bash
make check     # cargo fmt --check, clippy -D warnings, cargo test
cargo test     # no external services needed — each test uses its own temp database
```

Project layout:

```
src/
  app.rs              router, CORS, Swagger
  clients/            SQLite, Typst render engine, remote image fetching
  core/               config, API-key middleware, errors, logging
  modules/            render, templates, documents, template_data, barcodes, qrcodes
templates/            Typst templates, schemas and samples (+ lib/ for shared Typst packages)
fonts/                bundled fonts (Noto Sans, SIL Open Font License)
tests/                integration tests
```

## Contributing

Issues and pull requests are welcome. Run `make check` before opening a PR and
add a test for your change. New or changed templates need an up-to-date
`.schema.json` and sample.

## Security

Please report vulnerabilities privately through
[GitHub's "Report a vulnerability"](https://github.com/simplebash-official/document-server/security/advisories/new),
not in a public issue.

## License

[GNU Affero General Public License v3.0](LICENSE). Bundled third-party
assets keep their own licences: Noto Sans ([`fonts/OFL.txt`](fonts/OFL.txt))
and the Typst packages under [`templates/lib/`](templates/lib).
