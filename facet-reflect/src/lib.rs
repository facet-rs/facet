#![cfg_attr(not(feature = "std"), no_std)]
#![warn(missing_docs)]
#![warn(clippy::std_instead_of_core)]
#![warn(clippy::std_instead_of_alloc)]
#![doc = include_str!("../README.md")]

#[cfg(feature = "alloc")]
extern crate alloc;

mod error;
pub use error::*;

#[cfg(feature = "alloc")]
mod poke;
#[cfg(feature = "alloc")]
pub use poke::*;

// mod peek;
// pub use peek::*;

mod scalar;
pub use scalar::*;
