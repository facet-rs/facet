#![cfg_attr(not(feature = "std"), no_std)]
#![warn(missing_docs)]
#![warn(clippy::std_instead_of_core)]
#![warn(clippy::std_instead_of_alloc)]
#![forbid(unsafe_code)]
#![doc = include_str!("../README.md")]
extern crate alloc;

#[cfg(feature = "std")]
mod serialize;
#[cfg(feature = "std")]
pub use serialize::*;

// mod deserialize;
// pub use deserialize::*;
