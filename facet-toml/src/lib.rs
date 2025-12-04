#![cfg_attr(not(feature = "std"), no_std)]
#![warn(missing_docs)]
#![warn(clippy::std_instead_of_core)]
#![warn(clippy::std_instead_of_alloc)]
#![forbid(unsafe_code)]
#![doc = include_str!("../README.md")]

extern crate alloc;

#[doc = include_str!("../docs/design.md")]
pub mod design {}

#[cfg(feature = "deserialize")]
mod deserialize;
#[cfg(feature = "deserialize")]
pub use deserialize::*;

#[cfg(feature = "serialize")]
mod serialize;
#[cfg(feature = "serialize")]
pub use serialize::*;

mod toml_wrapper;
pub use toml_wrapper::Toml;

#[cfg(feature = "axum")]
mod axum;
#[cfg(feature = "axum")]
pub use self::axum::TomlRejection;

// Re-export span types from facet-reflect
pub use facet_reflect::{Span, Spanned};
