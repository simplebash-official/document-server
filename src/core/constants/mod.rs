// Central source of truth for string constants reused across modules, so a
// module tag, id prefix, or error code is never hand-typed (and potentially
// mis-typed) in more than one place.
pub mod codes;
pub mod modules;
pub mod prefixes;
