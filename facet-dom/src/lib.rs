//! Tree-based (DOM) deserializer for facet.
//!
//! This crate provides a deserializer designed for tree-structured documents
//! like HTML and XML, where:
//! - Nodes have a tag name
//! - Nodes can have attributes (key-value pairs)
//! - Nodes can have children (mixed content: text and child elements interleaved)
//!
//! This is fundamentally different from facet-format's event model, which is
//! designed for flat key-value formats like JSON and TOML.

#![deny(missing_docs, rustdoc::broken_intra_doc_links)]

mod deserializer;
mod event;
mod parser;

pub use deserializer::*;
pub use event::*;
pub use parser::*;
