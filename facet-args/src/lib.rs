#![warn(missing_docs)]
#![warn(clippy::std_instead_of_core)]
#![warn(clippy::std_instead_of_alloc)]
#![forbid(unsafe_code)]
#![doc = include_str!("../README.md")]

extern crate alloc;

mod format;

pub(crate) mod arg;
pub(crate) mod error;
pub(crate) mod span;

pub use format::from_slice;
pub use format::from_std_args;
