#![forbid(unsafe_code)]
//! A small LSP 3.17 surface with Facet derives.
//!
//! This crate intentionally does not mirror the whole Language Server Protocol.
//! It contains only the types and stdio JSON-RPC framing needed by current Vix
//! language-server integration.

pub mod framing;
pub mod position;
pub mod semantic;
pub mod types;

pub use facet_json::RawJson;
