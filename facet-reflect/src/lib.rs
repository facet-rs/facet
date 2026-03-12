#![cfg_attr(not(feature = "std"), no_std)]
#![warn(missing_docs)]
#![warn(clippy::std_instead_of_core)]
#![warn(clippy::std_instead_of_alloc)]
//!
//! [![Coverage Status](https://coveralls.io/repos/github/facet-rs/facet-reflect/badge.svg?branch=main)](https://coveralls.io/github/facet-rs/facet?branch=main)
//! [![crates.io](https://img.shields.io/crates/v/facet-reflect.svg)](https://crates.io/crates/facet-reflect)
//! [![documentation](https://docs.rs/facet-reflect/badge.svg)](https://docs.rs/facet-reflect)
//! [![MIT/Apache-2.0 licensed](https://img.shields.io/crates/l/facet-reflect.svg)](./LICENSE)
//! [![Discord](https://img.shields.io/discord/1379550208551026748?logo=discord&label=discord)](https://discord.gg/JhD7CwCJ8F)
//!
//!
//! Whereas the core `facet` crate provides essential traits like `Facet` itself, and
//! data structures like `Type`, `StructType`, `Field`, etc., `facet-reflect` uses that
//! information about the shape of types to allow:
//!
//!   * Read-only access to already-initialized values (via [`Peek`])
//!   * Construction of values from scratch (via [`Partial`])
//!
//! This allows, respectively, serialization and deserialization, without risking breaking
//! invariants in types that implement `Facet`.
//!
#![doc = include_str!("../readme-footer.md")]

extern crate alloc;

#[cfg(doc)]
pub mod deferred_materialization;

mod error;
pub use error::*;

#[cfg(feature = "alloc")]
mod partial;
#[cfg(feature = "alloc")]
pub use partial::*;

#[cfg(feature = "alloc")]
mod resolution;
#[cfg(feature = "alloc")]
pub use resolution::*;

mod peek;
pub use peek::*;

mod poke;
pub use poke::*;

mod scalar;
pub use scalar::*;

mod spanned;
pub use spanned::{Span, get_metadata_container_value_shape};

#[cfg(feature = "tracing")]
#[allow(unused_imports)]
pub(crate) use tracing::{debug, trace};

#[cfg(not(feature = "tracing"))]
#[macro_export]
/// Forwards to tracing::trace when the tracing feature is enabled
macro_rules! trace {
    ($($tt:tt)*) => {};
}
#[cfg(not(feature = "tracing"))]
#[macro_export]
/// Forwards to tracing::debug when the tracing feature is enabled
macro_rules! debug {
    ($($tt:tt)*) => {};
}
