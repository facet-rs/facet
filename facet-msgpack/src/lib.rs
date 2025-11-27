#![warn(missing_docs)]
#![forbid(unsafe_code)]
#![doc = include_str!("../README.md")]

mod errors;
pub use errors::Error as DecodeError;

mod constants;
pub use constants::*;

mod deserialize;
pub use deserialize::*;

mod serialize;
pub use serialize::*;
