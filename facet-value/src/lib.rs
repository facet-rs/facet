//! `facet-value` provides a memory-efficient dynamic value type for representing
//! structured data similar to JSON, but with added support for binary data (bytes).
//!
//! # Features
//!
//! - **Pointer-sized `Value` type**: The main `Value` type is exactly one pointer in size
//! - **Seven value types**: Null, Bool, Number, String, Bytes, Array, Object
//! - **`no_std` compatible**: Works with just `alloc`, no standard library required
//! - **Bytes support**: First-class support for binary data (useful for MessagePack, CBOR, etc.)
//!
//! # Design
//!
//! `Value` uses a tagged pointer representation with 8-byte alignment, giving us 3 tag bits
//! to distinguish between value types. Inline values (null, true, false) don't require
//! heap allocation.

#![cfg_attr(not(feature = "std"), no_std)]
#![warn(missing_docs)]
#![warn(clippy::std_instead_of_core)]
#![warn(clippy::std_instead_of_alloc)]

#[cfg(feature = "alloc")]
extern crate alloc;

#[cfg(feature = "alloc")]
mod macros;

mod value;
pub use value::*;

mod number;
pub use number::*;

mod string;
pub use string::*;

mod bytes;
pub use bytes::*;

mod array;
pub use array::*;

mod object;
pub use object::*;
