// Template packages: publish a zip as a new immutable template version,
// install an exported archive under its given identity (desktop
// replication), export, grant a family to tenants, retire a family, and
// preview a package's sample. Templates themselves (lookup, visibility) live
// in `modules::templates`; this module owns the archive format and the files
// under `TEMPLATES_DIR/packages/`.
pub(crate) mod archive;
pub mod routes;
pub(crate) mod service;
