//! JIT support for postcard format.
//!
//! This module provides Tier-2 format JIT for postcard deserialization,
//! enabling direct byte parsing without going through the event abstraction.
//!
//! Postcard is a binary format with NO trivia (whitespace/comments), which
//! makes it an excellent test case for the format-agnostic Tier-2 design.

/// Debug print macro for JIT - uses tracing::debug! in all builds.
macro_rules! jit_debug {
    ($($arg:tt)*) => { tracing::debug!($($arg)*) }
}

pub(crate) use jit_debug;

mod format;
mod helpers;

pub use format::PostcardJitFormat;
pub use helpers::{
    postcard_jit_parse_bool, postcard_jit_read_varint, postcard_jit_seq_begin,
    postcard_jit_seq_is_end, postcard_jit_seq_next,
};
