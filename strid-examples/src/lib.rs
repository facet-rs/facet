//! Examples showing the output of using [`strid`] to
//! generate strongly-typed wrappers around string values.
//!
//! Three types of braids are demonstrated:
//! * [Wrapper][wrapper]
//!   * A wrapper around a string with [small-string optimizations][sso_wrapper]
//!   * A wrapper around a string [backed by `Bytes`][bytes]
//! * [Validated][validated]
//! * [Normalized][normalized]
//!
//! In addition, the [`minimal`] module demonstrates the minimal string
//! implementation that can be wrapped inside a braid type.
#![deny(unsafe_code)]

pub mod bytes;
pub mod minimal;
pub mod normalized;
pub mod ref_only;
pub mod sso_wrapper;
pub mod validated;
pub mod wrapper;
