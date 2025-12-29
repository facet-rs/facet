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
pub use deserialize::{from_str, from_str_owned};

// Re-export serialization
pub use serialize::{to_string, to_writer};

mod kdl_wrapper;
pub use kdl_wrapper::Kdl;

#[cfg(feature = "axum")]
mod axum;
#[cfg(feature = "axum")]
pub use self::axum::KdlRejection;

// KDL extension attributes for use with #[facet(kdl::attr)] syntax.
//
// After importing `use facet_kdl_legacy as kdl;`, users can write:
//   #[facet(kdl::child)]
//   #[facet(kdl::children)]
//   #[facet(kdl::children = "custom_name")]
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
    crate_path ::facet_kdl_legacy;

    /// KDL attribute types for field and container configuration.
    pub enum Attr {
        /// Marks a field as a single KDL child node.
        ///
        /// Can optionally specify a custom node name to match:
        /// - `#[facet(kdl::child)]` - matches by field name
        /// - `#[facet(kdl::child = "custom")]` - matches nodes named "custom"
        Child(Option<&'static str>),
        /// Marks a field as collecting multiple KDL children into a Vec, HashMap, or Set.
        ///
        /// When a struct has a single `#[facet(kdl::children)]` field, all child nodes
        /// are collected into that field (catch-all behavior).
        ///
        /// When a struct has multiple `#[facet(kdl::children)]` fields, nodes are routed
        /// based on matching the node name to the singular form of the field name:
        /// - `dependency` nodes → `dependencies` field
        /// - `sample` nodes → `samples` field
        /// - `item` nodes → `items` field
        ///
        /// Supported pluralization patterns:
        /// - Simple `s`: `item` → `items`
        /// - `ies` ending: `dependency` → `dependencies`
        /// - `es` ending: `box` → `boxes`
        ///
        /// To override automatic singularization, specify a custom node name:
        /// - `#[facet(kdl::children = "kiddo")]` matches nodes named `kiddo`
        Children(Option<&'static str>),
        /// Marks a field as a KDL property (key=value)
        Property,
        /// Marks a field as a single KDL positional argument
        Argument,
        /// Marks a field as collecting all KDL positional arguments
        Arguments,
        /// Marks a field as storing the KDL node name during deserialization.
        /// Use this to capture the name of the current node into a field.
        ///
        /// Example:
        /// ```ignore
        /// #[derive(Facet)]
        /// struct Node {
        ///     #[facet(kdl::node_name)]
        ///     name: String,
        /// }
        /// ```
        NodeName,
    }
}
