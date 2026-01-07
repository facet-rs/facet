//! KDL serialization and deserialization using facet-format.
//!
//! This crate provides KDL (KDL Document Language) support using the
//! `FormatParser` and `FormatSerializer` traits from `facet-format`.
//!
//! # KDL Data Model
//!
//! **KDL is not XML.** Unlike XML which has exactly one root element, KDL documents
//! can have **multiple root nodes**. This is a fundamental difference that affects
//! how you design your Rust types.
//!
//! ## The Transparent Document Model
//!
//! When you serialize or deserialize with facet-kdl, the outermost Rust struct is
//! the **document struct**. It's transparent - it doesn't appear in the KDL output.
//! Instead, its fields become root-level nodes.
//!
//! ```ignore
//! use facet::Facet;
//!
//! #[derive(Facet)]
//! struct Config {
//!     host: String,
//!     port: u16,
//! }
//!
//! let cfg = Config { host: "localhost".into(), port: 8080 };
//! let kdl = facet_kdl::to_string(&cfg).unwrap();
//! // Produces:
//! // host "localhost"
//! // port 8080
//! ```
//!
//! Each field becomes its own root node. The struct name `Config` doesn't appear
//! anywhere in the output.
//!
//! ## Why Fields Default to Child Nodes
//!
//! At the document level, there's no node to attach properties to. You can't write:
//! ```kdl
//! host="localhost" port=8080  // Invalid! Properties need a node name
//! ```
//!
//! So fields without explicit `kdl::*` attributes default to being child nodes.
//! If you want properties, you need a wrapper node.
//!
//! ## KDL Node Structure
//!
//! Each KDL node has:
//! - A **name** (identifier)
//! - **Arguments** (positional values after the name)
//! - **Properties** (key=value pairs)
//! - **Children** (nested nodes inside braces)
//!
//! # Mapping to Rust Types
//!
//! KDL nodes map to Rust structs using the `kdl::*` attributes:
//!
//! - `#[facet(kdl::argument)]` - Field receives a single positional argument
//! - `#[facet(kdl::arguments)]` - Field receives all positional arguments as Vec
//! - `#[facet(kdl::property)]` - Field receives a property value
//! - `#[facet(kdl::child)]` - Field receives a single child node
//! - `#[facet(kdl::children)]` - Field receives multiple child nodes as Vec
//! - `#[facet(kdl::node_name)]` - Field receives the node's name (for dynamic dispatch)
//!
//! # Examples
//!
//! ## Simple Roundtrip
//!
//! ```ignore
//! use facet::Facet;
//!
//! #[derive(Facet, Debug, PartialEq)]
//! struct Config {
//!     host: String,
//!     port: u16,
//! }
//!
//! let cfg = Config { host: "https://example.com".into(), port: 443 };
//!
//! // Serialize to KDL
//! let kdl = facet_kdl::to_string(&cfg).unwrap();
//! assert_eq!(kdl, "host \"https://example.com\"\nport 443");
//!
//! // Deserialize back
//! let parsed: Config = facet_kdl::from_str(&kdl).unwrap();
//! assert_eq!(cfg, parsed);
//! ```
//!
//! ## Multiple Root Nodes
//!
//! ```ignore
//! use facet::Facet;
//!
//! #[derive(Facet, Debug)]
//! struct RoutesConfig {
//!     #[facet(kdl::children)]
//!     routes: Vec<Route>,
//! }
//!
//! #[derive(Facet, Debug)]
//! struct Route {
//!     #[facet(kdl::argument)]
//!     path: String,
//!     #[facet(kdl::property)]
//!     handler: String,
//! }
//!
//! let kdl = r#"
//! route "/api/users" handler="users_handler"
//! route "/api/posts" handler="posts_handler"
//! "#;
//!
//! let config: RoutesConfig = facet_kdl::from_str(kdl).unwrap();
//! assert_eq!(config.routes.len(), 2);
//! ```
//!
//! ## Nested Structure with Arguments and Properties
//!
//! ```ignore
//! use facet::Facet;
//!
//! #[derive(Facet, Debug)]
//! struct ServerConfig {
//!     #[facet(kdl::child)]
//!     server: Server,
//! }
//!
//! #[derive(Facet, Debug)]
//! struct Server {
//!     #[facet(kdl::argument)]
//!     host: String,
//!     #[facet(kdl::property)]
//!     port: u16,
//! }
//!
//! let kdl = r#"server "localhost" port=8080"#;
//! let config: ServerConfig = facet_kdl::from_str(kdl).unwrap();
//! ```

#![forbid(unsafe_code)]

extern crate alloc;

mod parser;
mod serializer;

#[cfg(feature = "axum")]
mod axum;

pub use parser::{KdlDeserializeError, KdlError, KdlParser, KdlProbe};

#[cfg(feature = "axum")]
pub use axum::{Kdl, KdlRejection};
pub use serializer::{KdlSerializeError, KdlSerializer, to_string, to_vec};

// Re-export DeserializeError for convenience
pub use facet_format::DeserializeError;

/// Deserialize a value from a KDL string into an owned type.
///
/// Returns rich error diagnostics with source context for display.
///
/// # Example
///
/// ```ignore
/// use facet::Facet;
/// use facet_kdl::from_str;
///
/// #[derive(Facet, Debug)]
/// struct Config {
///     #[facet(kdl::property)]
///     name: String,
/// }
///
/// let kdl = r#"config name="test""#;
/// let config: Config = from_str(kdl).unwrap();
/// ```
#[allow(clippy::result_large_err)] // Rich diagnostics require storing source context
pub fn from_str<T>(input: &str) -> Result<T, KdlDeserializeError>
where
    T: facet_core::Facet<'static>,
{
    use facet_format::FormatDeserializer;
    let parser = KdlParser::new(input);
    let mut de = FormatDeserializer::new_owned(parser);
    de.deserialize()
        .map_err(|inner| KdlDeserializeError::new(inner, input.to_string(), Some(T::SHAPE)))
}

/// Deserialize a value from a KDL string, allowing zero-copy borrowing.
///
/// This variant requires the input to outlive the result (`'input: 'facet`),
/// enabling zero-copy deserialization of string values.
pub fn from_str_borrowed<'input, 'facet, T>(
    input: &'input str,
) -> Result<T, DeserializeError<KdlError>>
where
    T: facet_core::Facet<'facet>,
    'input: 'facet,
{
    use facet_format::FormatDeserializer;
    let parser = KdlParser::new(input);
    let mut de = FormatDeserializer::new(parser);
    de.deserialize()
}

/// Deserialize a value from KDL bytes into an owned type.
///
/// This is the recommended default for most use cases. The input does not need
/// to outlive the result, making it suitable for deserializing from temporary
/// buffers (e.g., HTTP request bodies).
///
/// # Errors
///
/// Returns an error if the input is not valid UTF-8 or if deserialization fails.
///
/// # Example
///
/// ```ignore
/// use facet::Facet;
/// use facet_kdl::from_slice;
///
/// #[derive(Facet, Debug)]
/// struct Config {
///     #[facet(kdl::property)]
///     name: String,
/// }
///
/// let kdl = b"config name=\"test\"";
/// let config: Config = from_slice(kdl).unwrap();
/// ```
#[allow(clippy::result_large_err)]
pub fn from_slice<T>(input: &[u8]) -> Result<T, KdlDeserializeError>
where
    T: facet_core::Facet<'static>,
{
    let s = core::str::from_utf8(input).map_err(|e| {
        let inner = DeserializeError::Parser(KdlError::InvalidUtf8(e));
        KdlDeserializeError::new(inner, String::new(), Some(T::SHAPE))
    })?;
    from_str(s)
}

/// Deserialize a value from KDL bytes, allowing zero-copy borrowing.
///
/// This variant requires the input to outlive the result (`'input: 'facet`),
/// enabling zero-copy deserialization of string values.
///
/// # Errors
///
/// Returns an error if the input is not valid UTF-8 or if deserialization fails.
pub fn from_slice_borrowed<'input, 'facet, T>(
    input: &'input [u8],
) -> Result<T, DeserializeError<KdlError>>
where
    T: facet_core::Facet<'facet>,
    'input: 'facet,
{
    let s = core::str::from_utf8(input)
        .map_err(|e| DeserializeError::Parser(KdlError::InvalidUtf8(e)))?;
    from_str_borrowed(s)
}

// KDL attribute grammar for field and container configuration.
// This allows users to write #[facet(kdl::property)] etc.
facet::define_attr_grammar! {
    ns "kdl";
    crate_path ::facet_kdl;

    /// KDL attribute types for field and container configuration.
    pub enum Attr {
        /// Marks a field as a single KDL child node.
        ///
        /// The field name (or `rename`) determines which child node to match.
        /// Use `#[facet(rename = "custom")]` to match a different node name.
        Child,
        /// Marks a field as collecting multiple KDL children into a Vec, HashMap, or Set.
        ///
        /// When a struct has a single `#[facet(kdl::children)]` field, all child nodes
        /// are collected into that field (catch-all behavior).
        ///
        /// When a struct has multiple `#[facet(kdl::children)]` fields, nodes are routed
        /// based on matching the node name to the singular form of the field name.
        Children,
        /// Marks a field as a KDL property (key=value)
        Property,
        /// Marks a field as a single KDL positional argument
        Argument,
        /// Marks a field as collecting all KDL positional arguments
        Arguments,
        /// Marks a field as storing the KDL node name during deserialization.
        NodeName,
    }
}
