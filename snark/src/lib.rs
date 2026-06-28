#![forbid(unsafe_code)]
#![warn(missing_docs)]
//! Tree-sitter-compatible grammar package and parser runtime foundations.
//!
//! Snark keeps the Tree-sitter compatibility boundary separate from the
//! validated grammar and runtime layers. The current crate can import and
//! preserve Tree-sitter package artifacts; lowering into Snark's resolved
//! grammar IR is the next layer.

pub mod corpus;
pub mod diagnostic;
pub mod grammar;
pub mod manifest;
pub mod node_types;
pub mod parse;
pub mod query;
pub mod runtime_input;
pub mod scanner;
pub mod source;
#[cfg(feature = "tree-sitter-import")]
pub mod tree_sitter;
