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

/// Serde-compatible extension attributes for YAML serialization.
///
/// This module provides extension attributes that mirror serde's attribute syntax,
/// allowing users to write `#[facet(serde::rename = "name")]` for field renaming.
///
/// Users import `use facet_yaml::serde;` to use these attributes.
pub mod serde {
    pub use crate::serde_attrs::*;
}

mod serde_attrs {
    use facet_core::{AnyStaticRef, LiteralKind, Token};

    // Marker struct for rename attribute
    #[doc(hidden)]
    #[allow(non_camel_case_types)]
    pub struct rename {
        _private: (),
    }

    /// The rename attribute function parses `= "name"` and returns the string.
    #[doc(hidden)]
    pub fn rename(args: &[Token]) -> AnyStaticRef {
        // Parse `= "name"` syntax and leak the result
        let result: Option<&'static str> = parse_rename_args(args);
        Box::leak(Box::new(result))
    }

    fn parse_rename_args(args: &[Token]) -> Option<&'static str> {
        let mut iter = args.iter();

        // Skip the '=' if present
        if let Some(Token::Punct { ch: '=', .. }) = iter.next() {
            // Look for a string literal
            if let Some(Token::Literal { text, kind, .. }) = iter.next() {
                if *kind == LiteralKind::String {
                    // Strip surrounding quotes: "value" -> value
                    return Some(text.trim_start_matches('"').trim_end_matches('"'));
                }
            }
        }
        None
    }

    // Validation machinery
    #[doc(hidden)]
    pub struct ValidAttr<A>(::core::marker::PhantomData<A>);

    #[doc(hidden)]
    #[diagnostic::on_unimplemented(
        message = "`{Self}` is not a recognized serde attribute",
        label = "unknown attribute",
        note = "valid attributes are: `rename`"
    )]
    pub trait IsValidAttr {}

    #[doc(hidden)]
    pub const fn __check_attr<A>()
    where
        ValidAttr<A>: IsValidAttr,
    {
    }

    #[doc(hidden)]
    impl IsValidAttr for ValidAttr<rename> {}
}
