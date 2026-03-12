//! JIT support for MsgPack format.
//!
//! This module provides Tier-2 format JIT for MsgPack deserialization,
//! enabling direct byte parsing without going through the event abstraction.
//!
//! MsgPack is a tagged binary format with NO trivia (whitespace/comments), which
//! makes it an excellent test case for the format-agnostic Tier-2 design.
//!
//! ## Wire Format (v1 supported subset)
//!
//! ### Booleans
//! - `0xC2` = false
//! - `0xC3` = true
//!
//! ### Unsigned integers
//! - Positive fixint: `0x00..=0x7F` (value = tag)
//! - `0xCC` u8 (1 byte)
//! - `0xCD` u16 (2 bytes BE)
//! - `0xCE` u32 (4 bytes BE)
//! - `0xCF` u64 (8 bytes BE)
//!
//! ### Signed integers
//! - Negative fixint: `0xE0..=0xFF` (i8 from tag)
//! - `0xD0` i8 (1 byte)
//! - `0xD1` i16 (2 bytes BE)
//! - `0xD2` i32 (4 bytes BE)
//! - `0xD3` i64 (8 bytes BE)
//!
//! ### Binary data (bytes)
//! - `0xC4` bin8 (len u8 + bytes)
//! - `0xC5` bin16 (len u16 BE + bytes)
//! - `0xC6` bin32 (len u32 BE + bytes)
//!
//! ### Arrays
//! - Fixarray: `0x90..=0x9F` (count = low 4 bits)
//! - `0xDC` array16 (count u16 BE)
//! - `0xDD` array32 (count u32 BE)

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

pub use format::MsgPackJitFormat;
pub use helpers::{
    msgpack_jit_bulk_copy_u8, msgpack_jit_parse_bool, msgpack_jit_parse_i64, msgpack_jit_parse_u8,
    msgpack_jit_parse_u64, msgpack_jit_read_bin_header, msgpack_jit_seq_begin,
    msgpack_jit_seq_is_end, msgpack_jit_seq_next,
};
