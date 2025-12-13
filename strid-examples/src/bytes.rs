//! An example of constructing a basic strongly-typed wrapper around
//! a [`Bytes`]-backed value.
//!
//! The types in this module do not perform any validation or normalization
//! of their values, so every valid UTF-8 string is potentially valid for
//! these types.
//!
//! Note: This example requires facet support for `bytestring::ByteString`.
//! See: https://github.com/facet-rs/facet/issues/1284
//!
//! [`Bytes`]: https://docs.rs/bytes/*/bytes/struct.Bytes.html

#![allow(dead_code)]

#[cfg(feature = "bytestring-facet")]
use bytestring::ByteString;
#[cfg(feature = "bytestring-facet")]
use strid::braid;

/// A basic example of a wrapper around a [`Bytes`]
///
/// This type ends in _Buf_, so the borrowed form of this type
/// will be named [`Username`].
///
/// [`Bytes`]: https://docs.rs/bytes/*/bytes/struct.Bytes.html
#[cfg(feature = "bytestring-facet")]
#[braid(
    serde,
    ref_doc = "A borrowed reference to a basic string slice wrapper"
)]
pub struct UsernameBuf(ByteString);
