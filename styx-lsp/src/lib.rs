//! Styx Language Server
//!
//! LSP server for the Styx configuration language, providing:
//! - Semantic highlighting (schema-aware)
//! - Diagnostics (parse errors, validation errors)
//! - Completions (keys, values, tags from schema)
//! - Hover information (type info from schema)

mod schema_validation;
mod semantic_tokens;
mod server;

pub use server::run;
