//! Tree-based (DOM) serialization and deserialization for facet.
//!
//! This crate provides serializers and deserializers designed for tree-structured
//! documents like HTML and XML, where:
//! - Nodes have a tag name
//! - Nodes can have attributes (key-value pairs)
//! - Nodes can have children (mixed content: text and child elements interleaved)

#![deny(missing_docs, rustdoc::broken_intra_doc_links)]

mod deserializer;
mod error;
mod event;
pub mod naming;
mod parser;
mod parser_ext;
mod serializer;
mod tracing_macros;

pub use deserializer::*;
pub use error::*;
pub use event::*;
pub use parser::*;
pub use parser_ext::*;
pub use serializer::*;
