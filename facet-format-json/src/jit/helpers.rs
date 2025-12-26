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
    /// Control character in string (bytes < 0x20 must be escaped)
    pub const CONTROL_CHAR_IN_STRING: i32 = -112;
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

/// Fast scan for quote ("), backslash (\), or control chars using SWAR.
/// Returns: (index_of_hit, byte_found, is_all_ascii_before_hit)
///
/// Uses Mycroft's algorithm for word-at-a-time scanning, adapted from serde_json.
/// This is faster than memchr2 for our use case because:
/// 1. We need to check for control chars anyway (invalid in JSON strings)
/// 2. We can track ASCII status during the scan (no separate pass)
/// 3. Avoids function call overhead
#[inline(always)]
fn find_quote_or_backslash_with_ascii(ptr: *const u8, len: usize) -> Option<(usize, u8, bool)> {
    let slice = unsafe { std::slice::from_raw_parts(ptr, len) };

    // SWAR constants
    type Chunk = usize;
    const STEP: usize = core::mem::size_of::<Chunk>();
    const ONE_BYTES: Chunk = Chunk::MAX / 255; // 0x0101...01
    const HIGH_BITS: Chunk = ONE_BYTES << 7; // 0x8080...80

    let mut i = 0;
    let mut has_non_ascii = false;

    // Process word-at-a-time
    while i + STEP <= len {
        // SAFETY: we checked bounds above
        let chunk = unsafe { ptr.add(i).cast::<Chunk>().read_unaligned() };

        // Check for non-ASCII (any byte with high bit set)
        if (chunk & HIGH_BITS) != 0 {
            has_non_ascii = true;
        }

        // Mycroft's algorithm: detect special bytes in parallel
        // Control chars: bytes < 0x20
        let contains_ctrl = chunk.wrapping_sub(ONE_BYTES * 0x20) & !chunk & HIGH_BITS;

        // Quote: bytes == '"' (0x22)
        let chars_quote = chunk ^ (ONE_BYTES * (b'"' as Chunk));
        let contains_quote = chars_quote.wrapping_sub(ONE_BYTES) & !chars_quote & HIGH_BITS;

        // Backslash: bytes == '\\' (0x5C)
        let chars_backslash = chunk ^ (ONE_BYTES * (b'\\' as Chunk));
        let contains_backslash =
            chars_backslash.wrapping_sub(ONE_BYTES) & !chars_backslash & HIGH_BITS;

        let masked = contains_ctrl | contains_quote | contains_backslash;
        if masked != 0 {
            // Found a special byte - figure out which one and where
            let byte_idx = if cfg!(target_endian = "little") {
                masked.trailing_zeros() as usize / 8
            } else {
                masked.leading_zeros() as usize / 8
            };
            let hit_idx = i + byte_idx;
            let hit_byte = slice[hit_idx];
            return Some((hit_idx, hit_byte, !has_non_ascii));
        }

        i += STEP;
    }

    // Process remaining bytes one at a time
    while i < len {
        let b = slice[i];
        if b & 0x80 != 0 {
            has_non_ascii = true;
        }
        if b == b'"' || b == b'\\' || b < 0x20 {
            return Some((i, b, !has_non_ascii));
        }
        i += 1;
    }

    // No special byte found
    None
}

/// Check if a byte slice is all ASCII using word-at-a-time scanning.
#[inline]
fn is_ascii_swar(slice: &[u8]) -> bool {
    const WORD_SIZE: usize = core::mem::size_of::<usize>();
    const HI_MASK: usize = usize::from_ne_bytes([0x80; WORD_SIZE]);

    let ptr = slice.as_ptr();
    let len = slice.len();
    let mut i = 0;

    // Word-at-a-time check
    while i + WORD_SIZE <= len {
        let word = unsafe { ptr.add(i).cast::<usize>().read_unaligned() };
        if (word & HI_MASK) != 0 {
            return false;
        }
        i += WORD_SIZE;
    }

    // Check remaining bytes
    while i < len {
        if slice[i] & 0x80 != 0 {
            return false;
        }
        i += 1;
    }

    true
}

/// Handle string parsing when escapes are detected.
/// This is split out to keep the unescaped fast path inline-friendly.
/// Uses the scratch buffer from JitScratch for decoding, reusing it across string parses.
///
/// Single-pass approach: copies literal spans and decodes escapes in one pass using memchr2
/// to find the next quote or backslash, avoiding a separate scanning phase.
///
/// Optimizations adapted from serde_json (MIT/Apache-2.0, Copyright David Tolnay):
/// - Track high-bit during copy to detect ASCII-only strings
/// - For ASCII: skip UTF-8 validation, convert Vec to String directly
/// - For non-ASCII: validate UTF-8, convert to String without extra copy
/// - Lookup table for hex digit decoding (see `decode_four_hex_digits`)
#[inline(never)]
fn parse_string_with_escapes(
    input: *const u8,
    len: usize,
    pos: usize,
    start: usize,
    first_escape_pos: usize,
    jit_scratch: *mut JitScratch,
) -> JsonJitStringResult {
    // Take the scratch buffer from JitScratch (or create new one)
    let capacity_hint = len - start;
    let mut scratch = unsafe { take_scratch_buffer(jit_scratch, capacity_hint) };
    scratch.clear();

    // Copy the literal prefix (bytes before first escape)
    // Track high-bit to detect ASCII-only strings (avoids UTF-8 validation)
    let prefix_len = first_escape_pos - start;
    let mut has_non_ascii = false;
    if prefix_len > 0 {
        let prefix = unsafe { std::slice::from_raw_parts(input.add(start), prefix_len) };
        has_non_ascii = !is_ascii_swar(prefix);
        scratch.extend_from_slice(prefix);
    }

    // Now decode the escape at first_escape_pos
    let mut p = first_escape_pos;

    loop {
        // We're at a backslash - decode the escape
        debug_assert!(p < len && unsafe { *input.add(p) } == b'\\');
        p += 1; // Skip backslash

        if p >= len {
            unsafe { save_scratch_buffer(jit_scratch, scratch) };
            return JsonJitStringResult::error(pos, error::UNEXPECTED_EOF);
        }

        let escaped = unsafe { *input.add(p) };
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
                // \uXXXX - may produce non-ASCII
                if p + 4 >= len {
                    unsafe { save_scratch_buffer(jit_scratch, scratch) };
                    return JsonJitStringResult::error(pos, error::INVALID_ESCAPE);
                }
                let slice = unsafe { std::slice::from_raw_parts(input.add(p + 1), 4) };
                let code_point =
                    match decode_four_hex_digits(slice[0], slice[1], slice[2], slice[3]) {
                        Some(n) => n,
                        None => {
                            unsafe { save_scratch_buffer(jit_scratch, scratch) };
                            return JsonJitStringResult::error(pos, error::INVALID_ESCAPE);
                        }
                    };

                // Handle surrogate pairs
                if (0xD800..=0xDBFF).contains(&code_point) {
                    // High surrogate - look for low surrogate
                    if p + 10 < len {
                        let maybe_low = unsafe { std::slice::from_raw_parts(input.add(p + 5), 6) };
                        if maybe_low[0] == b'\\'
                            && maybe_low[1] == b'u'
                            && let Some(low_point) = decode_four_hex_digits(
                                maybe_low[2],
                                maybe_low[3],
                                maybe_low[4],
                                maybe_low[5],
                            )
                            && (0xDC00..=0xDFFF).contains(&low_point)
                        {
                            // Valid surrogate pair - always non-ASCII (>= U+10000)
                            has_non_ascii = true;
                            let full = 0x10000
                                + ((code_point as u32 - 0xD800) << 10)
                                + (low_point as u32 - 0xDC00);
                            push_utf8_codepoint(full, &mut scratch);
                            p += 10; // Skip \uXXXX\uXXXX (we'll add 1 more below)
                        } else {
                            unsafe { save_scratch_buffer(jit_scratch, scratch) };
                            return JsonJitStringResult::error(pos, error::INVALID_ESCAPE);
                        }
                    } else {
                        unsafe { save_scratch_buffer(jit_scratch, scratch) };
                        return JsonJitStringResult::error(pos, error::INVALID_ESCAPE);
                    }
                } else {
                    // Check if escape produces non-ASCII (code point >= 0x80)
                    if code_point >= 0x80 {
                        has_non_ascii = true;
                    }
                    push_utf8_codepoint(code_point as u32, &mut scratch);
                    p += 4; // Skip the 4 hex digits (we'll add 1 more below)
                }
            }
            _ => {
                unsafe { save_scratch_buffer(jit_scratch, scratch) };
                return JsonJitStringResult::error(pos, error::INVALID_ESCAPE);
            }
        }
        p += 1; // Move past the escaped character

        // Find next quote or backslash using inline SWAR scanning
        if p >= len {
            unsafe { save_scratch_buffer(jit_scratch, scratch) };
            return JsonJitStringResult::error(pos, error::UNEXPECTED_EOF);
        }

        match find_special_byte_with_ascii(unsafe { input.add(p) }, len - p) {
            Some((idx, hit_byte, is_ascii)) => {
                // Copy literal bytes before the hit
                if idx > 0 {
                    let literal = unsafe { std::slice::from_raw_parts(input.add(p), idx) };
                    scratch.extend_from_slice(literal);
                }
                if !is_ascii {
                    has_non_ascii = true;
                }
                p += idx;

                if hit_byte == b'"' {
                    // Found closing quote - we're done
                    // Validate/convert and copy to String, keeping scratch buffer for reuse
                    let result_string = if has_non_ascii {
                        // Non-ASCII: validate UTF-8
                        match std::str::from_utf8(&scratch) {
                            Ok(s) => s.to_owned(),
                            Err(_) => {
                                unsafe { save_scratch_buffer(jit_scratch, scratch) };
                                return JsonJitStringResult::error(pos, error::INVALID_UTF8);
                            }
                        }
                    } else {
                        // ASCII-only: skip validation, all bytes < 0x80 are valid UTF-8
                        // SAFETY: SWAR verified all bytes have high bit clear
                        unsafe { std::str::from_utf8_unchecked(&scratch) }.to_owned()
                    };
                    // Clear and save scratch buffer for reuse (keeps allocation)
                    scratch.clear();
                    unsafe { save_scratch_buffer(jit_scratch, scratch) };
                    return JsonJitStringResult::owned(p + 1, result_string);
                } else if hit_byte == b'\\' {
                    // hit_byte == b'\\', loop continues to decode next escape
                } else {
                    // Control character - invalid in JSON string
                    unsafe { save_scratch_buffer(jit_scratch, scratch) };
                    return JsonJitStringResult::error(pos, error::CONTROL_CHAR_IN_STRING);
                }
            }
            None => {
                // No quote or backslash found - unterminated string
                unsafe { save_scratch_buffer(jit_scratch, scratch) };
                return JsonJitStringResult::error(pos, error::UNEXPECTED_EOF);
            }
        }
    }
}

/// Fast scan for quote ("), backslash (\), or control chars using SWAR.
/// Returns: (index_of_hit, byte_found, is_all_ascii_before_hit)
///
/// Inlined version for the escape decoding loop.
#[inline(always)]
fn find_special_byte_with_ascii(ptr: *const u8, len: usize) -> Option<(usize, u8, bool)> {
    // SWAR constants
    type Chunk = usize;
    const STEP: usize = core::mem::size_of::<Chunk>();
    const ONE_BYTES: Chunk = Chunk::MAX / 255; // 0x0101...01
    const HIGH_BITS: Chunk = ONE_BYTES << 7; // 0x8080...80

    let slice = unsafe { std::slice::from_raw_parts(ptr, len) };
    let mut i = 0;
    let mut has_non_ascii = false;

    // Process word-at-a-time
    while i + STEP <= len {
        // SAFETY: we checked bounds above
        let chunk = unsafe { ptr.add(i).cast::<Chunk>().read_unaligned() };

        // Check for non-ASCII (any byte with high bit set)
        if (chunk & HIGH_BITS) != 0 {
            has_non_ascii = true;
        }

        // Mycroft's algorithm: detect special bytes in parallel
        // Control chars: bytes < 0x20
        let contains_ctrl = chunk.wrapping_sub(ONE_BYTES * 0x20) & !chunk & HIGH_BITS;

        // Quote: bytes == '"' (0x22)
        let chars_quote = chunk ^ (ONE_BYTES * (b'"' as Chunk));
        let contains_quote = chars_quote.wrapping_sub(ONE_BYTES) & !chars_quote & HIGH_BITS;

        // Backslash: bytes == '\\' (0x5C)
        let chars_backslash = chunk ^ (ONE_BYTES * (b'\\' as Chunk));
        let contains_backslash =
            chars_backslash.wrapping_sub(ONE_BYTES) & !chars_backslash & HIGH_BITS;

        let masked = contains_ctrl | contains_quote | contains_backslash;
        if masked != 0 {
            // Found a special byte - figure out which one and where
            let byte_idx = if cfg!(target_endian = "little") {
                masked.trailing_zeros() as usize / 8
            } else {
                masked.leading_zeros() as usize / 8
            };
            let hit_idx = i + byte_idx;
            let hit_byte = slice[hit_idx];
            return Some((hit_idx, hit_byte, !has_non_ascii));
        }

        i += STEP;
    }

    // Process remaining bytes one at a time
    while i < len {
        let b = slice[i];
        if b & 0x80 != 0 {
            has_non_ascii = true;
        }
        if b == b'"' || b == b'\\' || b < 0x20 {
            return Some((i, b, !has_non_ascii));
        }
        i += 1;
    }

    // No special byte found
    None
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

/// Hex decoding lookup tables.
/// HEX0[ch] = hex value of ch (0-15), or -1 if invalid
/// HEX1[ch] = hex value of ch shifted left by 4 bits, or -1 if invalid
///
/// Adapted from serde_json (MIT/Apache-2.0, Copyright David Tolnay).
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

// =============================================================================
// Inline String Parser Helpers
// =============================================================================
//
// These helpers support the inline string parser emitted by emit_parse_string_inline.
// They handle operations that are too complex to emit as Cranelift IR directly:
// - SIMD-accelerated memchr2
// - Scratch buffer memory management
// - UTF-8 validation

/// Find the next quote (") or backslash (\) in the input using SIMD-accelerated memchr2.
/// Returns the index of the hit, or -1 if not found.
///
/// This is worth keeping as a helper because memchr2 uses SIMD intrinsics that
/// can't be expressed in Cranelift IR.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn json_jit_memchr2_quote_backslash(input: *const u8, len: usize) -> isize {
    let slice = unsafe { std::slice::from_raw_parts(input, len) };
    match memchr::memchr2(b'"', b'\\', slice) {
        Some(idx) => idx as isize,
        None => -1,
    }
}

/// Take or initialize the scratch buffer from JitScratch.
/// If the buffer doesn't exist, creates one with the given capacity hint.
/// Clears the buffer and returns its pointer.
///
/// After this call, the scratch buffer in JitScratch is marked as "taken"
/// (ptr=null, len=0, cap=0) and the returned Vec is owned by the JIT code.
/// Call `json_jit_scratch_save` when done to return ownership.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn json_jit_scratch_take(scratch: *mut JitScratch, capacity_hint: usize) {
    let jit_scratch = unsafe { &mut *scratch };

    // If we don't have a scratch buffer yet, create one
    if jit_scratch.string_scratch_ptr.is_null() {
        let mut vec = Vec::<u8>::with_capacity(capacity_hint);
        jit_scratch.string_scratch_ptr = vec.as_mut_ptr();
        jit_scratch.string_scratch_len = 0;
        jit_scratch.string_scratch_cap = vec.capacity();
        std::mem::forget(vec);
    } else if jit_scratch.string_scratch_cap < capacity_hint {
        // Need to grow the buffer - the inline code doesn't call scratch_extend
        // which would grow, so we must ensure capacity here
        let mut vec = unsafe {
            Vec::from_raw_parts(
                jit_scratch.string_scratch_ptr,
                0, // We don't care about the old contents
                jit_scratch.string_scratch_cap,
            )
        };
        vec.reserve(capacity_hint - vec.capacity());
        jit_scratch.string_scratch_ptr = vec.as_mut_ptr();
        jit_scratch.string_scratch_len = 0;
        jit_scratch.string_scratch_cap = vec.capacity();
        std::mem::forget(vec);
    } else {
        // Clear the existing buffer (set len to 0, keep capacity)
        jit_scratch.string_scratch_len = 0;
    }
}

/// Extend the scratch buffer with bytes from the given pointer.
/// The scratch buffer must have been initialized with `json_jit_scratch_take`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn json_jit_scratch_extend(
    scratch: *mut JitScratch,
    src: *const u8,
    src_len: usize,
) {
    let jit_scratch = unsafe { &mut *scratch };

    // Reconstruct Vec from scratch buffer parts
    let mut vec = unsafe {
        Vec::from_raw_parts(
            jit_scratch.string_scratch_ptr,
            jit_scratch.string_scratch_len,
            jit_scratch.string_scratch_cap,
        )
    };

    // Extend with the source bytes
    let src_slice = unsafe { std::slice::from_raw_parts(src, src_len) };
    vec.extend_from_slice(src_slice);

    // Save back to scratch
    jit_scratch.string_scratch_ptr = vec.as_mut_ptr();
    jit_scratch.string_scratch_len = vec.len();
    jit_scratch.string_scratch_cap = vec.capacity();
    std::mem::forget(vec);
}

/// Push a single byte to the scratch buffer.
/// The scratch buffer must have been initialized with `json_jit_scratch_take`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn json_jit_scratch_push_byte(scratch: *mut JitScratch, byte: u8) {
    let jit_scratch = unsafe { &mut *scratch };

    // Reconstruct Vec from scratch buffer parts
    let mut vec = unsafe {
        Vec::from_raw_parts(
            jit_scratch.string_scratch_ptr,
            jit_scratch.string_scratch_len,
            jit_scratch.string_scratch_cap,
        )
    };

    vec.push(byte);

    // Save back to scratch
    jit_scratch.string_scratch_ptr = vec.as_mut_ptr();
    jit_scratch.string_scratch_len = vec.len();
    jit_scratch.string_scratch_cap = vec.capacity();
    std::mem::forget(vec);
}

/// Decode a \uXXXX escape sequence (and potential surrogate pair) and push as UTF-8.
/// Returns the number of input bytes consumed (4 for BMP, 10 for surrogate pair),
/// or negative error code on failure.
///
/// This handles the complex surrogate pair logic that would be difficult to emit as IR.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn json_jit_decode_unicode_escape(
    scratch: *mut JitScratch,
    input: *const u8,
    remaining_len: usize,
) -> isize {
    // Need at least 4 hex digits
    if remaining_len < 4 {
        return error::INVALID_ESCAPE as isize;
    }

    let slice = unsafe { std::slice::from_raw_parts(input, remaining_len) };
    let code_point = match decode_four_hex_digits(slice[0], slice[1], slice[2], slice[3]) {
        Some(n) => n,
        None => return error::INVALID_ESCAPE as isize,
    };

    // Handle surrogate pairs
    if (0xD800..=0xDBFF).contains(&code_point) {
        // High surrogate - look for low surrogate
        if remaining_len < 10 || slice[4] != b'\\' || slice[5] != b'u' {
            return error::INVALID_ESCAPE as isize;
        }

        let low_point = match decode_four_hex_digits(slice[6], slice[7], slice[8], slice[9]) {
            Some(n) if (0xDC00..=0xDFFF).contains(&n) => n,
            _ => return error::INVALID_ESCAPE as isize,
        };

        // Valid surrogate pair
        let full = 0x10000 + ((code_point as u32 - 0xD800) << 10) + (low_point as u32 - 0xDC00);

        // Push UTF-8 encoded codepoint to scratch
        let jit_scratch = unsafe { &mut *scratch };
        let mut vec = unsafe {
            Vec::from_raw_parts(
                jit_scratch.string_scratch_ptr,
                jit_scratch.string_scratch_len,
                jit_scratch.string_scratch_cap,
            )
        };
        push_utf8_codepoint(full, &mut vec);
        jit_scratch.string_scratch_ptr = vec.as_mut_ptr();
        jit_scratch.string_scratch_len = vec.len();
        jit_scratch.string_scratch_cap = vec.capacity();
        std::mem::forget(vec);

        10 // Consumed \uXXXX\uXXXX (we're positioned after 'u', so 4+6=10)
    } else if (0xDC00..=0xDFFF).contains(&code_point) {
        // Lone low surrogate is invalid
        error::INVALID_ESCAPE as isize
    } else {
        // BMP character
        let jit_scratch = unsafe { &mut *scratch };
        let mut vec = unsafe {
            Vec::from_raw_parts(
                jit_scratch.string_scratch_ptr,
                jit_scratch.string_scratch_len,
                jit_scratch.string_scratch_cap,
            )
        };
        push_utf8_codepoint(code_point as u32, &mut vec);
        jit_scratch.string_scratch_ptr = vec.as_mut_ptr();
        jit_scratch.string_scratch_len = vec.len();
        jit_scratch.string_scratch_cap = vec.capacity();
        std::mem::forget(vec);

        4 // Consumed XXXX (4 hex digits)
    }
}

/// Finalize the scratch buffer into an owned String.
/// Validates UTF-8 if is_ascii is false.
/// Writes the result to the output pointer.
/// The scratch buffer remains allocated for reuse (cleared but capacity preserved).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn json_jit_scratch_finalize_string(
    scratch: *mut JitScratch,
    out: *mut JsonJitStringResult,
    new_pos: usize,
    is_ascii: u8,
) {
    let jit_scratch = unsafe { &mut *scratch };

    // Reconstruct Vec from scratch buffer parts
    let vec = unsafe {
        Vec::from_raw_parts(
            jit_scratch.string_scratch_ptr,
            jit_scratch.string_scratch_len,
            jit_scratch.string_scratch_cap,
        )
    };

    // Validate UTF-8 if not ASCII
    let result = if is_ascii != 0 {
        // ASCII-only: no validation needed
        let s = unsafe { String::from_utf8_unchecked(vec) };
        JsonJitStringResult::owned(new_pos, s)
    } else {
        match String::from_utf8(vec) {
            Ok(s) => JsonJitStringResult::owned(new_pos, s),
            Err(e) => {
                // Put the vec back into scratch before returning error
                let vec = e.into_bytes();
                jit_scratch.string_scratch_ptr = vec.as_ptr() as *mut u8;
                jit_scratch.string_scratch_len = 0; // Clear for next use
                jit_scratch.string_scratch_cap = vec.capacity();
                std::mem::forget(vec);
                JsonJitStringResult::error(new_pos, error::INVALID_UTF8)
            }
        }
    };

    // The string has taken ownership of the buffer's data, so we need a fresh buffer
    // Allocate a new scratch buffer for future use
    let new_vec = Vec::<u8>::with_capacity(64);
    jit_scratch.string_scratch_ptr = new_vec.as_ptr() as *mut u8;
    jit_scratch.string_scratch_len = 0;
    jit_scratch.string_scratch_cap = new_vec.capacity();
    std::mem::forget(new_vec);

    unsafe { out.write(result) };
}

/// Check if a byte slice is all ASCII using word-at-a-time scanning.
/// Returns 1 if all ASCII, 0 if non-ASCII bytes present.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn json_jit_is_ascii(input: *const u8, len: usize) -> u8 {
    let slice = unsafe { std::slice::from_raw_parts(input, len) };
    if is_ascii_swar(slice) { 1 } else { 0 }
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
    let slice = unsafe { std::slice::from_raw_parts(ptr, len) };
    let mut i = 0;

    loop {
        // Use SIMD-accelerated memchr2 to find " or \
        let hit = memchr::memchr2(b'"', b'\\', &slice[i..])?;
        let abs_hit = i + hit;
        let byte = slice[abs_hit];

        if byte == b'"' {
            return Some(abs_hit);
        }

        // Found escape - skip it
        i = abs_hit + 1; // Move past backslash
        if i >= len {
            return None;
        }
        let escaped = slice[i];
        if escaped == b'u' {
            i += 5; // +1 for 'u', +4 for hex digits
        } else {
            i += 1; // Skip the escaped character
        }
    }
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
