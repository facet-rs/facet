//! `facet-value` provides a memory-efficient dynamic value type for representing
//! structured data similar to JSON, but with added support for binary data and datetime.
//!
//! # Features
//!
//! - **Pointer-sized `Value` type**: The main `Value` type is exactly one pointer in size
//! - **Eight value types**: Null, Bool, Number, String, Bytes, Array, Object, DateTime
//! - **`no_std` compatible**: Works with just `alloc`, no standard library required
//! - **Bytes support**: First-class support for binary data (useful for MessagePack, CBOR, etc.)
//! - **DateTime support**: First-class support for temporal data (useful for TOML, YAML, etc.)
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

mod datetime;
pub use datetime::*;

mod other;
pub use other::{OtherKind, VQName, VUuid};

#[cfg(feature = "alloc")]
mod facet_impl;
#[cfg(feature = "alloc")]
pub use facet_impl::VALUE_SHAPE;

#[cfg(feature = "alloc")]
mod deserialize;
#[cfg(feature = "alloc")]
pub use deserialize::{PathSegment, ValueError, ValueErrorKind, from_value};

#[cfg(feature = "alloc")]
mod format;
#[cfg(feature = "alloc")]
pub use format::{FormattedValue, format_value, format_value_with_spans};

#[cfg(all(test, feature = "alloc"))]
mod inline_roundtrip_tests;
