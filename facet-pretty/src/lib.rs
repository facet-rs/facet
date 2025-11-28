#![warn(missing_docs)]
#![warn(clippy::std_instead_of_core)]
#![warn(clippy::std_instead_of_alloc)]
#![doc = include_str!("../README.md")]

extern crate alloc;

mod color;
mod display;
mod printer;
mod shape;

pub use color::*;
pub use display::*;
pub use printer::*;
pub use shape::*;
