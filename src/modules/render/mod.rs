// The render endpoint itself: resolves a template, compiles it against the
// request body via `clients::render::RenderEngine`, and records a
// `documents` entry. `repository`/`service` are private — `routes` is the
// only public entry point.
mod repository;
pub mod routes;
mod service;
