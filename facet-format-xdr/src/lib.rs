//! XDR (External Data Representation) format support via facet-format.
//!
//! **Status:** Placeholder implementation.
//!
//! XDR is a binary format defined in RFC 4506 for encoding structured data.
//! It is primarily used in Sun RPC (ONC RPC) protocols.
//!
//! Key characteristics:
//! - Big-endian byte order
//! - Fixed-size integers (4 bytes for i32/u32, 8 bytes for i64/u64)
//! - No support for i128/u128
//! - Strings are length-prefixed with 4-byte aligned padding
//! - Arrays have explicit length prefixes
//!
//! This crate will provide:
//! - `XdrParser` implementing `FormatParser`
//! - `XdrSerializer` implementing `FormatSerializer`
//! - `from_slice` and `to_vec` convenience functions

#![forbid(unsafe_code)]

// TODO: Implement XDR parser and serializer
// Reference implementation: facet-xdr/src/lib.rs

/// Placeholder - XDR implementation pending
pub fn placeholder() {}
