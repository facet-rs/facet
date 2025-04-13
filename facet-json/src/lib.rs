#![warn(missing_docs)]
#![warn(clippy::std_instead_of_core)]
#![warn(clippy::std_instead_of_alloc)]
#![doc = include_str!("../README.md")]

mod deserialize;
mod parser;
mod serialize;

pub use deserialize::*;
pub use serialize::*;
