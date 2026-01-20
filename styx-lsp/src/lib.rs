//! Styx Language Server
//!
//! LSP server for the Styx configuration language, providing:
//! - Semantic highlighting (schema-aware)
//! - Diagnostics (parse errors, validation errors)
//! - Completions (keys, values, tags from schema)
//! - Hover information (type info from schema)
//! - Schema suggestions for known file patterns

pub mod cache;
pub mod schema_hints;
pub mod semantic_tokens;
mod schema_validation;
mod server;

pub use server::run;
pub use semantic_tokens::{compute_highlight_spans, HighlightSpan, TokenType};
