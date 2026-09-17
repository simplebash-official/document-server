use nanoid::nanoid;

/// Generates a unique key prefixed with a model identifier and separated by
/// an underscore. Format: `<prefix>_<nanoid>` (e.g., `tpl_x7K9mP2...`,
/// `doc_B8nL1q...`). Unlike SimpleBash POS's backend, there is no `local_`-prefix
/// guard here: that assert protects an offline-sync invariant (the frontend
/// mints provisional `local_`-prefixed keys while offline) that doesn't
/// exist in this service — document_server has no offline-first client and mints
/// every key itself, from exactly two hardcoded prefixes (`tpl`, `doc`),
/// neither of which can collide with `local_`.
pub fn generate_id(prefix: &str) -> String {
    assert!(!prefix.is_empty(), "Key prefix must not be empty");
    format!("{}_{}", prefix, nanoid!(16))
}

/// Generates a template's stable on-disk identity (its `.typ` stem and
/// `templates.name`): `<prefix>_temp_<nanoid16>`, e.g.
/// `doc_temp_Xk3nR8Qp2vT9wLm0`. `prefix` is `doc` for documents / `lbl` for
/// labels — see `core::constants::prefixes::TEMPLATE_NAME_*`. Distinct from
/// `generate_id`'s `<prefix>_<nanoid>` keys: the `_temp_` infix keeps a
/// template *name* from being mistaken for a `doc_...` document *key*.
pub fn generate_template_name(prefix: &str) -> String {
    assert!(!prefix.is_empty(), "Template name prefix must not be empty");
    format!("{}_temp_{}", prefix, nanoid!(16))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_id_creates_prefixed_key() {
        let key = generate_id("tpl");
        assert!(key.starts_with("tpl_"));
        assert_eq!(key.len(), 4 + 16);
    }

    #[test]
    fn generate_template_name_creates_temp_infixed_name() {
        let name = generate_template_name("doc");
        assert!(name.starts_with("doc_temp_"));
        assert_eq!(name.len(), "doc_temp_".len() + 16);
        // Must not look like a `doc_<nanoid>` document key.
        assert_ne!(name.split('_').count(), 2);
    }
}
