#![cfg_attr(not(feature = "std"), no_std)]
#![warn(missing_docs)]
#![doc = include_str!("../README.md")]

pub use facet_core::*;
pub use facet_derive::*;

pub mod hacking;
