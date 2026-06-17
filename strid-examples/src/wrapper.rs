//! An example of constructing a basic strongly-typed wrapper around
//! a string value.
//!
//! The types in this module do not perform any validation or normalization
//! of their values, so every valid UTF-8 string is potentially valid for
//! these types.

use strid::braid;

/// A basic example of a wrapper around a [`String`]
///
/// This type ends in _Buf_, so the borrowed form of this type
/// will be named [`Username`].
#[braid(
    serde,
    ref_doc = "A borrowed reference to a basic string slice wrapper"
)]
pub struct UsernameBuf(String);
