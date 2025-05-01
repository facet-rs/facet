#![cfg_attr(not(feature = "std"), no_std)]
#![warn(missing_docs)]
#![warn(clippy::std_instead_of_core)]
#![warn(clippy::std_instead_of_alloc)]
#![doc = include_str!("../README.md")]

extern crate alloc;

mod error;
pub use error::*;

#[cfg(feature = "alloc")]
mod wip;
#[cfg(feature = "alloc")]
pub use wip::*;

mod peek;
pub use peek::*;

mod scalar;
pub use scalar::*;

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
