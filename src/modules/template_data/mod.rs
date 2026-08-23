// Stored shared/static template data (`template_data` table): named JSON
// blobs per template that get deep-merged into every render's payload.
// Layered like every other module — routes → service → repository — with
// `repository` private to this tree. `service` is `pub(crate)`, not
// private: the sibling `render`/`documents` services call
// `service::merge_into_payload`, same cross-module rule as everywhere else.
mod model;
mod repository;
pub mod routes;
pub(crate) mod service;
