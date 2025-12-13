//! An example of constructing a basic ref-only strongly-typed wrapper around
//! a string slice.
//!
//! The types in this module do not perform any validation or normalization
//! of their values, so every valid UTF-8 string is potentially valid for
//! these types.

use strid::braid_ref;

/// A basic example of a wrapper around a [`str`]
#[braid_ref(serde, no_std)]
pub struct Element(str);
