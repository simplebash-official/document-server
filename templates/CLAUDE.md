# CLAUDE.md (templates/)

## Barcode/QR (`templates/lib/`)

`templates/sticker.typ` (the Phase 2 second template — a 50mm×30mm print label) draws a Code128 barcode via the vendored `tiaoma` package and a QR code via the vendored `zebra` package, both under `templates/lib/`. Both are **vendored copies**, not `@preview` imports resolved over the network — `RenderEngine` deliberately has no package resolver, only `with_file_system_resolver`, so a template can only ever `#import` something that's actually checked into this repo. This is the same determinism argument as bundling fonts explicitly (see spec §6.1): render output must not depend on package-registry availability or on which version happened to be cached on whichever machine runs the server. See `templates/lib/README.md` for exact provenance/upgrade instructions. Both packages happen to use a WASM plugin (`plugin("...wasm")`) for their encoding math — that resolves through the same file-system resolver as any other asset, no special engine support was needed.

`RenderEngine::warm_up`'s `.typ` scan only looks at the top level of `TEMPLATES_DIR`, so `templates/lib/**` is never mistaken for a render-able template — `clients::render::tests::warm_up_and_compile_sticker_produces_a_pdf` asserts exactly that (`"lib"` never appears in `known_templates()`).
