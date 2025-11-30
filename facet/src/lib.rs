#![cfg_attr(not(feature = "std"), no_std)]
#![warn(missing_docs)]
#![warn(clippy::std_instead_of_core)]
#![warn(clippy::std_instead_of_alloc)]
#![doc = include_str!("../README.md")]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![cfg_attr(docsrs, feature(builtin_syntax))]
#![cfg_attr(docsrs, feature(prelude_import))]
#![cfg_attr(docsrs, allow(internal_features))]

pub use facet_core::*;

#[doc = include_str!("derive_facet.md")]
pub use facet_macros::*;

#[cfg(feature = "reflect")]
pub use facet_reflect::*;

pub mod hacking;

pub use static_assertions;

/// Define an attribute grammar with type-safe parsing.
///
/// This macro generates:
/// - The attribute types (enum + structs)
/// - A `__parse_attr!` macro for parsing attribute tokens
/// - Re-exports for the necessary proc-macros
///
/// # Example
///
/// ```ignore
/// facet::define_attr_grammar! {
///     pub enum Attr {
///         /// Skip this field entirely
///         Skip,
///         /// Rename to a different name
///         Rename(&'static str),
///         /// Database column configuration
///         Column(Column),
///     }
///
///     pub struct Column {
///         /// Override the database column name
///         pub name: Option<&'static str>,
///         /// Mark as primary key
///         pub primary_key: bool,
///     }
/// }
/// ```
///
/// This generates an `Attr` enum and `Column` struct with the specified fields,
/// along with a `__parse_attr!` macro that can parse attribute syntax like:
///
/// - `skip` → `Attr::Skip`
/// - `rename("users")` → `Attr::Rename("users")`
/// - `column(name = "user_id", primary_key)` → `Attr::Column(Column { name: Some("user_id"), primary_key: true })`
///
/// # Supported Field Types
///
/// | Grammar Type | Rust Type | Syntax |
/// |--------------|-----------|--------|
/// | `bool` | `bool` | `flag` or `flag = true` |
/// | `&'static str` | `&'static str` | `name = "value"` |
/// | `Option<&'static str>` | `Option<&'static str>` | `name = "value"` (optional) |
/// | `Option<bool>` | `Option<bool>` | `flag = true` (optional) |
#[macro_export]
macro_rules! define_attr_grammar {
    ($($grammar:tt)*) => {
        $crate::__make_parse_attr! { $($grammar)* }
    };
}
