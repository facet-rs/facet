//! Declarative attribute grammar system.
//!
//! This crate provides `define_attr_grammar!` for defining type-safe,
//! self-documenting attribute grammars that can be used across crates.
//!
//! # Example
//!
//! ```ignore
//! proto_attr::define_attr_grammar! {
//!     pub enum Attr {
//!         /// Skip this field
//!         Skip,
//!         /// Rename to a different name
//!         Rename(&'static str),
//!     }
//! }
//! ```

pub use proto_attr_core::*;
pub use proto_attr_macros::{__attr_error, __field_error, __make_parse_attr};

/// Define an attribute grammar with type-safe parsing.
///
/// This macro generates:
/// - The attribute types (enums and structs)
/// - A `__parse_attr!` macro for parsing attribute tokens
///
/// # Example
///
/// ```ignore
/// proto_attr::define_attr_grammar! {
///     pub enum Attr {
///         Skip,
///         Rename(&'static str),
///         Column(Column),
///     }
///
///     pub struct Column {
///         pub name: Option<&'static str>,
///         pub primary_key: bool,
///     }
/// }
/// ```
#[macro_export]
macro_rules! define_attr_grammar {
    ($($grammar:tt)*) => {
        $crate::__make_parse_attr! { $($grammar)* }
    };
}
