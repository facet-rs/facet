#![doc = include_str!("../README.md")]
//! Core formatting and parsing utilities for Styx.
//!
//! This crate provides the low-level building blocks for Styx serialization
//! and deserialization, independent of any specific framework (facet, serde, etc.).

mod cst_format;
mod options;
mod scalar;
mod value_format;
mod writer;

pub use cst_format::{format_cst, format_source};
pub use options::FormatOptions;
pub use scalar::{can_be_bare, count_escapes, count_newlines, escape_quoted, unescape_quoted};
pub use value_format::{format_object_braced, format_value, format_value_default};
pub use writer::StyxWriter;
