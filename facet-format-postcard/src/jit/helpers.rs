//! Postcard JIT helper functions for Tier-2 format JIT.
//!
//! # Implementation Status
//!
//! **These helpers are implemented but NOT currently used by the JIT.**
//!
//! The `JitFormat` implementation in [`super::format`] uses **inline Cranelift IR**
//! instead of calling these extern functions. This is intentional for now:
//!
//! - **Inline IR**: Faster (no call overhead), but harder to debug and maintain.

#![allow(clippy::missing_safety_doc)] // Safety docs are in function comments
//! - **Helpers**: Easier to test and debug, but adds function call overhead.
//!
//! The helpers here serve as:
//! 1. **Reference implementation**: Correct postcard parsing logic in readable Rust
//! 2. **Unit tests**: The `#[cfg(test)]` module validates parsing behavior
//! 3. **Future fallback**: Could be used if inline IR becomes too complex
//!
//! # Why Keep Both?
//!
//! During development, having both allows:
//! - Testing parsing logic without running JIT (via unit tests below)
//! - Comparing inline IR behavior against known-good Rust implementation
//! - Debugging by temporarily swapping to helper calls
//!
//! Long-term, we may:
//! - Delete helpers if inline IR is stable and well-tested
//! - Or switch to helpers if inline IR maintenance cost is too high
//!
//! # Helper ABI
//!
//! These extern "C" functions implement postcard parsing operations for direct
//! byte-level parsing by JIT-compiled code.

use super::jit_debug;

// =============================================================================
// Return Types
// =============================================================================

/// Return type for postcard_jit_seq_begin.
#[repr(C)]
pub struct PostcardJitPosError {
    /// New position after parsing
    pub new_pos: usize,
    /// Error code (0 = success, negative = error)
    pub error: i32,
}

/// Return type for postcard_jit_seq_is_end.
///
/// To fit in 2 return registers, we pack `is_end` into the high bit of `new_pos`.
#[repr(C)]
pub struct PostcardJitPosEndError {
    /// Packed: `(is_end << 63) | pos` (pos is unchanged for postcard)
    pub packed_pos_end: usize,
    /// Error code (0 = success, negative = error)
    pub error: i32,
}

impl PostcardJitPosEndError {
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

/// Return type for postcard_jit_parse_bool.
#[repr(C)]
pub struct PostcardJitPosValueError {
    /// Packed: `(value << 63) | new_pos`
    pub packed_pos_value: usize,
    /// Error code (0 = success, negative = error)
    pub error: i32,
}

impl PostcardJitPosValueError {
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

/// Return type for varint decoding.
#[repr(C)]
pub struct PostcardJitVarintResult {
    /// New position after parsing the varint
    pub new_pos: usize,
    /// Decoded value
    pub value: u64,
    /// Error code (0 = success, negative = error)
    pub error: i32,
}

// =============================================================================
// Error Codes
// =============================================================================

/// Postcard JIT error codes
pub mod error {
    /// Unexpected end of input
    pub const UNEXPECTED_EOF: i32 = -100;
    /// Invalid boolean value (not 0 or 1)
    pub const INVALID_BOOL: i32 = -101;
    /// Varint overflow (too many continuation bytes)
    pub const VARINT_OVERFLOW: i32 = -102;
    /// Sequence underflow (decrement when remaining is 0)
    pub const SEQ_UNDERFLOW: i32 = -103;
    /// Unsupported operation
    pub const UNSUPPORTED: i32 = -1;
}

// =============================================================================
// Helper Functions
// =============================================================================

/// Read a varint (LEB128) from the input.
///
/// Postcard uses unsigned LEB128 for lengths and unsigned integers.
/// Each byte has 7 data bits (0-6) and 1 continuation bit (7).
/// If bit 7 is set, more bytes follow.
///
/// Returns: (new_pos, value, error_code).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn postcard_jit_read_varint(
    input: *const u8,
    len: usize,
    pos: usize,
) -> PostcardJitVarintResult {
    jit_debug!("[postcard_jit_read_varint] pos={}, len={}", pos, len);

    let mut result: u64 = 0;
    let mut shift: u32 = 0;
    let mut p = pos;

    loop {
        if p >= len {
            jit_debug!("[postcard_jit_read_varint] EOF at pos={}", p);
            return PostcardJitVarintResult {
                new_pos: p,
                value: 0,
                error: error::UNEXPECTED_EOF,
            };
        }

        let byte = unsafe { *input.add(p) };
        p += 1;

        // Extract 7 data bits and add to result
        let data = (byte & 0x7F) as u64;

        // Check for overflow before shifting
        if shift >= 64 {
            jit_debug!("[postcard_jit_read_varint] overflow at shift={}", shift);
            return PostcardJitVarintResult {
                new_pos: p,
                value: 0,
                error: error::VARINT_OVERFLOW,
            };
        }

        result |= data << shift;
        shift += 7;

        // If continuation bit is clear, we're done
        if (byte & 0x80) == 0 {
            jit_debug!(
                "[postcard_jit_read_varint] done: value={}, new_pos={}",
                result,
                p
            );
            return PostcardJitVarintResult {
                new_pos: p,
                value: result,
                error: 0,
            };
        }
    }
}

/// Parse the start of a postcard sequence (read length varint).
///
/// Postcard sequences are length-prefixed: `[varint_length][elements...]`
/// This function reads the length and stores it in the state pointer.
///
/// Returns: (new_pos, error_code).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn postcard_jit_seq_begin(
    input: *const u8,
    len: usize,
    pos: usize,
    state_ptr: *mut u64,
) -> PostcardJitPosError {
    jit_debug!(
        "[postcard_jit_seq_begin] pos={}, len={}, state_ptr={:p}",
        pos,
        len,
        state_ptr
    );

    // Read the length varint
    let result = unsafe { postcard_jit_read_varint(input, len, pos) };

    if result.error != 0 {
        return PostcardJitPosError {
            new_pos: result.new_pos,
            error: result.error,
        };
    }

    // Store the remaining count in the state
    unsafe {
        *state_ptr = result.value;
    }

    jit_debug!(
        "[postcard_jit_seq_begin] remaining={}, new_pos={}",
        result.value,
        result.new_pos
    );

    PostcardJitPosError {
        new_pos: result.new_pos,
        error: 0,
    }
}

/// Check if at end of postcard sequence.
///
/// For postcard, "end" is determined by the remaining count in state, not by
/// reading a delimiter byte. If remaining == 0, we're at the end.
///
/// Returns: (packed_pos_end, error_code) where packed = (is_end << 63) | pos.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn postcard_jit_seq_is_end(
    pos: usize,
    state_ptr: *const u64,
) -> PostcardJitPosEndError {
    let remaining = unsafe { *state_ptr };
    jit_debug!(
        "[postcard_jit_seq_is_end] pos={}, remaining={}",
        pos,
        remaining
    );

    let is_end = remaining == 0;
    jit_debug!("[postcard_jit_seq_is_end] -> is_end={}", is_end);

    // Note: pos is unchanged - postcard doesn't consume any bytes for "end"
    PostcardJitPosEndError::new(pos, is_end, 0)
}

/// Advance to next sequence element.
///
/// For postcard, this just decrements the remaining count. No bytes are consumed
/// (the separator is implicit - elements are back-to-back).
///
/// Returns: (new_pos, error_code).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn postcard_jit_seq_next(
    pos: usize,
    state_ptr: *mut u64,
) -> PostcardJitPosError {
    let remaining = unsafe { *state_ptr };
    jit_debug!(
        "[postcard_jit_seq_next] pos={}, remaining={}",
        pos,
        remaining
    );

    if remaining == 0 {
        // This shouldn't happen if the protocol is followed correctly
        return PostcardJitPosError {
            new_pos: pos,
            error: error::SEQ_UNDERFLOW,
        };
    }

    // Decrement remaining count
    unsafe {
        *state_ptr = remaining - 1;
    }

    jit_debug!("[postcard_jit_seq_next] -> new_remaining={}", remaining - 1);

    // Position unchanged - no bytes consumed
    PostcardJitPosError {
        new_pos: pos,
        error: 0,
    }
}

/// Parse a postcard boolean.
///
/// Postcard bools are single bytes: 0 = false, 1 = true.
/// Any other value is an error.
///
/// Returns: (packed_pos_value, error_code) where packed = (value << 63) | new_pos.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn postcard_jit_parse_bool(
    input: *const u8,
    len: usize,
    pos: usize,
) -> PostcardJitPosValueError {
    jit_debug!("[postcard_jit_parse_bool] pos={}, len={}", pos, len);

    if pos >= len {
        jit_debug!("[postcard_jit_parse_bool] EOF!");
        return PostcardJitPosValueError::new(pos, false, error::UNEXPECTED_EOF);
    }

    let byte = unsafe { *input.add(pos) };
    jit_debug!("[postcard_jit_parse_bool] byte={}", byte);

    match byte {
        0 => {
            jit_debug!("[postcard_jit_parse_bool] -> false");
            PostcardJitPosValueError::new(pos + 1, false, 0)
        }
        1 => {
            jit_debug!("[postcard_jit_parse_bool] -> true");
            PostcardJitPosValueError::new(pos + 1, true, 0)
        }
        _ => {
            jit_debug!("[postcard_jit_parse_bool] -> invalid!");
            PostcardJitPosValueError::new(pos, false, error::INVALID_BOOL)
        }
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
pub unsafe extern "C" fn postcard_jit_bulk_copy_u8(dest: *mut u8, src: *const u8, count: usize) {
    jit_debug!(
        "[postcard_jit_bulk_copy_u8] dest={:p}, src={:p}, count={}",
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
    fn test_varint_single_byte() {
        // Values 0-127 encode as single byte
        let input = [0u8];
        let result = unsafe { postcard_jit_read_varint(input.as_ptr(), input.len(), 0) };
        assert_eq!(result.error, 0);
        assert_eq!(result.value, 0);
        assert_eq!(result.new_pos, 1);

        let input = [127u8];
        let result = unsafe { postcard_jit_read_varint(input.as_ptr(), input.len(), 0) };
        assert_eq!(result.error, 0);
        assert_eq!(result.value, 127);
        assert_eq!(result.new_pos, 1);
    }

    #[test]
    fn test_varint_multi_byte() {
        // 128 = 0x80 0x01 (continuation bit set on first byte)
        let input = [0x80, 0x01];
        let result = unsafe { postcard_jit_read_varint(input.as_ptr(), input.len(), 0) };
        assert_eq!(result.error, 0);
        assert_eq!(result.value, 128);
        assert_eq!(result.new_pos, 2);

        // 300 = 0xAC 0x02
        let input = [0xAC, 0x02];
        let result = unsafe { postcard_jit_read_varint(input.as_ptr(), input.len(), 0) };
        assert_eq!(result.error, 0);
        assert_eq!(result.value, 300);
        assert_eq!(result.new_pos, 2);
    }

    #[test]
    fn test_parse_bool() {
        let input = [0u8];
        let result = unsafe { postcard_jit_parse_bool(input.as_ptr(), input.len(), 0) };
        assert_eq!(result.error, 0);
        assert!(!result.value());
        assert_eq!(result.new_pos(), 1);

        let input = [1u8];
        let result = unsafe { postcard_jit_parse_bool(input.as_ptr(), input.len(), 0) };
        assert_eq!(result.error, 0);
        assert!(result.value());
        assert_eq!(result.new_pos(), 1);

        let input = [2u8];
        let result = unsafe { postcard_jit_parse_bool(input.as_ptr(), input.len(), 0) };
        assert_eq!(result.error, error::INVALID_BOOL);
    }

    #[test]
    fn test_seq_operations() {
        // Test sequence: [3, true, false, true] = [0x03, 0x01, 0x00, 0x01]
        let input = [0x03, 0x01, 0x00, 0x01];
        let mut state: u64 = 0;

        // Begin: read length (3)
        let result = unsafe { postcard_jit_seq_begin(input.as_ptr(), input.len(), 0, &mut state) };
        assert_eq!(result.error, 0);
        assert_eq!(result.new_pos, 1); // Past the varint
        assert_eq!(state, 3);

        // Check not at end
        let result = unsafe { postcard_jit_seq_is_end(result.new_pos, &state) };
        assert_eq!(result.error, 0);
        assert!(!result.is_end());

        // Advance (after parsing first element)
        let result = unsafe { postcard_jit_seq_next(result.pos(), &mut state) };
        assert_eq!(result.error, 0);
        assert_eq!(state, 2);

        // Advance twice more
        let result = unsafe { postcard_jit_seq_next(result.new_pos, &mut state) };
        assert_eq!(result.error, 0);
        assert_eq!(state, 1);

        let result = unsafe { postcard_jit_seq_next(result.new_pos, &mut state) };
        assert_eq!(result.error, 0);
        assert_eq!(state, 0);

        // Now at end
        let result = unsafe { postcard_jit_seq_is_end(result.new_pos, &state) };
        assert_eq!(result.error, 0);
        assert!(result.is_end());
    }
}
