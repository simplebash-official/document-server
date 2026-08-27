//! Standard entity ID prefixes used for generating prefixed NanoID model keys.

pub const TEMPLATE: &str = "tpl";
pub const DOCUMENT: &str = "doc";
pub const TEMPLATE_DATA: &str = "tdat";

/// Prefixes for a template's generated *name* (its stable on-disk identity /
/// `.typ` stem), as opposed to `TEMPLATE` above which prefixes the row's
/// `key`. A name looks like `doc_temp_<nanoid>` / `lbl_temp_<nanoid>` — see
/// `core::id::generate_template_name`. Chosen by template type.
pub const TEMPLATE_NAME_DOCUMENT: &str = "doc";
pub const TEMPLATE_NAME_LABEL: &str = "lbl";
