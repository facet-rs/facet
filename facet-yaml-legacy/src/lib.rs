//! Legacy YAML serialization and deserialization for Facet types.
//!
//! **Note**: This crate is deprecated. Please use `facet-yaml` instead, which provides
//! the modern format-based implementation.
//!
//! This crate provides YAML parsing using a streaming event-based parser (saphyr-parser)
//! and integrates with the facet reflection system for type-safe deserialization.
//!
//! # Example
//!
//! ```
//! use facet::Facet;
//! use facet_yaml_legacy::from_str;
//!
//! #[derive(Facet, Debug, PartialEq)]
//! struct Config {
//!     name: String,
//!     port: u16,
//! }
//!
//! let yaml = "name: myapp\nport: 8080";
//! let config: Config = from_str(yaml).unwrap();
//! assert_eq!(config.name, "myapp");
//! assert_eq!(config.port, 8080);
//! ```

#![cfg_attr(not(feature = "std"), no_std)]
#![warn(missing_docs)]
#![warn(clippy::std_instead_of_core)]
#![warn(clippy::std_instead_of_alloc)]

extern crate alloc;

mod deserialize;
mod error;
#[cfg(feature = "std")]
mod serialize;

pub use deserialize::{from_str, from_str_borrowed};
pub use error::{YamlError, YamlErrorKind};
#[cfg(feature = "std")]
pub use serialize::{to_string, to_writer};

mod yaml;
pub use yaml::Yaml;

#[cfg(feature = "axum")]
mod axum;
#[cfg(feature = "axum")]
pub use self::axum::YamlRejection;

// Re-export span types from facet-reflect
pub use facet_reflect::{Span, Spanned};

/// Serde-compatible extension attributes for YAML serialization.
///
/// This module provides extension attributes that mirror serde's attribute syntax,
/// allowing users to write `#[facet(serde::rename = "name")]` for field renaming.
///
/// Users import `use facet_yaml_legacy::serde;` to use these attributes.
pub mod serde {
    // Generate serde attribute grammar using the grammar DSL.
    // This generates:
    // - `Attr` enum with all serde attribute variants
    // - `__attr!` macro that dispatches to attribute handlers and returns ExtensionAttr
    // - `__parse_attr!` macro for parsing (internal use)
    facet::define_attr_grammar! {
        ns "serde";
        crate_path ::facet_yaml_legacy::serde;

        /// Serde-compatible attribute types for field and container configuration.
        pub enum Attr {
            /// Rename a field during serialization/deserialization.
            ///
            /// Usage: `#[facet(serde::rename = "new_name")]`
            Rename(&'static str),
        }
    }
}
