#![warn(missing_docs)]
#![doc = include_str!("../README.md")]

mod color;
mod display;
mod printer;

pub use color::*;
pub use display::*;
pub use printer::*;
