#![doc = include_str!("../README.md")]
//! Styx Language Server
//!
//! LSP server for the Styx configuration language, providing:
//! - Semantic highlighting (schema-aware)
//! - Diagnostics (parse errors, validation errors)
//! - Completions (keys, values, tags from schema)
//! - Hover information (type info from schema)
//! - Schema suggestions for known file patterns

pub mod cache;
pub mod config;
pub mod extensions;
pub mod schema_hints;
mod schema_validation;
pub mod semantic_tokens;
mod server;
pub mod testing;

pub use semantic_tokens::{HighlightSpan, TokenType, compute_highlight_spans};
pub use server::{DocumentMap, DocumentState, StyxLanguageServer, run};
