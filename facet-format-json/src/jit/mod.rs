//! JIT support for JSON format.
//!
//! This module provides Tier-2 format JIT for JSON deserialization,
//! enabling direct byte parsing without going through the event abstraction.

mod format;

pub use format::JsonJitFormat;
