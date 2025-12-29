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
#[cfg(feature = "miette")]
pub use miette::SourceSpan;
pub use spanned::{
    Span, Spanned, find_span_metadata_field, get_spanned_inner_shape, is_spanned_shape,
};

#[cfg(feature = "log")]
#[allow(unused_imports)]
pub(crate) use log::{debug, trace};

#[cfg(not(feature = "log"))]
#[macro_export]
/// Forwards to log::trace when the log feature is enabled
macro_rules! trace {
    ($($tt:tt)*) => {};
}
#[cfg(not(feature = "log"))]
#[macro_export]
/// Forwards to log::debug when the log feature is enabled
macro_rules! debug {
    ($($tt:tt)*) => {};
}
