#![forbid(unsafe_code)]
#![warn(missing_docs)]
//! Tree-sitter-compatible grammar package and Weavy lowering foundations.
//!
//! Snark keeps the Tree-sitter compatibility boundary separate from the
//! validated grammar and runtime layers. The current crate can import and
//! preserve Tree-sitter grammar, scanner, query, and fixture inputs. The
//! runtime direction is a provenance-rich lowering pipeline from validated
//! Snark grammar IR into Snark's Weavy dialect, checked against Tree-sitter's
//! corpus and query test outputs.

pub mod corpus;
pub mod diagnostic;
pub mod grammar;
mod lex_match;
pub mod lexical;
pub mod lower;
pub mod manifest;
pub mod parser;
pub mod query;
pub mod runtime_input;
pub mod scanner;
pub mod source;
#[cfg(feature = "tree-sitter-import")]
pub mod tree_sitter;
pub mod validated;
