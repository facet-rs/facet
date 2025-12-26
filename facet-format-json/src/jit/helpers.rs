//! JSON JIT helper functions for Tier-2 format JIT.
//!
//! These extern "C" functions implement JSON parsing operations for direct
//! byte-level parsing by JIT-compiled code.

#![allow(clippy::missing_safety_doc)] // Safety docs are in function comments

use facet_format::jit::JitScratch;

use super::jit_debug;

// =============================================================================
// Return Types
// =============================================================================

/// Return type for simple JIT helpers that return position or error.
///
/// On Windows x64, returning a struct > 8 bytes requires a hidden first parameter,
/// which breaks Cranelift's multi-return-value expectations. So we pack into isize:
/// - `>= 0`: success, value is new_pos
/// - `< 0`: error code
pub type JsonJitResult = isize;

/// Legacy struct type - DO NOT USE for new extern "C" functions called from JIT.
/// Kept for compatibility with internal helper functions.
#[repr(C)]
pub struct JsonJitPosError {
    /// New position after parsing
    pub new_pos: usize,
    /// Error code (0 = success, negative = error)
    pub error: i32,
}

impl JsonJitPosError {
    /// Convert to single-value result for JIT return.
    #[inline]
    pub fn into_result(self) -> JsonJitResult {
        if self.error == 0 {
            self.new_pos as isize
        } else {
            self.error as isize
        }
    }
}

/// Return type for json_jit_seq_is_end.
///
/// To fit in 2 return registers, we pack `is_end` into the high bit of `new_pos`.
/// Use `unpack_pos_end()` to extract the values.
#[repr(C)]
pub struct JsonJitPosEndError {
    /// Packed: `(is_end << 63) | new_pos`
    /// Extract with: `new_pos = packed & 0x7FFFFFFFFFFFFFFF`, `is_end = packed >> 63`
    pub packed_pos_end: usize,
    /// Error code (0 = success, negative = error)
    pub error: i32,
}

impl JsonJitPosEndError {
    /// Create with explicit values
    pub fn new(new_pos: usize, is_end: bool, error: i32) -> Self {
        let packed_pos_end = if is_end {
            new_pos | (1usize << 63)
        } else {
            new_pos
        };
        Self {
            packed_pos_end,
            error,
        }
    }

    /// Extract new_pos from packed value
    #[allow(dead_code)]
    pub fn new_pos(&self) -> usize {
        self.packed_pos_end & 0x7FFFFFFFFFFFFFFF
    }

    /// Extract is_end from packed value
    #[allow(dead_code)]
    pub fn is_end(&self) -> bool {
        (self.packed_pos_end >> 63) != 0
    }
}

/// Return type for json_jit_parse_bool.
///
/// To fit in 2 return registers, we pack `value` into the high bit of `new_pos`.
/// Use `unpack_pos_value()` to extract the values.
#[repr(C)]
pub struct JsonJitPosValueError {
    /// Packed: `(value << 63) | new_pos`
    /// Extract with: `new_pos = packed & 0x7FFFFFFFFFFFFFFF`, `value = packed >> 63`
    pub packed_pos_value: usize,
    /// Error code (0 = success, negative = error)
    pub error: i32,
}

impl JsonJitPosValueError {
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

// =============================================================================
// Error Codes
// =============================================================================

/// JSON JIT error codes
pub mod error {
    /// Unexpected end of input
    pub const UNEXPECTED_EOF: i32 = -100;
    /// Expected '[' for array start
    pub const EXPECTED_ARRAY_START: i32 = -101;
    /// Expected 'true' or 'false'
    pub const EXPECTED_BOOL: i32 = -102;
    /// Expected ',' or ']'
    pub const EXPECTED_COMMA_OR_END: i32 = -103;
    /// Expected a number (digit or '-')
    pub const EXPECTED_NUMBER: i32 = -104;
    /// Number overflow (value too large for target type)
    pub const NUMBER_OVERFLOW: i32 = -105;
    /// Expected a string (opening '"')
    pub const EXPECTED_STRING: i32 = -106;
    /// Invalid escape sequence in string
    pub const INVALID_ESCAPE: i32 = -107;
    /// Invalid UTF-8 in string
    pub const INVALID_UTF8: i32 = -108;
    /// Expected '{' for object start
    pub const EXPECTED_OBJECT_START: i32 = -109;
    /// Expected ',' or '}'
    pub const EXPECTED_COMMA_OR_BRACE: i32 = -110;
    /// Expected ':' after object key
    pub const EXPECTED_COLON: i32 = -111;
    /// Unsupported operation
    pub const UNSUPPORTED: i32 = -1;
}

// =============================================================================
// Helper Functions
// =============================================================================

/// Skip whitespace in JSON input.
/// Returns the new position after skipping whitespace.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn json_jit_skip_ws(input: *const u8, len: usize, pos: usize) -> usize {
    let mut p = pos;
    while p < len {
        let byte = unsafe { *input.add(p) };
        if byte == b' ' || byte == b'\t' || byte == b'\n' || byte == b'\r' {
            p += 1;
        } else {
            break;
        }
    }
    p
}

/// Parse the start of a JSON array ('[').
/// Returns: (new_pos, error_code). error_code is 0 on success.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn json_jit_seq_begin(
    input: *const u8,
    len: usize,
    pos: usize,
) -> JsonJitPosError {
    // Skip whitespace
    let pos = unsafe { json_jit_skip_ws(input, len, pos) };

    if pos >= len {
        return JsonJitPosError {
            new_pos: pos,
            error: error::UNEXPECTED_EOF,
        };
    }

    let byte = unsafe { *input.add(pos) };
    if byte != b'[' {
        return JsonJitPosError {
            new_pos: pos,
            error: error::EXPECTED_ARRAY_START,
        };
    }

    // Skip whitespace after '['
    let pos = unsafe { json_jit_skip_ws(input, len, pos + 1) };
    JsonJitPosError {
        new_pos: pos,
        error: 0,
    }
}

/// Check if at end of JSON array (']').
/// Returns: (packed_pos_end, error_code) where packed_pos_end = (is_end << 63) | new_pos.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn json_jit_seq_is_end(
    input: *const u8,
    len: usize,
    pos: usize,
) -> JsonJitPosEndError {
    jit_debug!("[json_jit_seq_is_end] pos={}, len={}", pos, len);
    if pos >= len {
        jit_debug!("[json_jit_seq_is_end] EOF!");
        return JsonJitPosEndError::new(pos, false, error::UNEXPECTED_EOF);
    }

    let byte = unsafe { *input.add(pos) };
    jit_debug!("[json_jit_seq_is_end] byte='{}' ({})", byte as char, byte);
    if byte == b']' {
        // Skip whitespace after ']'
        let pos = unsafe { json_jit_skip_ws(input, len, pos + 1) };
        jit_debug!("[json_jit_seq_is_end] -> is_end=true, new_pos={}", pos);
        JsonJitPosEndError::new(pos, true, 0)
    } else {
        jit_debug!("[json_jit_seq_is_end] -> is_end=false, new_pos={}", pos);
        JsonJitPosEndError::new(pos, false, 0)
    }
}

/// Handle separator after element in JSON array.
/// Returns: (new_pos, error_code).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn json_jit_seq_next(
    input: *const u8,
    len: usize,
    pos: usize,
) -> JsonJitPosError {
    // Skip whitespace
    let pos = unsafe { json_jit_skip_ws(input, len, pos) };

    if pos >= len {
        return JsonJitPosError {
            new_pos: pos,
            error: error::UNEXPECTED_EOF,
        };
    }

    let byte = unsafe { *input.add(pos) };
    if byte == b',' {
        // Skip whitespace after comma
        let pos = unsafe { json_jit_skip_ws(input, len, pos + 1) };
        JsonJitPosError {
            new_pos: pos,
            error: 0,
        }
    } else if byte == b']' {
        // Don't consume, let seq_is_end handle it
        JsonJitPosError {
            new_pos: pos,
            error: 0,
        }
    } else {
        JsonJitPosError {
            new_pos: pos,
            error: error::EXPECTED_COMMA_OR_END,
        }
    }
}

/// Parse a JSON boolean.
/// Returns: (packed_pos_value, error_code) where packed_pos_value = (value << 63) | new_pos.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn json_jit_parse_bool(
    input: *const u8,
    len: usize,
    pos: usize,
) -> JsonJitPosValueError {
    // Skip whitespace
    let pos = unsafe { json_jit_skip_ws(input, len, pos) };

    if pos + 4 <= len {
        // Check for "true"
        let slice = unsafe { std::slice::from_raw_parts(input.add(pos), 4) };
        if slice == b"true" {
            return JsonJitPosValueError::new(pos + 4, true, 0);
        }
    }

    if pos + 5 <= len {
        // Check for "false"
        let slice = unsafe { std::slice::from_raw_parts(input.add(pos), 5) };
        if slice == b"false" {
            return JsonJitPosValueError::new(pos + 5, false, 0);
        }
    }

    JsonJitPosValueError::new(pos, false, error::EXPECTED_BOOL)
}

/// Fast i64 parser using word-at-a-time digit scanning.
///
/// Implements a fast path for 1-19 digit integers without overflow checks.
/// Uses output pointer to avoid ABI issues with struct returns.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn json_jit_parse_i64(
    out: *mut JsonJitI64Result,
    input: *const u8,
    len: usize,
    pos: usize,
) {
    if pos >= len {
        unsafe {
            *out = JsonJitI64Result {
                new_pos: pos,
                value: 0,
                error: error::UNEXPECTED_EOF,
            };
        }
        return;
    }

    let mut p = pos;
    let mut is_negative = false;

    // Check for optional minus sign
    if unsafe { *input.add(p) } == b'-' {
        is_negative = true;
        p += 1;
        if p >= len {
            unsafe {
                *out = JsonJitI64Result {
                    new_pos: pos,
                    value: 0,
                    error: error::EXPECTED_NUMBER,
                };
            }
            return;
        }
    }

    // Fast path: scan digits word-at-a-time
    let mut value: u64 = 0;
    let mut digit_count = 0;

    // Fast loop: process up to 8 digits at a time
    while p + 8 <= len && digit_count < 19 {
        let word = unsafe { (input.add(p) as *const u64).read_unaligned() };

        // Check if all 8 bytes are digits using SWAR (SIMD Within A Register)
        // A byte is a digit if it's in range ['0', '9'] (0x30-0x39)
        let less_than_zero = word.wrapping_sub(0x3030303030303030);
        let greater_than_nine = word | 0x4646464646464646; // Set bit 6 to make non-digits fail
        let is_all_digits = (less_than_zero | greater_than_nine) & 0x8080808080808080 == 0;

        if !is_all_digits {
            break;
        }

        // All 8 bytes are digits - accumulate them
        // Extract each digit: (byte - '0')
        let digits = word.wrapping_sub(0x3030303030303030);

        // Accumulate: value = value * 10^8 + extracted_number
        // We need to convert 8 packed digits into a number
        // This is complex, so fall back to byte-by-byte for now
        // TODO: Optimize with SWAR arithmetic
        for i in 0..8 {
            let digit = (digits >> (i * 8)) & 0xFF;
            value = value * 10 + digit;
            digit_count += 1;
        }
        p += 8;
    }

    // Byte-by-byte tail processing
    while p < len && digit_count < 19 {
        let byte = unsafe { *input.add(p) };
        if !byte.is_ascii_digit() {
            break;
        }
        let digit = (byte - b'0') as u64;
        value = value * 10 + digit;
        digit_count += 1;
        p += 1;
    }

    if digit_count == 0 {
        unsafe {
            *out = JsonJitI64Result {
                new_pos: pos,
                value: 0,
                error: error::EXPECTED_NUMBER,
            };
        }
        return;
    }

    // Check if there are more digits (would cause overflow)
    if p < len {
        let byte = unsafe { *input.add(p) };
        if byte.is_ascii_digit() {
            // 20+ digits - overflow
            unsafe {
                *out = JsonJitI64Result {
                    new_pos: pos,
                    value: 0,
                    error: error::NUMBER_OVERFLOW,
                };
            }
            return;
        }
    }

    // Apply sign and range check
    let signed_value = if is_negative {
        // Check if it fits in i64 range (max negative is -9223372036854775808)
        if value > 9223372036854775808u64 {
            unsafe {
                *out = JsonJitI64Result {
                    new_pos: pos,
                    value: 0,
                    error: error::NUMBER_OVERFLOW,
                };
            }
            return;
        }
        -(value as i64)
    } else {
        // Check if it fits in i64 range (max positive is 9223372036854775807)
        if value > 9223372036854775807u64 {
            unsafe {
                *out = JsonJitI64Result {
                    new_pos: pos,
                    value: 0,
                    error: error::NUMBER_OVERFLOW,
                };
            }
            return;
        }
        value as i64
    };

    unsafe {
        *out = JsonJitI64Result {
            new_pos: p,
            value: signed_value,
            error: 0,
        };
    }
}

/// Fast u64 parser using word-at-a-time digit scanning.
///
/// Implements a fast path for 1-20 digit integers without overflow checks.
/// Uses output pointer to avoid ABI issues with struct returns.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn json_jit_parse_u64(
    out: *mut JsonJitI64Result,
    input: *const u8,
    len: usize,
    pos: usize,
) {
    if pos >= len {
        unsafe {
            *out = JsonJitI64Result {
                new_pos: pos,
                value: 0,
                error: error::UNEXPECTED_EOF,
            };
        }
        return;
    }

    let mut p = pos;
    let mut value: u64 = 0;
    let mut digit_count = 0;

    // Byte-by-byte for simplicity (word-at-a-time conversion is complex)
    // Fast path: up to 19 digits without overflow check
    while p < len && digit_count < 19 {
        let byte = unsafe { *input.add(p) };
        if !byte.is_ascii_digit() {
            break;
        }
        let digit = (byte - b'0') as u64;
        value = value * 10 + digit;
        digit_count += 1;
        p += 1;
    }

    if digit_count == 0 {
        unsafe {
            *out = JsonJitI64Result {
                new_pos: pos,
                value: 0,
                error: error::EXPECTED_NUMBER,
            };
        }
        return;
    }

    // Handle 20th digit with overflow check
    if p < len {
        let byte = unsafe { *input.add(p) };
        if byte.is_ascii_digit() {
            let digit = (byte - b'0') as u64;
            // Check for overflow: u64::MAX = 18446744073709551615
            // If value > 1844674407370955161, or
            //    value == 1844674407370955161 && digit > 5
            if value > 1844674407370955161 || (value == 1844674407370955161 && digit > 5) {
                unsafe {
                    *out = JsonJitI64Result {
                        new_pos: pos,
                        value: 0,
                        error: error::NUMBER_OVERFLOW,
                    };
                }
                return;
            }
            value = value * 10 + digit;
            p += 1;

            // Check if there's a 21st digit
            if p < len {
                let byte = unsafe { *input.add(p) };
                if byte.is_ascii_digit() {
                    unsafe {
                        *out = JsonJitI64Result {
                            new_pos: pos,
                            value: 0,
                            error: error::NUMBER_OVERFLOW,
                        };
                    }
                    return;
                }
            }
        }
    }

    unsafe {
        *out = JsonJitI64Result {
            new_pos: p,
            value: value as i64,
            error: 0,
        };
    }
}

/// Return type for json_jit_parse_i64/u64.
#[repr(C)]
pub struct JsonJitI64Result {
    /// New position after parsing
    pub new_pos: usize,
    /// Parsed i64/u64 value
    pub value: i64,
    /// Error code (0 = success, negative = error)
    pub error: i32,
}

/// Return type for json_jit_parse_f64.
#[repr(C)]
pub struct JsonJitF64Result {
    /// New position after parsing
    pub new_pos: usize,
    /// Parsed f64 value
    pub value: f64,
    /// Error code (0 = success, negative = error)
    pub error: i32,
}

/// Return type for json_jit_parse_string.
#[repr(C)]
pub struct JsonJitStringResult {
    /// New position after parsing
    pub new_pos: usize,
    /// Pointer to string data (either into input or heap-allocated)
    pub ptr: *const u8,
    /// Length of string in bytes
    pub len: usize,
    /// Capacity (only meaningful if owned)
    pub cap: usize,
    /// 1 if owned (heap-allocated, needs drop), 0 if borrowed
    pub owned: u8,
    /// Error code (0 = success, negative = error)
    pub error: i32,
}

impl JsonJitStringResult {
    fn error(pos: usize, code: i32) -> Self {
        Self {
            new_pos: pos,
            ptr: std::ptr::null(),
            len: 0,
            cap: 0,
            owned: 0,
            error: code,
        }
    }

    fn borrowed(new_pos: usize, ptr: *const u8, len: usize) -> Self {
        Self {
            new_pos,
            ptr,
            len,
            cap: 0,
            owned: 0,
            error: 0,
        }
    }

    fn owned(new_pos: usize, s: String) -> Self {
        let len = s.len();
        let cap = s.capacity();
        let ptr = s.as_ptr();
        std::mem::forget(s); // Transfer ownership to caller
        Self {
            new_pos,
            ptr,
            len,
            cap,
            owned: 1,
            error: 0,
        }
    }
}

/// Parse a JSON string.
/// Handles: quotes, escape sequences (\n, \t, \\, \", \/, \b, \f, \r, \uXXXX).
/// Returns borrowed slice if no escapes, owned String if escapes present.
///
/// Uses output pointer to avoid large struct return ABI issues.
/// The scratch buffer in JitScratch is reused across string parses for escaped strings.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn json_jit_parse_string(
    out: *mut JsonJitStringResult,
    input: *const u8,
    len: usize,
    pos: usize,
    scratch: *mut JitScratch,
) {
    let result = json_jit_parse_string_impl(input, len, pos, scratch);
    unsafe { out.write(result) };
}

fn json_jit_parse_string_impl(
    input: *const u8,
    len: usize,
    pos: usize,
    scratch: *mut JitScratch,
) -> JsonJitStringResult {
    if pos >= len {
        return JsonJitStringResult::error(pos, error::UNEXPECTED_EOF);
    }

    // Expect opening quote
    let byte = unsafe { *input.add(pos) };
    if byte != b'"' {
        return JsonJitStringResult::error(pos, error::EXPECTED_STRING);
    }

    let start = pos + 1; // After opening quote

    // Fast word-at-a-time scan for " or \, with ASCII detection
    let (hit_idx, hit_byte, is_ascii) =
        match find_quote_or_backslash_with_ascii(unsafe { input.add(start) }, len - start) {
            Some(result) => result,
            None => return JsonJitStringResult::error(pos, error::UNEXPECTED_EOF),
        };

    if hit_byte == b'"' {
        // Unescaped path: found closing quote before any escape
        let string_len = hit_idx;
        let ptr = unsafe { input.add(start) };

        if is_ascii {
            // ASCII-only: no validation needed, all bytes < 0x80 are valid UTF-8
            JsonJitStringResult::borrowed(start + hit_idx + 1, ptr, string_len)
        } else {
            // Non-ASCII: validate UTF-8
            let slice = unsafe { std::slice::from_raw_parts(ptr, string_len) };
            match std::str::from_utf8(slice) {
                Ok(_) => JsonJitStringResult::borrowed(start + hit_idx + 1, ptr, string_len),
                Err(_) => JsonJitStringResult::error(pos, error::INVALID_UTF8),
            }
        }
    } else {
        // Found backslash - escaped path (uses scratch buffer for decoding)
        parse_string_with_escapes(input, len, pos, start, start + hit_idx, scratch)
    }
}

/// Fast word-at-a-time scan for quote (") or backslash (\).
/// Returns: (index_of_hit, byte_found, is_all_ascii)
fn find_quote_or_backslash_with_ascii(ptr: *const u8, len: usize) -> Option<(usize, u8, bool)> {
    const WORD_SIZE: usize = core::mem::size_of::<usize>();
    const HI_MASK: usize = usize::from_ne_bytes([0x80; WORD_SIZE]);

    let mut i = 0;
    let mut is_ascii = true;

    // Word-at-a-time scan
    while i + WORD_SIZE <= len {
        let word = unsafe { ptr.add(i).cast::<usize>().read_unaligned() };

        // Check ASCII: all bytes must have high bit clear
        is_ascii = is_ascii && (word & HI_MASK) == 0;

        // Check for quote or backslash using has_byte trick
        let quote_mask = has_byte(word, b'"');
        let backslash_mask = has_byte(word, b'\\');
        let mask = quote_mask | backslash_mask;

        if mask != 0 {
            // Found a match - determine which byte and its position
            let byte_offset = (mask.trailing_zeros() / 8) as usize;
            let byte = unsafe { *ptr.add(i + byte_offset) };
            return Some((i + byte_offset, byte, is_ascii));
        }

        i += WORD_SIZE;
    }

    // Tail loop for remaining bytes (< WORD_SIZE)
    while i < len {
        let byte = unsafe { *ptr.add(i) };
        is_ascii = is_ascii && (byte & 0x80) == 0;
        if byte == b'"' || byte == b'\\' {
            return Some((i, byte, is_ascii));
        }
        i += 1;
    }

    None
}

/// Detect if a word contains a specific byte using the "has_zero_byte" trick.
/// Returns a bitmask with 0x80 set in byte lanes that match.
#[inline(always)]
fn has_byte(word: usize, byte: u8) -> usize {
    const WORD_SIZE: usize = core::mem::size_of::<usize>();
    const LO_ONES: usize = usize::from_ne_bytes([0x01; WORD_SIZE]);
    const HI_MASK: usize = usize::from_ne_bytes([0x80; WORD_SIZE]);

    // Create pattern with byte repeated across all lanes
    let pattern = usize::from_ne_bytes([byte; WORD_SIZE]);

    // XOR converts matches to zero bytes
    let xor = word ^ pattern;

    // Classic has_zero_byte formula: ((x - 0x01010101) & ~x & 0x80808080)
    // Sets high bit in any byte lane that was zero
    (xor.wrapping_sub(LO_ONES)) & !xor & HI_MASK
}

/// Handle string parsing when escapes are detected.
/// This is split out to keep the unescaped fast path inline-friendly.
/// Uses the scratch buffer from JitScratch for decoding, reusing it across string parses.
#[inline(never)]
fn parse_string_with_escapes(
    input: *const u8,
    len: usize,
    pos: usize,
    start: usize,
    first_escape_pos: usize,
    jit_scratch: *mut JitScratch,
) -> JsonJitStringResult {
    let mut p = first_escape_pos;
    let mut or_mask: u8 = 0;

    // Accumulate or_mask for ASCII detection during escaped scan
    // We need to scan the entire string anyway to find the closing quote
    while p < len {
        let byte = unsafe { *input.add(p) };

        if byte == b'"' {
            // Found closing quote - now decode the escaped string
            let slice = unsafe { std::slice::from_raw_parts(input.add(start), p - start) };

            // Check if the string is ASCII-only for faster decoding
            let is_ascii = (or_mask & 0x80) == 0;

            // Take the scratch buffer from JitScratch (or create new one)
            let mut scratch = unsafe { take_scratch_buffer(jit_scratch, slice.len()) };
            scratch.clear();

            match decode_json_string_into(slice, is_ascii, &mut scratch) {
                Ok(()) => {
                    // Convert scratch to String - this consumes the buffer.
                    // We'll create a fresh buffer next time. The win here is that
                    // the scratch buffer's capacity is already correct for the string size,
                    // avoiding reallocations during decode.
                    let result_string = if is_ascii {
                        // SAFETY: ASCII input + our escape decoding = valid UTF-8
                        unsafe { String::from_utf8_unchecked(scratch) }
                    } else {
                        match String::from_utf8(scratch) {
                            Ok(s) => s,
                            Err(e) => {
                                // Put buffer back before returning error
                                unsafe { save_scratch_buffer(jit_scratch, e.into_bytes()) };
                                return JsonJitStringResult::error(pos, error::INVALID_UTF8);
                            }
                        }
                    };
                    // Note: we consumed scratch, so JitScratch.string_scratch_* is null.
                    // A new buffer will be allocated on next string parse.
                    return JsonJitStringResult::owned(p + 1, result_string);
                }
                Err(code) => {
                    // Put buffer back before returning error
                    unsafe { save_scratch_buffer(jit_scratch, scratch) };
                    return JsonJitStringResult::error(pos, code);
                }
            }
        } else if byte == b'\\' {
            p += 1; // Skip the backslash
            if p >= len {
                return JsonJitStringResult::error(pos, error::UNEXPECTED_EOF);
            }
            let escaped = unsafe { *input.add(p) };
            if escaped == b'u' {
                // \uXXXX - skip 4 more bytes
                p += 4;
            }
            or_mask |= byte;
            p += 1;
        } else {
            or_mask |= byte;
            p += 1;
        }
    }

    // Reached end without closing quote
    JsonJitStringResult::error(pos, error::UNEXPECTED_EOF)
}

/// Get or create a scratch buffer from JitScratch, returning raw Vec parts.
/// The caller must call `save_scratch_buffer` after using the buffer.
///
/// # Safety
/// - `jit_scratch` must be a valid pointer to a JitScratch
/// - The returned Vec must be passed to `save_scratch_buffer` before any other
///   call to `take_scratch_buffer`
unsafe fn take_scratch_buffer(jit_scratch: *mut JitScratch, capacity_hint: usize) -> Vec<u8> {
    // SAFETY: Caller guarantees jit_scratch is valid
    let scratch = unsafe { &mut *jit_scratch };

    // If we don't have a scratch buffer yet, create one
    if scratch.string_scratch_ptr.is_null() {
        return Vec::with_capacity(capacity_hint);
    }

    // Reconstruct the Vec from the raw parts and take ownership
    // SAFETY: We maintain the Vec invariants - ptr/len/cap are valid from previous Vec
    let vec = unsafe {
        Vec::from_raw_parts(
            scratch.string_scratch_ptr,
            scratch.string_scratch_len,
            scratch.string_scratch_cap,
        )
    };

    // Mark as taken
    scratch.string_scratch_ptr = std::ptr::null_mut();
    scratch.string_scratch_len = 0;
    scratch.string_scratch_cap = 0;

    vec
}

/// Save a scratch buffer back to JitScratch for reuse.
///
/// # Safety
/// - `jit_scratch` must be a valid pointer to a JitScratch
unsafe fn save_scratch_buffer(jit_scratch: *mut JitScratch, mut buf: Vec<u8>) {
    // SAFETY: Caller guarantees jit_scratch is valid
    let scratch = unsafe { &mut *jit_scratch };

    // Store the Vec parts back
    scratch.string_scratch_ptr = buf.as_mut_ptr();
    scratch.string_scratch_len = buf.len();
    scratch.string_scratch_cap = buf.capacity();

    // Forget the Vec so it doesn't deallocate
    std::mem::forget(buf);
}

/// Hex decoding lookup tables (same approach as serde_json)
/// HEX0[ch] = hex value of ch (0-15), or -1 if invalid
/// HEX1[ch] = hex value of ch shifted left by 4 bits, or -1 if invalid
static HEX0: [i16; 256] = {
    let mut table = [0i16; 256];
    let mut ch = 0usize;
    while ch < 256 {
        table[ch] = match ch as u8 {
            b'0'..=b'9' => (ch as u8 - b'0') as i16,
            b'A'..=b'F' => (ch as u8 - b'A' + 10) as i16,
            b'a'..=b'f' => (ch as u8 - b'a' + 10) as i16,
            _ => -1,
        };
        ch += 1;
    }
    table
};

static HEX1: [i16; 256] = {
    let mut table = [0i16; 256];
    let mut ch = 0usize;
    while ch < 256 {
        table[ch] = match ch as u8 {
            b'0'..=b'9' => ((ch as u8 - b'0') as i16) << 4,
            b'A'..=b'F' => ((ch as u8 - b'A' + 10) as i16) << 4,
            b'a'..=b'f' => ((ch as u8 - b'a' + 10) as i16) << 4,
            _ => -1,
        };
        ch += 1;
    }
    table
};

/// Decode four hex digits into a u16 using lookup tables.
/// Returns None if any digit is invalid.
#[inline]
fn decode_four_hex_digits(a: u8, b: u8, c: u8, d: u8) -> Option<u16> {
    let a = HEX1[a as usize] as i32;
    let b = HEX0[b as usize] as i32;
    let c = HEX1[c as usize] as i32;
    let d = HEX0[d as usize] as i32;

    let codepoint = ((a | b) << 8) | c | d;

    // A single sign bit check - if any nibble was -1, the result will be negative
    if codepoint >= 0 {
        Some(codepoint as u16)
    } else {
        None
    }
}

/// Push a UTF-8 encoded codepoint directly to a byte buffer.
/// This is more efficient than String::push(char) as it avoids
/// char-to-UTF8 encoding overhead.
#[inline]
fn push_utf8_codepoint(n: u32, scratch: &mut Vec<u8>) {
    if n < 0x80 {
        scratch.push(n as u8);
        return;
    }

    scratch.reserve(4);

    // SAFETY: After reserve, scratch has at least 4 bytes available.
    // We write encoded_len bytes and update length accordingly.
    unsafe {
        let ptr = scratch.as_mut_ptr().add(scratch.len());

        let encoded_len = match n {
            0..=0x7F => unreachable!(),
            0x80..=0x7FF => {
                ptr.write(((n >> 6) & 0b0001_1111) as u8 | 0b1100_0000);
                ptr.add(1).write((n & 0b0011_1111) as u8 | 0b1000_0000);
                2
            }
            0x800..=0xFFFF => {
                ptr.write(((n >> 12) & 0b0000_1111) as u8 | 0b1110_0000);
                ptr.add(1)
                    .write(((n >> 6) & 0b0011_1111) as u8 | 0b1000_0000);
                ptr.add(2).write((n & 0b0011_1111) as u8 | 0b1000_0000);
                3
            }
            0x1_0000..=0x10_FFFF => {
                ptr.write(((n >> 18) & 0b0000_0111) as u8 | 0b1111_0000);
                ptr.add(1)
                    .write(((n >> 12) & 0b0011_1111) as u8 | 0b1000_0000);
                ptr.add(2)
                    .write(((n >> 6) & 0b0011_1111) as u8 | 0b1000_0000);
                ptr.add(3).write((n & 0b0011_1111) as u8 | 0b1000_0000);
                4
            }
            _ => return, // Invalid codepoint, don't write anything
        };

        scratch.set_len(scratch.len() + encoded_len);
    }
}

/// Decode a JSON string with escape sequences into the provided scratch buffer.
/// `is_ascii`: hint that all bytes are ASCII (< 0x80), allowing faster processing
/// `scratch`: pre-allocated buffer to decode into (should be cleared by caller)
///
/// Uses serde_json-style optimizations:
/// - Direct byte writes to scratch buffer
/// - Lookup tables for hex decoding
/// - Bulk copies for unescaped regions
///
/// Returns Ok(()) on success (data is in scratch buffer), Err(code) on failure.
fn decode_json_string_into(
    slice: &[u8],
    _is_ascii: bool,
    scratch: &mut Vec<u8>,
) -> Result<(), i32> {
    let mut i = 0;

    while i < slice.len() {
        let byte = slice[i];

        if byte == b'\\' {
            i += 1;
            if i >= slice.len() {
                return Err(error::INVALID_ESCAPE);
            }
            let escaped = slice[i];
            match escaped {
                b'"' => scratch.push(b'"'),
                b'\\' => scratch.push(b'\\'),
                b'/' => scratch.push(b'/'),
                b'b' => scratch.push(b'\x08'),
                b'f' => scratch.push(b'\x0C'),
                b'n' => scratch.push(b'\n'),
                b'r' => scratch.push(b'\r'),
                b't' => scratch.push(b'\t'),
                b'u' => {
                    // \uXXXX
                    if i + 4 >= slice.len() {
                        return Err(error::INVALID_ESCAPE);
                    }
                    let code_point = match decode_four_hex_digits(
                        slice[i + 1],
                        slice[i + 2],
                        slice[i + 3],
                        slice[i + 4],
                    ) {
                        Some(n) => n,
                        None => return Err(error::INVALID_ESCAPE),
                    };
                    // Handle surrogate pairs
                    if (0xD800..=0xDBFF).contains(&code_point) {
                        // High surrogate - look for low surrogate
                        if i + 10 < slice.len() && slice[i + 5] == b'\\' && slice[i + 6] == b'u' {
                            if let Some(low_point) = decode_four_hex_digits(
                                slice[i + 7],
                                slice[i + 8],
                                slice[i + 9],
                                slice[i + 10],
                            ) {
                                if (0xDC00..=0xDFFF).contains(&low_point) {
                                    // Valid surrogate pair
                                    let full = 0x10000
                                        + ((code_point as u32 - 0xD800) << 10)
                                        + (low_point as u32 - 0xDC00);
                                    push_utf8_codepoint(full, scratch);
                                    i += 11; // Skip both \uXXXX sequences
                                    continue;
                                }
                            }
                        }
                        return Err(error::INVALID_ESCAPE);
                    } else {
                        push_utf8_codepoint(code_point as u32, scratch);
                    }
                    i += 4; // Skip the 4 hex digits
                }
                _ => return Err(error::INVALID_ESCAPE),
            }
            i += 1;
        } else {
            // Fast path for non-escape sequences
            // Find the next escape or end of string
            let chunk_start = i;
            while i < slice.len() && slice[i] != b'\\' {
                i += 1;
            }
            // Bulk copy the unescaped region
            scratch.extend_from_slice(&slice[chunk_start..i]);
        }
    }

    Ok(())
}

/// Parse a JSON floating-point number (output pointer version).
/// Handles: optional sign, integer part, optional decimal, optional exponent.
/// Writes result to output pointer to avoid ABI issues with f64 returns.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn json_jit_parse_f64_out(
    out: *mut JsonJitF64Result,
    input: *const u8,
    len: usize,
    pos: usize,
) {
    let result = json_jit_parse_f64_impl(input, len, pos);
    unsafe { *out = result };
}

/// Parse a JSON floating-point number.
/// Handles: optional sign, integer part, optional decimal, optional exponent.
/// Returns: (new_pos, value, error_code).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn json_jit_parse_f64(
    input: *const u8,
    len: usize,
    pos: usize,
) -> JsonJitF64Result {
    json_jit_parse_f64_impl(input, len, pos)
}

/// Negative powers of 10 for fast decimal parsing.
/// POW10_NEG\[k\] = 10^(-k) for k=0..=19
static POW10_NEG: [f64; 20] = [
    1e0, 1e-1, 1e-2, 1e-3, 1e-4, 1e-5, 1e-6, 1e-7, 1e-8, 1e-9, 1e-10, 1e-11, 1e-12, 1e-13, 1e-14,
    1e-15, 1e-16, 1e-17, 1e-18, 1e-19,
];

/// Internal implementation of f64 parsing with simple decimal fast path.
fn json_jit_parse_f64_impl(input: *const u8, len: usize, pos: usize) -> JsonJitF64Result {
    let mut p = pos;
    let start = p;

    // Check for optional minus sign
    let is_negative = if p < len && unsafe { *input.add(p) } == b'-' {
        p += 1;
        true
    } else {
        false
    };

    // Parse integer part (up to 19 digits for fast path)
    let mut int_part: u64 = 0;
    let mut int_digits = 0;
    while p < len && int_digits < 19 {
        let byte = unsafe { *input.add(p) };
        if byte.is_ascii_digit() {
            let digit = (byte - b'0') as u64;
            int_part = int_part * 10 + digit;
            int_digits += 1;
            p += 1;
        } else {
            break;
        }
    }

    // Check if we need to fallback (more than 19 integer digits)
    if p < len {
        let byte = unsafe { *input.add(p) };
        if byte.is_ascii_digit() {
            // 20+ integer digits - fall back to slow path
            return json_jit_parse_f64_slow(input, len, start);
        }
    }

    // Parse optional fractional part
    let mut frac_part: u64 = 0;
    let mut frac_digits = 0;
    if p < len && unsafe { *input.add(p) } == b'.' {
        p += 1;
        // Parse up to 19 fractional digits
        while p < len && frac_digits < 19 {
            let byte = unsafe { *input.add(p) };
            if byte.is_ascii_digit() {
                let digit = (byte - b'0') as u64;
                frac_part = frac_part * 10 + digit;
                frac_digits += 1;
                p += 1;
            } else {
                break;
            }
        }
        // Skip remaining fractional digits (truncate, don't round for simplicity)
        while p < len {
            let byte = unsafe { *input.add(p) };
            if byte.is_ascii_digit() {
                p += 1;
            } else {
                break;
            }
        }
    }

    // Check for exponent - fall back to slow path
    if p < len {
        let byte = unsafe { *input.add(p) };
        if byte == b'e' || byte == b'E' {
            return json_jit_parse_f64_slow(input, len, start);
        }
    }

    // Error: no digits found
    if int_digits == 0 && frac_digits == 0 {
        return JsonJitF64Result {
            new_pos: pos,
            value: 0.0,
            error: error::EXPECTED_NUMBER,
        };
    }

    // Fast path: compute f64 value
    let mut value = int_part as f64;
    if frac_digits > 0 {
        value += (frac_part as f64) * POW10_NEG[frac_digits];
    }
    if is_negative {
        value = -value;
    }

    JsonJitF64Result {
        new_pos: p,
        value,
        error: 0,
    }
}

/// Slow path fallback using stdlib parse for complex numbers.
fn json_jit_parse_f64_slow(input: *const u8, len: usize, start: usize) -> JsonJitF64Result {
    let mut p = start;
    let mut has_digit = false;

    // Optional minus sign
    if p < len && unsafe { *input.add(p) } == b'-' {
        p += 1;
    }

    // Integer part
    while p < len {
        let byte = unsafe { *input.add(p) };
        if byte.is_ascii_digit() {
            has_digit = true;
            p += 1;
        } else {
            break;
        }
    }

    // Optional decimal part
    if p < len && unsafe { *input.add(p) } == b'.' {
        p += 1;
        while p < len {
            let byte = unsafe { *input.add(p) };
            if byte.is_ascii_digit() {
                has_digit = true;
                p += 1;
            } else {
                break;
            }
        }
    }

    // Optional exponent
    if p < len {
        let byte = unsafe { *input.add(p) };
        if byte == b'e' || byte == b'E' {
            p += 1;
            // Optional sign
            if p < len {
                let sign_byte = unsafe { *input.add(p) };
                if sign_byte == b'+' || sign_byte == b'-' {
                    p += 1;
                }
            }
            // Exponent digits
            while p < len {
                let byte = unsafe { *input.add(p) };
                if byte.is_ascii_digit() {
                    p += 1;
                } else {
                    break;
                }
            }
        }
    }

    if !has_digit {
        return JsonJitF64Result {
            new_pos: start,
            value: 0.0,
            error: error::EXPECTED_NUMBER,
        };
    }

    // Parse the slice as f64
    let slice = unsafe { std::slice::from_raw_parts(input.add(start), p - start) };
    let s = match std::str::from_utf8(slice) {
        Ok(s) => s,
        Err(_) => {
            return JsonJitF64Result {
                new_pos: start,
                value: 0.0,
                error: error::EXPECTED_NUMBER,
            };
        }
    };

    match s.parse::<f64>() {
        Ok(value) => JsonJitF64Result {
            new_pos: p,
            value,
            error: 0,
        },
        Err(_) => JsonJitF64Result {
            new_pos: start,
            value: 0.0,
            error: error::NUMBER_OVERFLOW,
        },
    }
}

/// Skip a JSON value (scalar, string, array, or object).
/// Returns: new_pos on success (>= 0), error code on failure (< 0).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn json_jit_skip_value(
    input: *const u8,
    len: usize,
    pos: usize,
) -> JsonJitResult {
    // Skip leading whitespace
    let pos = unsafe { json_jit_skip_ws(input, len, pos) };

    if pos >= len {
        return error::UNEXPECTED_EOF as isize;
    }

    let byte = unsafe { *input.add(pos) };

    let result = match byte {
        // String
        b'"' => skip_string(input, len, pos),
        // Array
        b'[' => skip_array(input, len, pos),
        // Object
        b'{' => skip_object(input, len, pos),
        // Number (digit or minus)
        b'-' | b'0'..=b'9' => skip_number(input, len, pos),
        // true
        b't' => skip_literal(input, len, pos, b"true"),
        // false
        b'f' => skip_literal(input, len, pos, b"false"),
        // null
        b'n' => skip_literal(input, len, pos, b"null"),
        _ => JsonJitPosError {
            new_pos: pos,
            error: error::UNEXPECTED_EOF, // Generic error for unexpected byte
        },
    };
    result.into_result()
}

fn skip_string(input: *const u8, len: usize, pos: usize) -> JsonJitPosError {
    // Expect opening quote
    if pos >= len || unsafe { *input.add(pos) } != b'"' {
        return JsonJitPosError {
            new_pos: pos,
            error: error::EXPECTED_STRING,
        };
    }

    let start = pos + 1;

    // Fast skip using word-at-a-time scanner (no ASCII detection needed for skipping)
    match fast_skip_to_quote(unsafe { input.add(start) }, len - start) {
        Some(quote_idx) => JsonJitPosError {
            new_pos: start + quote_idx + 1, // +1 to skip past the closing quote
            error: 0,
        },
        None => JsonJitPosError {
            new_pos: pos,
            error: error::UNEXPECTED_EOF,
        },
    }
}

/// Fast skip to closing quote, handling escapes.
/// Returns the index of the closing quote relative to ptr.
fn fast_skip_to_quote(ptr: *const u8, len: usize) -> Option<usize> {
    const WORD_SIZE: usize = core::mem::size_of::<usize>();

    let mut i = 0;

    // Word-at-a-time scan for " or \
    while i + WORD_SIZE <= len {
        let word = unsafe { ptr.add(i).cast::<usize>().read_unaligned() };

        let quote_mask = has_byte(word, b'"');
        let backslash_mask = has_byte(word, b'\\');
        let mask = quote_mask | backslash_mask;

        if mask != 0 {
            // Found a match - check what it was
            let byte_offset = (mask.trailing_zeros() / 8) as usize;
            let byte = unsafe { *ptr.add(i + byte_offset) };

            if byte == b'"' {
                // Found closing quote
                return Some(i + byte_offset);
            } else {
                // Found escape - skip it
                i += byte_offset + 1; // Move past backslash
                if i >= len {
                    return None;
                }
                let escaped = unsafe { *ptr.add(i) };
                if escaped == b'u' {
                    // \uXXXX - skip 4 more bytes
                    i += 5; // +1 for 'u', +4 for hex digits
                } else {
                    i += 1; // Skip the escaped character
                }
                continue;
            }
        }

        i += WORD_SIZE;
    }

    // Tail loop for remaining bytes
    while i < len {
        let byte = unsafe { *ptr.add(i) };
        if byte == b'"' {
            return Some(i);
        } else if byte == b'\\' {
            i += 1;
            if i >= len {
                return None;
            }
            let escaped = unsafe { *ptr.add(i) };
            if escaped == b'u' {
                i += 5; // +1 already done, +4 more
            } else {
                i += 1;
            }
        } else {
            i += 1;
        }
    }

    None
}

fn skip_number(input: *const u8, len: usize, pos: usize) -> JsonJitPosError {
    let mut p = pos;

    // Optional minus
    if p < len && unsafe { *input.add(p) } == b'-' {
        p += 1;
    }

    // Integer part
    while p < len {
        let byte = unsafe { *input.add(p) };
        if byte.is_ascii_digit() {
            p += 1;
        } else {
            break;
        }
    }

    // Optional decimal part
    if p < len && unsafe { *input.add(p) } == b'.' {
        p += 1;
        while p < len {
            let byte = unsafe { *input.add(p) };
            if byte.is_ascii_digit() {
                p += 1;
            } else {
                break;
            }
        }
    }

    // Optional exponent
    if p < len {
        let byte = unsafe { *input.add(p) };
        if byte == b'e' || byte == b'E' {
            p += 1;
            if p < len {
                let sign = unsafe { *input.add(p) };
                if sign == b'+' || sign == b'-' {
                    p += 1;
                }
            }
            while p < len {
                let byte = unsafe { *input.add(p) };
                if byte.is_ascii_digit() {
                    p += 1;
                } else {
                    break;
                }
            }
        }
    }

    if p == pos {
        return JsonJitPosError {
            new_pos: pos,
            error: error::EXPECTED_NUMBER,
        };
    }

    JsonJitPosError {
        new_pos: p,
        error: 0,
    }
}

fn skip_literal(input: *const u8, len: usize, pos: usize, literal: &[u8]) -> JsonJitPosError {
    if pos + literal.len() > len {
        return JsonJitPosError {
            new_pos: pos,
            error: error::UNEXPECTED_EOF,
        };
    }

    let slice = unsafe { std::slice::from_raw_parts(input.add(pos), literal.len()) };
    if slice == literal {
        JsonJitPosError {
            new_pos: pos + literal.len(),
            error: 0,
        }
    } else {
        JsonJitPosError {
            new_pos: pos,
            error: error::EXPECTED_BOOL, // Generic mismatch
        }
    }
}

fn skip_array(input: *const u8, len: usize, pos: usize) -> JsonJitPosError {
    // Expect opening bracket
    if pos >= len || unsafe { *input.add(pos) } != b'[' {
        return JsonJitPosError {
            new_pos: pos,
            error: error::EXPECTED_ARRAY_START,
        };
    }

    let mut p = pos + 1;

    // Skip whitespace
    p = unsafe { json_jit_skip_ws(input, len, p) };

    // Check for empty array
    if p < len && unsafe { *input.add(p) } == b']' {
        return JsonJitPosError {
            new_pos: p + 1,
            error: 0,
        };
    }

    // Skip elements
    loop {
        // Skip value
        let result = unsafe { json_jit_skip_value(input, len, p) };
        if result < 0 {
            return JsonJitPosError {
                new_pos: p,
                error: result as i32,
            };
        }
        p = result as usize;

        // Skip whitespace
        p = unsafe { json_jit_skip_ws(input, len, p) };

        if p >= len {
            return JsonJitPosError {
                new_pos: p,
                error: error::UNEXPECTED_EOF,
            };
        }

        let byte = unsafe { *input.add(p) };
        if byte == b']' {
            return JsonJitPosError {
                new_pos: p + 1,
                error: 0,
            };
        } else if byte == b',' {
            p += 1;
            // Skip whitespace after comma
            p = unsafe { json_jit_skip_ws(input, len, p) };
        } else {
            return JsonJitPosError {
                new_pos: p,
                error: error::EXPECTED_COMMA_OR_END,
            };
        }
    }
}

fn skip_object(input: *const u8, len: usize, pos: usize) -> JsonJitPosError {
    // Expect opening brace
    if pos >= len || unsafe { *input.add(pos) } != b'{' {
        return JsonJitPosError {
            new_pos: pos,
            error: error::EXPECTED_OBJECT_START,
        };
    }

    let mut p = pos + 1;

    // Skip whitespace
    p = unsafe { json_jit_skip_ws(input, len, p) };

    // Check for empty object
    if p < len && unsafe { *input.add(p) } == b'}' {
        return JsonJitPosError {
            new_pos: p + 1,
            error: 0,
        };
    }

    // Skip entries
    loop {
        // Skip key (string)
        let result = skip_string(input, len, p);
        if result.error != 0 {
            return result;
        }
        p = result.new_pos;

        // Skip whitespace
        p = unsafe { json_jit_skip_ws(input, len, p) };

        // Expect colon
        if p >= len || unsafe { *input.add(p) } != b':' {
            return JsonJitPosError {
                new_pos: p,
                error: error::EXPECTED_COLON,
            };
        }
        p += 1;

        // Skip whitespace
        p = unsafe { json_jit_skip_ws(input, len, p) };

        // Skip value
        let result = unsafe { json_jit_skip_value(input, len, p) };
        if result < 0 {
            return JsonJitPosError {
                new_pos: p,
                error: result as i32,
            };
        }
        p = result as usize;

        // Skip whitespace
        p = unsafe { json_jit_skip_ws(input, len, p) };

        if p >= len {
            return JsonJitPosError {
                new_pos: p,
                error: error::UNEXPECTED_EOF,
            };
        }

        let byte = unsafe { *input.add(p) };
        if byte == b'}' {
            return JsonJitPosError {
                new_pos: p + 1,
                error: 0,
            };
        } else if byte == b',' {
            p += 1;
            // Skip whitespace after comma
            p = unsafe { json_jit_skip_ws(input, len, p) };
        } else {
            return JsonJitPosError {
                new_pos: p,
                error: error::EXPECTED_COMMA_OR_BRACE,
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_json_jit_parse_bool() {
        let input = b"true";
        let result = unsafe { json_jit_parse_bool(input.as_ptr(), input.len(), 0) };
        assert_eq!(result.error, 0);
        assert_eq!(result.new_pos(), 4);
        assert!(result.value());

        let input = b"false";
        let result = unsafe { json_jit_parse_bool(input.as_ptr(), input.len(), 0) };
        assert_eq!(result.error, 0);
        assert_eq!(result.new_pos(), 5);
        assert!(!result.value());
    }

    #[test]
    fn test_json_jit_seq_begin() {
        let input = b"[true]";
        let result = unsafe { json_jit_seq_begin(input.as_ptr(), input.len(), 0) };
        assert_eq!(result.error, 0);
        assert_eq!(result.new_pos, 1); // After '[', at 'true'
    }

    #[test]
    fn test_json_jit_seq_is_end() {
        let input = b"]";
        let result = unsafe { json_jit_seq_is_end(input.as_ptr(), input.len(), 0) };
        assert_eq!(result.error, 0);
        assert!(result.is_end());

        let input = b"true";
        let result = unsafe { json_jit_seq_is_end(input.as_ptr(), input.len(), 0) };
        assert_eq!(result.error, 0);
        assert!(!result.is_end());
    }
}
