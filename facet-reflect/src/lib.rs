//! The `facet-reflect` crate provides utilities for inspecting and manipulating data structures.
//!
//! This crate combines functionality that was previously split between separate crates:
//! - Peeking: Reading or inspecting data structures
//! - Poking: Modifying or manipulating data structures
//!
//! Both capabilities are essential for reflection operations in the Facet ecosystem.

#[cfg(feature = "alloc")]
mod poke;
#[cfg(feature = "alloc")]
pub use poke::*;

mod peek;
pub use peek::*;
