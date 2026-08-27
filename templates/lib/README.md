# Vendored Typst packages

`tiaoma/` and `zebra/` are vendored copies of two MIT-licensed Typst Universe packages (`@preview/tiaoma:0.3.0` and `@preview/zebra:0.1.0`), used by the Product Sticker Label template (`../labels/lbl_temp_qklcWIolwoFFN3xk.typ`) for barcode/QR generation:

- **`tiaoma`** — [Enter-tainer/zint-wasi](https://github.com/Enter-tainer/zint-wasi), Zint compiled to a WASM plugin. Used here for `code128(...)`.
- **`zebra`** — [rojul/typst-zebra](https://github.com/rojul/typst-zebra), a WASM plugin for QR/Data Matrix encoding plus native-Typst path drawing. Used here for `qrcode(...)`.

Vendored (copied from each package's published `packages.typst.org` tarball) rather than resolved at compile time via `@preview` imports, because `clients::render::RenderEngine` deliberately has no network package resolver — see spec §6.1/CLAUDE.md: fonts and templates are bundled explicitly so render output is identical regardless of which machine runs the server, and that same determinism argument applies to any Typst dependency a template pulls in, not just fonts. Each subdirectory keeps its package's own `LICENSE` file alongside the vendored source, per MIT's attribution requirement.

Not picked up by `RenderEngine::warm_up`'s `.typ` scan (which is why they can safely `#import` each other without ending up as `render`-able "templates" themselves) — that scan only looks at the top level of `TEMPLATES_DIR`, not this subdirectory.

To upgrade either package: download the new version's tarball from `https://packages.typst.org/preview/<name>-<version>.tar.gz`, replace this subdirectory's contents, and re-run `tests/render_test.rs`'s sticker case.
