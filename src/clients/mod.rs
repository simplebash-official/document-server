// External resources this service connects to at startup and shares
// read-only for the rest of the process's life: the SQLite connection pool
// (`sqlite`), and the warmed-up Typst compiler (`render`). `AppState` holds
// both so modules never reconnect or rebuild them per request.
//
// `http` is the exception — a stateless, per-call outbound HTTP fetch used
// only to download a render payload's `logoUrl`, since Typst cannot fetch a
// URL during compilation.
pub mod http;
pub mod render;
pub mod sqlite;
