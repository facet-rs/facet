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

// Define KDL extension attributes for use with #[facet(kdl::attr)] syntax.
//
// After importing `use facet_kdl as kdl;`, users can write:
//   #[facet(kdl::child)]
//   #[facet(kdl::children)]
//   #[facet(kdl::property)]
//   #[facet(kdl::argument)]
//   #[facet(kdl::arguments)]
//   #[facet(kdl::node_name)]
facet_core::define_extension_attrs! {
    "KDL";
    child,
    children,
    property,
    argument,
    arguments,
    node_name,
}
