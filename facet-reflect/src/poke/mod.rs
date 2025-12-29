//! Allows mutating values through reflection.
//!
//! This module provides the [`Poke`] type for mutating values at runtime.
//! Unlike [`Peek`](crate::Peek) which provides read-only access, `Poke` allows
//! modifying struct fields, enum variant data, and collection elements.
//!
//! # Safety
//!
//! Mutation through reflection is only safe for Plain Old Data (POD) types -
//! types that have no invariants. A type is POD if:
//!
//! - It's a primitive (`u32`, `bool`, `char`, etc.), OR
//! - It's marked with `#[facet(pod)]`
//!
//! Attempting to create a `Poke` for a non-POD type will fail.

mod value;
pub use value::*;

mod struct_;
pub use struct_::*;
