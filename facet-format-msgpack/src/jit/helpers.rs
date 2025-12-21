//! MsgPack JIT helper functions for Tier-2 format JIT.
//!
//! These extern "C" functions implement MsgPack parsing operations for direct
//! byte-level parsing by JIT-compiled code.
//!
//! ## MsgPack Wire Format Tags
//!
//! | Range | Name | Description |
//! |-------|------|-------------|
//! | 0x00-0x7F | positive fixint | Value is the tag itself |

#![allow(clippy::missing_safety_doc)] // Safety docs are in function comments
//! | 0x90-0x9F | fixarray | Count = tag & 0x0F |
//! | 0xC2 | false | Boolean false |
//! | 0xC3 | true | Boolean true |
//! | 0xC4 | bin8 | Bytes with u8 length |
//! | 0xC5 | bin16 | Bytes with u16 BE length |
//! | 0xC6 | bin32 | Bytes with u32 BE length |
//! | 0xCC | u8 | Unsigned 8-bit |
//! | 0xCD | u16 | Unsigned 16-bit BE |
//! | 0xCE | u32 | Unsigned 32-bit BE |
//! | 0xCF | u64 | Unsigned 64-bit BE |
//! | 0xD0 | i8 | Signed 8-bit |
//! | 0xD1 | i16 | Signed 16-bit BE |
//! | 0xD2 | i32 | Signed 32-bit BE |
//! | 0xD3 | i64 | Signed 64-bit BE |
//! | 0xDC | array16 | Array with u16 BE count |
//! | 0xDD | array32 | Array with u32 BE count |
//! | 0xE0-0xFF | negative fixint | Value is (tag as i8) |

use super::jit_debug;

// =============================================================================
// MsgPack Tags
// =============================================================================

pub mod tags {
    // Booleans
    pub const FALSE: u8 = 0xC2;
    pub const TRUE: u8 = 0xC3;

    // Bin (bytes)
    pub const BIN8: u8 = 0xC4;
    pub const BIN16: u8 = 0xC5;
    pub const BIN32: u8 = 0xC6;

    // Unsigned integers
    pub const U8: u8 = 0xCC;
    pub const U16: u8 = 0xCD;
    pub const U32: u8 = 0xCE;
    pub const U64: u8 = 0xCF;

    // Signed integers
    pub const I8: u8 = 0xD0;
    pub const I16: u8 = 0xD1;
    pub const I32: u8 = 0xD2;
    pub const I64: u8 = 0xD3;

    // Arrays
    pub const ARRAY16: u8 = 0xDC;
    pub const ARRAY32: u8 = 0xDD;

    // Ranges
    pub const POSITIVE_FIXINT_MAX: u8 = 0x7F;
    pub const FIXARRAY_MIN: u8 = 0x90;
    pub const FIXARRAY_MAX: u8 = 0x9F;
    pub const NEGATIVE_FIXINT_MIN: u8 = 0xE0;

    /// Check if tag is a positive fixint (0x00..=0x7F)
    #[inline]
    pub const fn is_positive_fixint(tag: u8) -> bool {
        tag <= POSITIVE_FIXINT_MAX
    }

    /// Check if tag is a negative fixint (0xE0..=0xFF)
    #[inline]
    pub const fn is_negative_fixint(tag: u8) -> bool {
        tag >= NEGATIVE_FIXINT_MIN
    }

    /// Check if tag is a fixarray (0x90..=0x9F)
    #[inline]
    pub const fn is_fixarray(tag: u8) -> bool {
        tag >= FIXARRAY_MIN && tag <= FIXARRAY_MAX
    }
}

// =============================================================================
// Return Types
// =============================================================================

/// Return type for simple position+error results.
#[repr(C)]
pub struct MsgPackJitPosError {
    /// New position after parsing
    pub new_pos: usize,
    /// Error code (0 = success, negative = error)
    pub error: i32,
}

/// Return type for sequence is_end check.
#[repr(C)]
pub struct MsgPackJitPosEndError {
    /// Packed: `(is_end << 63) | pos`
    pub packed_pos_end: usize,
    /// Error code (0 = success, negative = error)
    pub error: i32,
}

impl MsgPackJitPosEndError {
    /// Create with explicit values
    pub fn new(pos: usize, is_end: bool, error: i32) -> Self {
        let packed_pos_end = if is_end { pos | (1usize << 63) } else { pos };
        Self {
            packed_pos_end,
            error,
        }
    }

    /// Extract pos from packed value
    #[allow(dead_code)]
    pub fn pos(&self) -> usize {
        self.packed_pos_end & 0x7FFFFFFFFFFFFFFF
    }

    /// Extract is_end from packed value
    #[allow(dead_code)]
    pub fn is_end(&self) -> bool {
        (self.packed_pos_end >> 63) != 0
    }
}

/// Return type for boolean parsing.
#[repr(C)]
pub struct MsgPackJitPosValueError {
    /// Packed: `(value << 63) | new_pos`
    pub packed_pos_value: usize,
    /// Error code (0 = success, negative = error)
    pub error: i32,
}

impl MsgPackJitPosValueError {
    /// Create with explicit values
    pub fn new(new_pos: usize, value: bool, error: i32) -> Self {
        let packed_pos_value = if value {
            new_pos | (1usize << 63)
        } else {
            new_pos
        };
        Self {
            packed_pos_value,
            error,
        }
    }

    /// Extract new_pos from packed value
    #[allow(dead_code)]
    pub fn new_pos(&self) -> usize {
        self.packed_pos_value & 0x7FFFFFFFFFFFFFFF
    }

    /// Extract value from packed value
    #[allow(dead_code)]
    pub fn value(&self) -> bool {
        (self.packed_pos_value >> 63) != 0
    }
}

/// Return type for integer parsing (u8).
#[repr(C)]
pub struct MsgPackJitU8Result {
    /// New position after parsing
    pub new_pos: usize,
    /// Parsed value
    pub value: u8,
    /// Error code (0 = success, negative = error)
    pub error: i32,
}

/// Return type for integer parsing (u64/i64).
#[repr(C)]
pub struct MsgPackJitI64Result {
    /// New position after parsing
    pub new_pos: usize,
    /// Parsed value (for u64, stored as i64 bits)
    pub value: i64,
    /// Error code (0 = success, negative = error)
    pub error: i32,
}

/// Return type for sequence begin (array header parsing).
#[repr(C)]
pub struct MsgPackJitSeqBeginResult {
    /// New position after parsing header
    pub new_pos: usize,
    /// Number of elements in the array
    pub count: usize,
    /// Error code (0 = success, negative = error)
    pub error: i32,
}

/// Return type for bin header parsing.
#[repr(C)]
pub struct MsgPackJitBinHeaderResult {
    /// New position after parsing header (start of data)
    pub new_pos: usize,
    /// Length of binary data in bytes
    pub len: usize,
    /// Error code (0 = success, negative = error)
    pub error: i32,
}

// =============================================================================
// Error Codes
// =============================================================================

/// MsgPack JIT error codes
pub mod error {
    /// Unexpected end of input
    pub const UNEXPECTED_EOF: i32 = -100;
    /// Expected bool tag (0xC2 or 0xC3)
    pub const EXPECTED_BOOL: i32 = -101;
    /// Expected array tag (fixarray/array16/array32)
    pub const EXPECTED_ARRAY: i32 = -102;
    /// Expected bin tag (bin8/bin16/bin32)
    pub const EXPECTED_BIN: i32 = -103;
    /// Expected integer tag
    pub const EXPECTED_INT: i32 = -104;
    /// Integer value overflows target type
    pub const INT_OVERFLOW: i32 = -105;
    /// Count doesn't fit in usize
    #[allow(dead_code)]
    pub const COUNT_OVERFLOW: i32 = -106;
    /// Sequence underflow (decrement when remaining is 0)
    pub const SEQ_UNDERFLOW: i32 = -107;
    /// Unsupported operation
    pub const UNSUPPORTED: i32 = -1;
}

// =============================================================================
// Helper Functions
// =============================================================================

/// Parse a MsgPack boolean.
///
/// MsgPack bools are tagged: 0xC2 = false, 0xC3 = true.
///
/// Returns: (packed_pos_value, error_code) where packed = (value << 63) | new_pos.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn msgpack_jit_parse_bool(
    input: *const u8,
    len: usize,
    pos: usize,
) -> MsgPackJitPosValueError {
    jit_debug!("[msgpack_jit_parse_bool] pos={}, len={}", pos, len);

    if pos >= len {
        jit_debug!("[msgpack_jit_parse_bool] EOF!");
        return MsgPackJitPosValueError::new(pos, false, error::UNEXPECTED_EOF);
    }

    let tag = unsafe { *input.add(pos) };
    jit_debug!("[msgpack_jit_parse_bool] tag=0x{:02X}", tag);

    match tag {
        tags::FALSE => {
            jit_debug!("[msgpack_jit_parse_bool] -> false");
            MsgPackJitPosValueError::new(pos + 1, false, 0)
        }
        tags::TRUE => {
            jit_debug!("[msgpack_jit_parse_bool] -> true");
            MsgPackJitPosValueError::new(pos + 1, true, 0)
        }
        _ => {
            jit_debug!("[msgpack_jit_parse_bool] -> invalid tag!");
            MsgPackJitPosValueError::new(pos, false, error::EXPECTED_BOOL)
        }
    }
}

/// Parse a MsgPack u8.
///
/// Accepts:
/// - Positive fixint (0x00..=0x7F)
/// - u8 tag (0xCC)
/// - (Permissive) u16/u32/u64 if value fits
///
/// Returns: (new_pos, value, error_code).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn msgpack_jit_parse_u8(
    input: *const u8,
    len: usize,
    pos: usize,
) -> MsgPackJitU8Result {
    jit_debug!("[msgpack_jit_parse_u8] pos={}, len={}", pos, len);

    if pos >= len {
        return MsgPackJitU8Result {
            new_pos: pos,
            value: 0,
            error: error::UNEXPECTED_EOF,
        };
    }

    let tag = unsafe { *input.add(pos) };
    jit_debug!("[msgpack_jit_parse_u8] tag=0x{:02X}", tag);

    // Positive fixint (0x00..=0x7F)
    if tags::is_positive_fixint(tag) {
        return MsgPackJitU8Result {
            new_pos: pos + 1,
            value: tag,
            error: 0,
        };
    }

    match tag {
        tags::U8 => {
            if pos + 1 >= len {
                return MsgPackJitU8Result {
                    new_pos: pos + 1,
                    value: 0,
                    error: error::UNEXPECTED_EOF,
                };
            }
            let value = unsafe { *input.add(pos + 1) };
            MsgPackJitU8Result {
                new_pos: pos + 2,
                value,
                error: 0,
            }
        }
        tags::U16 => {
            if pos + 2 >= len {
                return MsgPackJitU8Result {
                    new_pos: pos + 1,
                    value: 0,
                    error: error::UNEXPECTED_EOF,
                };
            }
            let value = u16::from_be_bytes(unsafe { [*input.add(pos + 1), *input.add(pos + 2)] });
            if value > u8::MAX as u16 {
                return MsgPackJitU8Result {
                    new_pos: pos,
                    value: 0,
                    error: error::INT_OVERFLOW,
                };
            }
            MsgPackJitU8Result {
                new_pos: pos + 3,
                value: value as u8,
                error: 0,
            }
        }
        tags::U32 => {
            if pos + 4 >= len {
                return MsgPackJitU8Result {
                    new_pos: pos + 1,
                    value: 0,
                    error: error::UNEXPECTED_EOF,
                };
            }
            let value = u32::from_be_bytes(unsafe {
                [
                    *input.add(pos + 1),
                    *input.add(pos + 2),
                    *input.add(pos + 3),
                    *input.add(pos + 4),
                ]
            });
            if value > u8::MAX as u32 {
                return MsgPackJitU8Result {
                    new_pos: pos,
                    value: 0,
                    error: error::INT_OVERFLOW,
                };
            }
            MsgPackJitU8Result {
                new_pos: pos + 5,
                value: value as u8,
                error: 0,
            }
        }
        tags::U64 => {
            if pos + 8 >= len {
                return MsgPackJitU8Result {
                    new_pos: pos + 1,
                    value: 0,
                    error: error::UNEXPECTED_EOF,
                };
            }
            let value = u64::from_be_bytes(unsafe {
                [
                    *input.add(pos + 1),
                    *input.add(pos + 2),
                    *input.add(pos + 3),
                    *input.add(pos + 4),
                    *input.add(pos + 5),
                    *input.add(pos + 6),
                    *input.add(pos + 7),
                    *input.add(pos + 8),
                ]
            });
            if value > u8::MAX as u64 {
                return MsgPackJitU8Result {
                    new_pos: pos,
                    value: 0,
                    error: error::INT_OVERFLOW,
                };
            }
            MsgPackJitU8Result {
                new_pos: pos + 9,
                value: value as u8,
                error: 0,
            }
        }
        _ => MsgPackJitU8Result {
            new_pos: pos,
            value: 0,
            error: error::EXPECTED_INT,
        },
    }
}

/// Parse a MsgPack unsigned integer as u64.
///
/// Accepts:
/// - Positive fixint (0x00..=0x7F)
/// - u8/u16/u32/u64 tags
///
/// Returns: (new_pos, value as i64, error_code).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn msgpack_jit_parse_u64(
    input: *const u8,
    len: usize,
    pos: usize,
) -> MsgPackJitI64Result {
    jit_debug!("[msgpack_jit_parse_u64] pos={}, len={}", pos, len);

    if pos >= len {
        return MsgPackJitI64Result {
            new_pos: pos,
            value: 0,
            error: error::UNEXPECTED_EOF,
        };
    }

    let tag = unsafe { *input.add(pos) };
    jit_debug!("[msgpack_jit_parse_u64] tag=0x{:02X}", tag);

    // Positive fixint (0x00..=0x7F)
    if tags::is_positive_fixint(tag) {
        return MsgPackJitI64Result {
            new_pos: pos + 1,
            value: tag as i64,
            error: 0,
        };
    }

    match tag {
        tags::U8 => {
            if pos + 1 >= len {
                return MsgPackJitI64Result {
                    new_pos: pos + 1,
                    value: 0,
                    error: error::UNEXPECTED_EOF,
                };
            }
            let value = unsafe { *input.add(pos + 1) };
            MsgPackJitI64Result {
                new_pos: pos + 2,
                value: value as i64,
                error: 0,
            }
        }
        tags::U16 => {
            if pos + 2 >= len {
                return MsgPackJitI64Result {
                    new_pos: pos + 1,
                    value: 0,
                    error: error::UNEXPECTED_EOF,
                };
            }
            let value = u16::from_be_bytes(unsafe { [*input.add(pos + 1), *input.add(pos + 2)] });
            MsgPackJitI64Result {
                new_pos: pos + 3,
                value: value as i64,
                error: 0,
            }
        }
        tags::U32 => {
            if pos + 4 >= len {
                return MsgPackJitI64Result {
                    new_pos: pos + 1,
                    value: 0,
                    error: error::UNEXPECTED_EOF,
                };
            }
            let value = u32::from_be_bytes(unsafe {
                [
                    *input.add(pos + 1),
                    *input.add(pos + 2),
                    *input.add(pos + 3),
                    *input.add(pos + 4),
                ]
            });
            MsgPackJitI64Result {
                new_pos: pos + 5,
                value: value as i64,
                error: 0,
            }
        }
        tags::U64 => {
            if pos + 8 >= len {
                return MsgPackJitI64Result {
                    new_pos: pos + 1,
                    value: 0,
                    error: error::UNEXPECTED_EOF,
                };
            }
            let value = u64::from_be_bytes(unsafe {
                [
                    *input.add(pos + 1),
                    *input.add(pos + 2),
                    *input.add(pos + 3),
                    *input.add(pos + 4),
                    *input.add(pos + 5),
                    *input.add(pos + 6),
                    *input.add(pos + 7),
                    *input.add(pos + 8),
                ]
            });
            // Note: u64 bits stored in i64 - caller interprets as unsigned
            MsgPackJitI64Result {
                new_pos: pos + 9,
                value: value as i64,
                error: 0,
            }
        }
        _ => MsgPackJitI64Result {
            new_pos: pos,
            value: 0,
            error: error::EXPECTED_INT,
        },
    }
}

/// Parse a MsgPack signed integer as i64.
///
/// Accepts (permissive mode):
/// - Positive fixint (0x00..=0x7F) as positive i64
/// - Negative fixint (0xE0..=0xFF) as negative i64
/// - i8/i16/i32/i64 tags
/// - u8/u16/u32/u64 tags (if value fits in i64)
///
/// Returns: (new_pos, value, error_code).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn msgpack_jit_parse_i64(
    input: *const u8,
    len: usize,
    pos: usize,
) -> MsgPackJitI64Result {
    jit_debug!("[msgpack_jit_parse_i64] pos={}, len={}", pos, len);

    if pos >= len {
        return MsgPackJitI64Result {
            new_pos: pos,
            value: 0,
            error: error::UNEXPECTED_EOF,
        };
    }

    let tag = unsafe { *input.add(pos) };
    jit_debug!("[msgpack_jit_parse_i64] tag=0x{:02X}", tag);

    // Positive fixint (0x00..=0x7F)
    if tags::is_positive_fixint(tag) {
        return MsgPackJitI64Result {
            new_pos: pos + 1,
            value: tag as i64,
            error: 0,
        };
    }

    // Negative fixint (0xE0..=0xFF)
    if tags::is_negative_fixint(tag) {
        return MsgPackJitI64Result {
            new_pos: pos + 1,
            value: (tag as i8) as i64,
            error: 0,
        };
    }

    match tag {
        // Signed integer tags
        tags::I8 => {
            if pos + 1 >= len {
                return MsgPackJitI64Result {
                    new_pos: pos + 1,
                    value: 0,
                    error: error::UNEXPECTED_EOF,
                };
            }
            let value = unsafe { *input.add(pos + 1) } as i8;
            MsgPackJitI64Result {
                new_pos: pos + 2,
                value: value as i64,
                error: 0,
            }
        }
        tags::I16 => {
            if pos + 2 >= len {
                return MsgPackJitI64Result {
                    new_pos: pos + 1,
                    value: 0,
                    error: error::UNEXPECTED_EOF,
                };
            }
            let value = i16::from_be_bytes(unsafe { [*input.add(pos + 1), *input.add(pos + 2)] });
            MsgPackJitI64Result {
                new_pos: pos + 3,
                value: value as i64,
                error: 0,
            }
        }
        tags::I32 => {
            if pos + 4 >= len {
                return MsgPackJitI64Result {
                    new_pos: pos + 1,
                    value: 0,
                    error: error::UNEXPECTED_EOF,
                };
            }
            let value = i32::from_be_bytes(unsafe {
                [
                    *input.add(pos + 1),
                    *input.add(pos + 2),
                    *input.add(pos + 3),
                    *input.add(pos + 4),
                ]
            });
            MsgPackJitI64Result {
                new_pos: pos + 5,
                value: value as i64,
                error: 0,
            }
        }
        tags::I64 => {
            if pos + 8 >= len {
                return MsgPackJitI64Result {
                    new_pos: pos + 1,
                    value: 0,
                    error: error::UNEXPECTED_EOF,
                };
            }
            let value = i64::from_be_bytes(unsafe {
                [
                    *input.add(pos + 1),
                    *input.add(pos + 2),
                    *input.add(pos + 3),
                    *input.add(pos + 4),
                    *input.add(pos + 5),
                    *input.add(pos + 6),
                    *input.add(pos + 7),
                    *input.add(pos + 8),
                ]
            });
            MsgPackJitI64Result {
                new_pos: pos + 9,
                value,
                error: 0,
            }
        }
        // Unsigned integer tags (permissive: accept if fits in i64)
        tags::U8 => {
            if pos + 1 >= len {
                return MsgPackJitI64Result {
                    new_pos: pos + 1,
                    value: 0,
                    error: error::UNEXPECTED_EOF,
                };
            }
            let value = unsafe { *input.add(pos + 1) };
            MsgPackJitI64Result {
                new_pos: pos + 2,
                value: value as i64,
                error: 0,
            }
        }
        tags::U16 => {
            if pos + 2 >= len {
                return MsgPackJitI64Result {
                    new_pos: pos + 1,
                    value: 0,
                    error: error::UNEXPECTED_EOF,
                };
            }
            let value = u16::from_be_bytes(unsafe { [*input.add(pos + 1), *input.add(pos + 2)] });
            MsgPackJitI64Result {
                new_pos: pos + 3,
                value: value as i64,
                error: 0,
            }
        }
        tags::U32 => {
            if pos + 4 >= len {
                return MsgPackJitI64Result {
                    new_pos: pos + 1,
                    value: 0,
                    error: error::UNEXPECTED_EOF,
                };
            }
            let value = u32::from_be_bytes(unsafe {
                [
                    *input.add(pos + 1),
                    *input.add(pos + 2),
                    *input.add(pos + 3),
                    *input.add(pos + 4),
                ]
            });
            MsgPackJitI64Result {
                new_pos: pos + 5,
                value: value as i64,
                error: 0,
            }
        }
        tags::U64 => {
            if pos + 8 >= len {
                return MsgPackJitI64Result {
                    new_pos: pos + 1,
                    value: 0,
                    error: error::UNEXPECTED_EOF,
                };
            }
            let value = u64::from_be_bytes(unsafe {
                [
                    *input.add(pos + 1),
                    *input.add(pos + 2),
                    *input.add(pos + 3),
                    *input.add(pos + 4),
                    *input.add(pos + 5),
                    *input.add(pos + 6),
                    *input.add(pos + 7),
                    *input.add(pos + 8),
                ]
            });
            // Check if value fits in i64
            if value > i64::MAX as u64 {
                return MsgPackJitI64Result {
                    new_pos: pos,
                    value: 0,
                    error: error::INT_OVERFLOW,
                };
            }
            MsgPackJitI64Result {
                new_pos: pos + 9,
                value: value as i64,
                error: 0,
            }
        }
        _ => MsgPackJitI64Result {
            new_pos: pos,
            value: 0,
            error: error::EXPECTED_INT,
        },
    }
}

/// Parse the start of a MsgPack array (read count).
///
/// MsgPack arrays are tagged:
/// - fixarray (0x90..=0x9F): count in low 4 bits
/// - array16 (0xDC): count as u16 BE
/// - array32 (0xDD): count as u32 BE
///
/// Returns: (new_pos, count, error_code).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn msgpack_jit_seq_begin(
    input: *const u8,
    len: usize,
    pos: usize,
    state_ptr: *mut u64,
) -> MsgPackJitSeqBeginResult {
    jit_debug!(
        "[msgpack_jit_seq_begin] pos={}, len={}, state_ptr={:p}",
        pos,
        len,
        state_ptr
    );

    if pos >= len {
        return MsgPackJitSeqBeginResult {
            new_pos: pos,
            count: 0,
            error: error::UNEXPECTED_EOF,
        };
    }

    let tag = unsafe { *input.add(pos) };
    jit_debug!("[msgpack_jit_seq_begin] tag=0x{:02X}", tag);

    let (new_pos, count): (usize, usize) = if tags::is_fixarray(tag) {
        // Fixarray: count in low 4 bits
        let count = (tag & 0x0F) as usize;
        (pos + 1, count)
    } else {
        match tag {
            tags::ARRAY16 => {
                if pos + 2 >= len {
                    return MsgPackJitSeqBeginResult {
                        new_pos: pos + 1,
                        count: 0,
                        error: error::UNEXPECTED_EOF,
                    };
                }
                let count =
                    u16::from_be_bytes(unsafe { [*input.add(pos + 1), *input.add(pos + 2)] })
                        as usize;
                (pos + 3, count)
            }
            tags::ARRAY32 => {
                if pos + 4 >= len {
                    return MsgPackJitSeqBeginResult {
                        new_pos: pos + 1,
                        count: 0,
                        error: error::UNEXPECTED_EOF,
                    };
                }
                let count_u32 = u32::from_be_bytes(unsafe {
                    [
                        *input.add(pos + 1),
                        *input.add(pos + 2),
                        *input.add(pos + 3),
                        *input.add(pos + 4),
                    ]
                });
                // Check for overflow on 32-bit platforms
                #[cfg(target_pointer_width = "32")]
                if count_u32 > usize::MAX as u32 {
                    return MsgPackJitSeqBeginResult {
                        new_pos: pos,
                        count: 0,
                        error: error::COUNT_OVERFLOW,
                    };
                }
                (pos + 5, count_u32 as usize)
            }
            _ => {
                return MsgPackJitSeqBeginResult {
                    new_pos: pos,
                    count: 0,
                    error: error::EXPECTED_ARRAY,
                };
            }
        }
    };

    // Store count in state for is_end/next
    unsafe {
        *state_ptr = count as u64;
    }

    jit_debug!(
        "[msgpack_jit_seq_begin] count={}, new_pos={}",
        count,
        new_pos
    );

    MsgPackJitSeqBeginResult {
        new_pos,
        count,
        error: 0,
    }
}

/// Check if at end of MsgPack sequence.
///
/// For MsgPack, "end" is determined by the remaining count in state.
/// If remaining == 0, we're at the end.
///
/// Returns: (packed_pos_end, error_code) where packed = (is_end << 63) | pos.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn msgpack_jit_seq_is_end(
    pos: usize,
    state_ptr: *const u64,
) -> MsgPackJitPosEndError {
    let remaining = unsafe { *state_ptr };
    jit_debug!(
        "[msgpack_jit_seq_is_end] pos={}, remaining={}",
        pos,
        remaining
    );

    let is_end = remaining == 0;
    jit_debug!("[msgpack_jit_seq_is_end] -> is_end={}", is_end);

    MsgPackJitPosEndError::new(pos, is_end, 0)
}

/// Advance to next sequence element.
///
/// For MsgPack, this just decrements the remaining count.
/// No bytes are consumed (elements are back-to-back).
///
/// Returns: (new_pos, error_code).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn msgpack_jit_seq_next(
    pos: usize,
    state_ptr: *mut u64,
) -> MsgPackJitPosError {
    let remaining = unsafe { *state_ptr };
    jit_debug!(
        "[msgpack_jit_seq_next] pos={}, remaining={}",
        pos,
        remaining
    );

    if remaining == 0 {
        return MsgPackJitPosError {
            new_pos: pos,
            error: error::SEQ_UNDERFLOW,
        };
    }

    unsafe {
        *state_ptr = remaining - 1;
    }

    jit_debug!("[msgpack_jit_seq_next] -> new_remaining={}", remaining - 1);

    MsgPackJitPosError {
        new_pos: pos,
        error: 0,
    }
}

/// Parse a MsgPack bin header (for Vec<u8> fast path).
///
/// MsgPack binary data is tagged:
/// - bin8 (0xC4): length as u8
/// - bin16 (0xC5): length as u16 BE
/// - bin32 (0xC6): length as u32 BE
///
/// Returns: (new_pos, len, error_code) where new_pos points to start of data.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn msgpack_jit_read_bin_header(
    input: *const u8,
    len: usize,
    pos: usize,
) -> MsgPackJitBinHeaderResult {
    jit_debug!("[msgpack_jit_read_bin_header] pos={}, len={}", pos, len);

    if pos >= len {
        return MsgPackJitBinHeaderResult {
            new_pos: pos,
            len: 0,
            error: error::UNEXPECTED_EOF,
        };
    }

    let tag = unsafe { *input.add(pos) };
    jit_debug!("[msgpack_jit_read_bin_header] tag=0x{:02X}", tag);

    let (new_pos, data_len): (usize, usize) = match tag {
        tags::BIN8 => {
            if pos + 1 >= len {
                return MsgPackJitBinHeaderResult {
                    new_pos: pos + 1,
                    len: 0,
                    error: error::UNEXPECTED_EOF,
                };
            }
            let data_len = unsafe { *input.add(pos + 1) } as usize;
            (pos + 2, data_len)
        }
        tags::BIN16 => {
            if pos + 2 >= len {
                return MsgPackJitBinHeaderResult {
                    new_pos: pos + 1,
                    len: 0,
                    error: error::UNEXPECTED_EOF,
                };
            }
            let data_len =
                u16::from_be_bytes(unsafe { [*input.add(pos + 1), *input.add(pos + 2)] }) as usize;
            (pos + 3, data_len)
        }
        tags::BIN32 => {
            if pos + 4 >= len {
                return MsgPackJitBinHeaderResult {
                    new_pos: pos + 1,
                    len: 0,
                    error: error::UNEXPECTED_EOF,
                };
            }
            let data_len_u32 = u32::from_be_bytes(unsafe {
                [
                    *input.add(pos + 1),
                    *input.add(pos + 2),
                    *input.add(pos + 3),
                    *input.add(pos + 4),
                ]
            });
            // Check for overflow on 32-bit platforms
            #[cfg(target_pointer_width = "32")]
            if data_len_u32 > usize::MAX as u32 {
                return MsgPackJitBinHeaderResult {
                    new_pos: pos,
                    len: 0,
                    error: error::COUNT_OVERFLOW,
                };
            }
            (pos + 5, data_len_u32 as usize)
        }
        _ => {
            return MsgPackJitBinHeaderResult {
                new_pos: pos,
                len: 0,
                error: error::EXPECTED_BIN,
            };
        }
    };

    // Verify there's enough data
    if new_pos + data_len > len {
        return MsgPackJitBinHeaderResult {
            new_pos,
            len: 0,
            error: error::UNEXPECTED_EOF,
        };
    }

    jit_debug!(
        "[msgpack_jit_read_bin_header] data_len={}, new_pos={}",
        data_len,
        new_pos
    );

    MsgPackJitBinHeaderResult {
        new_pos,
        len: data_len,
        error: 0,
    }
}

/// Bulk copy bytes for Vec<u8> fast path.
///
/// This is called after bounds checking has been done by the JIT.
/// Simply copies `count` bytes from `src` to `dest`.
///
/// # Safety
/// - `dest` must be valid for writes of `count` bytes
/// - `src` must be valid for reads of `count` bytes
/// - The memory regions must not overlap
#[unsafe(no_mangle)]
pub unsafe extern "C" fn msgpack_jit_bulk_copy_u8(dest: *mut u8, src: *const u8, count: usize) {
    jit_debug!(
        "[msgpack_jit_bulk_copy_u8] dest={:p}, src={:p}, count={}",
        dest,
        src,
        count
    );
    unsafe {
        core::ptr::copy_nonoverlapping(src, dest, count);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_bool() {
        // false (0xC2)
        let input = [0xC2];
        let result = unsafe { msgpack_jit_parse_bool(input.as_ptr(), input.len(), 0) };
        assert_eq!(result.error, 0);
        assert!(!result.value());
        assert_eq!(result.new_pos(), 1);

        // true (0xC3)
        let input = [0xC3];
        let result = unsafe { msgpack_jit_parse_bool(input.as_ptr(), input.len(), 0) };
        assert_eq!(result.error, 0);
        assert!(result.value());
        assert_eq!(result.new_pos(), 1);

        // Invalid tag
        let input = [0x00];
        let result = unsafe { msgpack_jit_parse_bool(input.as_ptr(), input.len(), 0) };
        assert_eq!(result.error, error::EXPECTED_BOOL);
    }

    #[test]
    fn test_parse_u64_fixint() {
        // Positive fixint: 0
        let input = [0x00];
        let result = unsafe { msgpack_jit_parse_u64(input.as_ptr(), input.len(), 0) };
        assert_eq!(result.error, 0);
        assert_eq!(result.value, 0);
        assert_eq!(result.new_pos, 1);

        // Positive fixint: 127
        let input = [0x7F];
        let result = unsafe { msgpack_jit_parse_u64(input.as_ptr(), input.len(), 0) };
        assert_eq!(result.error, 0);
        assert_eq!(result.value, 127);
        assert_eq!(result.new_pos, 1);
    }

    #[test]
    fn test_parse_u64_typed() {
        // u8: 200
        let input = [0xCC, 200];
        let result = unsafe { msgpack_jit_parse_u64(input.as_ptr(), input.len(), 0) };
        assert_eq!(result.error, 0);
        assert_eq!(result.value, 200);
        assert_eq!(result.new_pos, 2);

        // u16: 1000
        let input = [0xCD, 0x03, 0xE8]; // 1000 in BE
        let result = unsafe { msgpack_jit_parse_u64(input.as_ptr(), input.len(), 0) };
        assert_eq!(result.error, 0);
        assert_eq!(result.value, 1000);
        assert_eq!(result.new_pos, 3);

        // u32: 100000
        let input = [0xCE, 0x00, 0x01, 0x86, 0xA0]; // 100000 in BE
        let result = unsafe { msgpack_jit_parse_u64(input.as_ptr(), input.len(), 0) };
        assert_eq!(result.error, 0);
        assert_eq!(result.value, 100000);
        assert_eq!(result.new_pos, 5);
    }

    #[test]
    fn test_parse_i64_fixint() {
        // Positive fixint: 42
        let input = [42];
        let result = unsafe { msgpack_jit_parse_i64(input.as_ptr(), input.len(), 0) };
        assert_eq!(result.error, 0);
        assert_eq!(result.value, 42);
        assert_eq!(result.new_pos, 1);

        // Negative fixint: -1 (0xFF)
        let input = [0xFF];
        let result = unsafe { msgpack_jit_parse_i64(input.as_ptr(), input.len(), 0) };
        assert_eq!(result.error, 0);
        assert_eq!(result.value, -1);
        assert_eq!(result.new_pos, 1);

        // Negative fixint: -32 (0xE0)
        let input = [0xE0];
        let result = unsafe { msgpack_jit_parse_i64(input.as_ptr(), input.len(), 0) };
        assert_eq!(result.error, 0);
        assert_eq!(result.value, -32);
        assert_eq!(result.new_pos, 1);
    }

    #[test]
    fn test_parse_i64_typed() {
        // i8: -100
        let input = [0xD0, 0x9C]; // -100 as i8
        let result = unsafe { msgpack_jit_parse_i64(input.as_ptr(), input.len(), 0) };
        assert_eq!(result.error, 0);
        assert_eq!(result.value, -100);
        assert_eq!(result.new_pos, 2);

        // i16: -1000
        let input = [0xD1, 0xFC, 0x18]; // -1000 in BE
        let result = unsafe { msgpack_jit_parse_i64(input.as_ptr(), input.len(), 0) };
        assert_eq!(result.error, 0);
        assert_eq!(result.value, -1000);
        assert_eq!(result.new_pos, 3);
    }

    #[test]
    fn test_seq_begin_fixarray() {
        // fixarray with 3 elements
        let input = [0x93, 0x01, 0x02, 0x03];
        let mut state: u64 = 0;
        let result = unsafe { msgpack_jit_seq_begin(input.as_ptr(), input.len(), 0, &mut state) };
        assert_eq!(result.error, 0);
        assert_eq!(result.count, 3);
        assert_eq!(result.new_pos, 1);
        assert_eq!(state, 3);
    }

    #[test]
    fn test_seq_begin_array16() {
        // array16 with 256 elements
        let input = [0xDC, 0x01, 0x00];
        let mut state: u64 = 0;
        let result = unsafe { msgpack_jit_seq_begin(input.as_ptr(), input.len(), 0, &mut state) };
        assert_eq!(result.error, 0);
        assert_eq!(result.count, 256);
        assert_eq!(result.new_pos, 3);
        assert_eq!(state, 256);
    }

    #[test]
    fn test_seq_operations() {
        // Array of 3 bools: [0x93, 0xC3, 0xC2, 0xC3] = [true, false, true]
        let input = [0x93, 0xC3, 0xC2, 0xC3];
        let mut state: u64 = 0;

        // Begin: read array header
        let result = unsafe { msgpack_jit_seq_begin(input.as_ptr(), input.len(), 0, &mut state) };
        assert_eq!(result.error, 0);
        assert_eq!(result.count, 3);
        assert_eq!(result.new_pos, 1);
        assert_eq!(state, 3);

        // Check not at end
        let result = unsafe { msgpack_jit_seq_is_end(result.new_pos, &state) };
        assert_eq!(result.error, 0);
        assert!(!result.is_end());

        // Advance after parsing first element
        let result = unsafe { msgpack_jit_seq_next(result.pos(), &mut state) };
        assert_eq!(result.error, 0);
        assert_eq!(state, 2);

        // Advance twice more
        let result = unsafe { msgpack_jit_seq_next(result.new_pos, &mut state) };
        assert_eq!(result.error, 0);
        assert_eq!(state, 1);

        let result = unsafe { msgpack_jit_seq_next(result.new_pos, &mut state) };
        assert_eq!(result.error, 0);
        assert_eq!(state, 0);

        // Now at end
        let result = unsafe { msgpack_jit_seq_is_end(result.new_pos, &state) };
        assert_eq!(result.error, 0);
        assert!(result.is_end());
    }

    #[test]
    fn test_bin_header() {
        // bin8 with 3 bytes
        let input = [0xC4, 0x03, 0xAA, 0xBB, 0xCC];
        let result = unsafe { msgpack_jit_read_bin_header(input.as_ptr(), input.len(), 0) };
        assert_eq!(result.error, 0);
        assert_eq!(result.len, 3);
        assert_eq!(result.new_pos, 2); // Points to start of data

        // bin16 with 256 bytes
        let mut input = vec![0xC5, 0x01, 0x00];
        input.extend(vec![0u8; 256]);
        let result = unsafe { msgpack_jit_read_bin_header(input.as_ptr(), input.len(), 0) };
        assert_eq!(result.error, 0);
        assert_eq!(result.len, 256);
        assert_eq!(result.new_pos, 3);
    }

    #[test]
    fn test_eof_errors() {
        // Empty input
        let input: [u8; 0] = [];
        let result = unsafe { msgpack_jit_parse_bool(input.as_ptr(), input.len(), 0) };
        assert_eq!(result.error, error::UNEXPECTED_EOF);

        let result = unsafe { msgpack_jit_parse_u64(input.as_ptr(), input.len(), 0) };
        assert_eq!(result.error, error::UNEXPECTED_EOF);

        // u16 tag but not enough bytes
        let input = [0xCD, 0x00]; // Need 2 more bytes
        let result = unsafe { msgpack_jit_parse_u64(input.as_ptr(), input.len(), 0) };
        assert_eq!(result.error, error::UNEXPECTED_EOF);
    }
}
