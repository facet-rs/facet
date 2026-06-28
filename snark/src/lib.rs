#![forbid(unsafe_code)]
#![warn(missing_docs)]
//! Tree-sitter-compatible grammar package and Weavy lowering foundations.
//!
//! Snark keeps the Tree-sitter compatibility boundary separate from the
//! validated grammar and runtime layers. The current crate can import and
//! preserve Tree-sitter package artifacts. The runtime direction is a
//! provenance-rich lowering pipeline from Tree-sitter artifacts into Snark's
//! Weavy dialect, not direct execution of raw `grammar.json` DTOs.

pub mod corpus;
pub mod diagnostic;
pub mod grammar;
pub mod lower;
pub mod manifest;
pub mod milestone;
pub mod node_types;
pub mod parser_c;
pub mod query;
pub mod runtime_input;
pub mod scanner;
pub mod source;
#[cfg(feature = "tree-sitter-import")]
pub mod tree_sitter;
