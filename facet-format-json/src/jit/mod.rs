//! JIT support for JSON format.
//!
//! This module provides Tier-2 format JIT for JSON deserialization,
//! enabling direct byte parsing without going through the event abstraction.

/// Debug print macro for JIT - only active in debug builds.
#[cfg(debug_assertions)]
macro_rules! jit_debug {
    ($($arg:tt)*) => { eprintln!($($arg)*) }
}

#[cfg(not(debug_assertions))]
macro_rules! jit_debug {
    ($($arg:tt)*) => {};
}

pub(crate) use jit_debug;

mod format;
mod helpers;

pub use format::JsonJitFormat;
pub use helpers::{
    json_jit_parse_bool, json_jit_seq_begin, json_jit_seq_is_end, json_jit_seq_next,
    json_jit_skip_ws,
};
