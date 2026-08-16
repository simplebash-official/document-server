use nanoid::nanoid;

/// Generates a unique key prefixed with a model identifier and separated by
/// an underscore. Format: `<prefix>_<nanoid>` (e.g., `tpl_x7K9mP2...`,
/// `doc_B8nL1q...`). Unlike jana2u-pos's backend, there is no `local_`-prefix
/// guard here: that assert protects an offline-sync invariant (the frontend
/// mints provisional `local_`-prefixed keys while offline) that doesn't
/// exist in this service — document_server has no offline-first client and mints
/// every key itself, from exactly two hardcoded prefixes (`tpl`, `doc`),
/// neither of which can collide with `local_`.
pub fn generate_id(prefix: &str) -> String {
    assert!(!prefix.is_empty(), "Key prefix must not be empty");
    format!("{}_{}", prefix, nanoid!(16))
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
}
