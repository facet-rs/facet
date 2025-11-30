#![warn(missing_docs)]
#![allow(clippy::result_large_err)]
#![doc = include_str!("../README.md")]

mod deserialize;
mod error;
mod serialize;

// Re-export span types from facet-reflect
pub use facet_reflect::{Span, Spanned};

// Re-export error types
pub use error::{KdlError, KdlErrorKind};

// Re-export deserialization
pub use deserialize::from_str;

// Re-export serialization
pub use serialize::{to_string, to_writer};

// KDL extension attributes for use with #[facet(kdl::attr)] syntax.
//
// After importing `use facet_kdl as kdl;`, users can write:
//   #[facet(kdl::child)]
//   #[facet(kdl::children)]
//   #[facet(kdl::property)]
//   #[facet(kdl::argument)]
//   #[facet(kdl::arguments)]
//   #[facet(kdl::node_name)]

// Generate KDL attribute grammar using the grammar DSL.
// This generates:
// - `Attr` enum with all KDL attribute variants
// - `__attr!` macro that dispatches to attribute handlers and returns ExtensionAttr
// - `__parse_attr!` macro for parsing (internal use)
facet::define_attr_grammar! {
    ns "kdl";
    crate_path ::facet_kdl;

    /// KDL attribute types for field and container configuration.
    pub enum Attr {
        /// Marks a field as a single KDL child node
        Child,
        /// Marks a field as collecting multiple KDL children
        Children,
        /// Marks a field as a KDL property (key=value)
        Property,
        /// Marks a field as a single KDL positional argument
        Argument,
        /// Marks a field as collecting all KDL positional arguments
        Arguments,
        /// Marks a field as storing the KDL node name
        NodeName,
    }
}
