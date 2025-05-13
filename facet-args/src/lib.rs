#![warn(missing_docs)]
#![warn(clippy::std_instead_of_core)]
#![warn(clippy::std_instead_of_alloc)]
#![forbid(unsafe_code)]
#![doc = include_str!("../README.md")]

extern crate alloc;

/// CLI argument format implementation for facet-deserialize
pub mod format;

#[allow(unused)]
pub use format::from_slice;
