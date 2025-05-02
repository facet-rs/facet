#![cfg_attr(not(feature = "std"), no_std)]
#![warn(missing_docs)]
#![warn(clippy::std_instead_of_core)]
#![warn(clippy::std_instead_of_alloc)]
#![deny(unsafe_code)]
#![doc = include_str!("../README.md")]

extern crate alloc;

#[cfg(not(feature = "alloc"))]
compile_error!("feature `alloc` is required");

mod deserialize;
pub use deserialize::*;

#[cfg(feature = "std")]
mod json_serializer;

#[cfg(feature = "std")]
mod serialize;
#[cfg(feature = "std")]
pub use serialize::*;

#[cfg(feature = "std")]
pub use json_serializer::JsonSerializer;
