//! YAML serialization and deserialization for Facet types.
//!
//! This crate provides YAML parsing using a streaming event-based parser (saphyr-parser)
//! and integrates with the facet reflection system for type-safe deserialization.
//!
//! # Example
//!
//! ```
//! use facet::Facet;
//! use facet_yaml::from_str;
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

pub use deserialize::from_str;
pub use error::{YamlError, YamlErrorKind};
#[cfg(feature = "std")]
pub use serialize::{to_string, to_writer};

// Re-export span types from facet-reflect
pub use facet_reflect::{Span, Spanned};
