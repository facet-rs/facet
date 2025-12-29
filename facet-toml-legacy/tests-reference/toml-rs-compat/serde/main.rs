//! Compatibility layer for toml-rs serde tests.
//!
//! This module provides API-compatible wrappers around facet_toml
//! to allow running tests ported from toml-rs.

#![recursion_limit = "256"]

macro_rules! t {
    ($e:expr) => {
        match $e {
            Ok(t) => t,
            Err(e) => panic!("{} failed with {}", stringify!($e), e),
        }
    };
}

mod de_enum;
mod de_errors;
mod de_key;
mod general;
mod ser_enum;
mod ser_key;
mod ser_tables_last;
mod ser_to_string;
mod ser_to_string_pretty;
mod spanned;

// Re-export facet_toml functions with toml-compatible names
pub use facet_toml_legacy::from_str;
pub use facet_toml_legacy::{to_string, to_string_pretty};

// Dynamic value type - facet_toml doesn't have a direct equivalent
// We'll use toml_edit::Value for now as a stand-in
pub type SerdeValue = toml_edit::Value;
pub type SerdeTable = toml_edit::Table;
pub type SerdeDocument = toml_edit::Table;

// Date/time types - placeholder for now
// TODO: These need proper implementation
pub struct Date;
pub struct Time;
pub struct Datetime;

// Spanned type - use facet_reflect's Spanned
pub use facet_reflect::Spanned;
