// Typst compilation only — no `AppError`, no SQLite. Converts a request's
// JSON body into the `Dict` typst-as-lib injects as `sys.inputs`, runs the
// compile, and exports to PDF bytes via `typst_pdf`. Error translation into
// `AppError` happens one layer up, in `modules::render::service`.

use typst::foundations::{Array, Dict, Str, Value};

use crate::clients::render::RenderEngine;

pub(crate) enum TypstEngineError {
    Compile(String),
    Export(String),
}

/// Converts a JSON request body into the `Dict` a template reads via
/// `#import sys: inputs`. A non-object top-level payload has nowhere to go
/// as `sys.inputs` (which is always a dict) — it compiles to an empty dict,
/// which then fails the template's own field access, surfacing as the same
/// `TypstEngineError::Compile` path as any other malformed payload rather
/// than needing a special case here.
fn json_to_dict(value: serde_json::Value) -> Dict {
    match json_to_value(value) {
        Value::Dict(d) => d,
        _ => Dict::new(),
    }
}

fn json_to_value(value: serde_json::Value) -> Value {
    match value {
        serde_json::Value::Null => Value::None,
        serde_json::Value::Bool(b) => Value::Bool(b),
        serde_json::Value::Number(n) => n
            .as_i64()
            .map(Value::Int)
            .unwrap_or_else(|| Value::Float(n.as_f64().unwrap_or_default())),
        serde_json::Value::String(s) => Value::Str(Str::from(s)),
        serde_json::Value::Array(items) => {
            Value::Array(items.into_iter().map(json_to_value).collect::<Array>())
        }
        serde_json::Value::Object(map) => Value::Dict(
            map.into_iter()
                .map(|(k, v)| (Str::from(k), json_to_value(v)))
                .collect::<Dict>(),
        ),
    }
}

/// Compiles `template_name` (a `.typ` filename stem) against `input`, then
/// exports the result to PDF bytes. Synchronous and CPU-bound — Typst
/// compilation isn't async I/O — called directly from `render::service`'s
/// async fn for Phase 1's expected "low-single-digit milliseconds once
/// warm" render cost (see spec §9); move it onto `spawn_blocking` if a
/// specific template later turns out to be CPU-heavy enough to matter.
pub(crate) fn render_pdf(
    engine: &RenderEngine,
    template_name: &str,
    input: serde_json::Value,
) -> Result<Vec<u8>, TypstEngineError> {
    let dict = json_to_dict(input);
    let warned = engine.compile(template_name, dict);

    for warning in warned.warnings.iter() {
        tracing::warn!(?warning, template = template_name, "typst compile warning");
    }

    let doc = warned
        .output
        .map_err(|err| TypstEngineError::Compile(err.to_string()))?;

    typst_pdf::pdf(&doc, &Default::default())
        .map_err(|err| TypstEngineError::Export(format!("{err:?}")))
}
