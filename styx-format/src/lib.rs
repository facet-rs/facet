//! Core formatting and parsing utilities for Styx.
//!
//! This crate provides the low-level building blocks for Styx serialization
//! and deserialization, independent of any specific framework (facet, serde, etc.).

mod options;
mod scalar;
mod writer;

pub use options::FormatOptions;
pub use scalar::{can_be_bare, count_escapes, count_newlines, escape_quoted, unescape_quoted};
pub use writer::StyxWriter;
