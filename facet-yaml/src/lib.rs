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
    pub use crate::__serde_attr as __attr;
    pub use crate::__serde_rename as __rename;
}

/// Dispatcher macro for serde extension attributes.
#[macro_export]
#[doc(hidden)]
macro_rules! __serde_attr {
    (rename { $($tt:tt)* }) => { $crate::__serde_rename!{ $($tt)* } };

    // Unknown attribute: use __attr_error! for typo suggestions
    ($unknown:ident $($tt:tt)*) => {
        ::facet::__attr_error!(
            @known_attrs { rename }
            @got_name { $unknown }
            @got_rest { $($tt)* }
        )
    };
}

/// The rename attribute for serde compatibility.
///
/// Usage: `#[facet(serde::rename = "name")]`
#[macro_export]
#[doc(hidden)]
macro_rules! __serde_rename {
    // Field with rename value: #[facet(serde::rename = "new_name")]
    { $field:ident : $ty:ty | = $name:literal } => {{
        static __VAL: ::core::option::Option<&'static str> = ::core::option::Option::Some($name);
        ::facet::ExtensionAttr::new("serde", "rename", &__VAL)
    }};
    // Field without rename value (shouldn't happen, but handle)
    { $field:ident : $ty:ty } => {{
        static __VAL: ::core::option::Option<&'static str> = ::core::option::Option::None;
        ::facet::ExtensionAttr::new("serde", "rename", &__VAL)
    }};
    // Container level
    { } => {{
        static __VAL: ::core::option::Option<&'static str> = ::core::option::Option::None;
        ::facet::ExtensionAttr::new("serde", "rename", &__VAL)
    }};
    { | = $name:literal } => {{
        static __VAL: ::core::option::Option<&'static str> = ::core::option::Option::Some($name);
        ::facet::ExtensionAttr::new("serde", "rename", &__VAL)
    }};
    // Invalid syntax
    { $($tt:tt)* } => {{
        ::core::compile_error!("serde::rename expects `= \"name\"` syntax, e.g., #[facet(serde::rename = \"new_name\")]")
    }};
}
