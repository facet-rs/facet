#![warn(missing_docs)]
#![deny(unsafe_code)]
#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!("../README.md")]

#[cfg(feature = "alloc")]
extern crate alloc;

mod error;
pub use error::*;

mod serialize;
pub use serialize::*;

mod deserialize;
pub use deserialize::*;
