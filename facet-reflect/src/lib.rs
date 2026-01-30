#![cfg_attr(not(feature = "std"), no_std)]
#![warn(missing_docs)]
#![warn(clippy::std_instead_of_core)]
#![warn(clippy::std_instead_of_alloc)]
#![doc = include_str!("../README.md")]

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
mod api2;
#[cfg(feature = "fuzz-all-types")]
mod api2_fuzz;

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
