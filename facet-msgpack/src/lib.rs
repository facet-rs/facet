#![warn(missing_docs)]
#![forbid(unsafe_code)]
#![doc = include_str!("../README.md")]

mod errors;
pub use errors::Error as DecodeError;

mod constants;
pub use constants::*;

mod deserialize;
pub use deserialize::*;

#[cfg(feature = "serialize")]
mod serialize;
#[cfg(feature = "serialize")]
pub use serialize::*;
