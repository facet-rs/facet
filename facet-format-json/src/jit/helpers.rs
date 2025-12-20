//! JSON JIT helper functions for Tier-2 format JIT.
//!
//! These extern "C" functions implement JSON parsing operations for direct
//! byte-level parsing by JIT-compiled code.

use super::jit_debug;

// =============================================================================
// Return Types
// =============================================================================

/// Return type for json_jit_seq_begin and json_jit_seq_next
#[repr(C)]
pub struct JsonJitPosError {
    /// New position after parsing
    pub new_pos: usize,
    /// Error code (0 = success, negative = error)
    pub error: i32,
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
#[unsafe(no_mangle)]
pub unsafe extern "C" fn json_jit_parse_string(
    out: *mut JsonJitStringResult,
    input: *const u8,
    len: usize,
    pos: usize,
) {
    let result = json_jit_parse_string_impl(input, len, pos);
    unsafe { out.write(result) };
}

fn json_jit_parse_string_impl(input: *const u8, len: usize, pos: usize) -> JsonJitStringResult {
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
            return JsonJitStringResult::borrowed(start + hit_idx + 1, ptr, string_len);
        } else {
            // Non-ASCII: validate UTF-8
            let slice = unsafe { std::slice::from_raw_parts(ptr, string_len) };
            return match std::str::from_utf8(slice) {
                Ok(_) => JsonJitStringResult::borrowed(start + hit_idx + 1, ptr, string_len),
                Err(_) => JsonJitStringResult::error(pos, error::INVALID_UTF8),
            };
        }
    } else {
        // Found backslash - escaped path
        return parse_string_with_escapes(input, len, pos, start, start + hit_idx);
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
#[inline(never)]
fn parse_string_with_escapes(
    input: *const u8,
    len: usize,
    pos: usize,
    start: usize,
    first_escape_pos: usize,
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

            match decode_json_string(slice, is_ascii) {
                Ok(s) => return JsonJitStringResult::owned(p + 1, s),
                Err(code) => return JsonJitStringResult::error(pos, code),
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

/// Decode a JSON string with escape sequences.
/// `is_ascii`: hint that all bytes are ASCII (< 0x80), allowing faster processing
fn decode_json_string(slice: &[u8], is_ascii: bool) -> Result<String, i32> {
    // Reserve capacity: slice.len() is upper bound (escapes make output shorter)
    let mut result = String::with_capacity(slice.len());
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
                b'"' => result.push('"'),
                b'\\' => result.push('\\'),
                b'/' => result.push('/'),
                b'b' => result.push('\x08'),
                b'f' => result.push('\x0C'),
                b'n' => result.push('\n'),
                b'r' => result.push('\r'),
                b't' => result.push('\t'),
                b'u' => {
                    // \uXXXX
                    if i + 4 >= slice.len() {
                        return Err(error::INVALID_ESCAPE);
                    }
                    let hex = &slice[i + 1..i + 5];
                    let hex_str = match std::str::from_utf8(hex) {
                        Ok(s) => s,
                        Err(_) => return Err(error::INVALID_ESCAPE),
                    };
                    let code_point = match u16::from_str_radix(hex_str, 16) {
                        Ok(n) => n,
                        Err(_) => return Err(error::INVALID_ESCAPE),
                    };
                    // Handle surrogate pairs
                    if (0xD800..=0xDBFF).contains(&code_point) {
                        // High surrogate - look for low surrogate
                        if i + 10 < slice.len() && slice[i + 5] == b'\\' && slice[i + 6] == b'u' {
                            let low_hex = &slice[i + 7..i + 11];
                            if let Ok(low_str) = std::str::from_utf8(low_hex) {
                                if let Ok(low_point) = u16::from_str_radix(low_str, 16) {
                                    if (0xDC00..=0xDFFF).contains(&low_point) {
                                        // Valid surrogate pair
                                        let full = 0x10000
                                            + ((code_point as u32 - 0xD800) << 10)
                                            + (low_point as u32 - 0xDC00);
                                        if let Some(c) = char::from_u32(full) {
                                            result.push(c);
                                            i += 10; // Skip both \uXXXX sequences
                                            i += 1;
                                            continue;
                                        }
                                    }
                                }
                            }
                        }
                        return Err(error::INVALID_ESCAPE);
                    } else if let Some(c) = char::from_u32(code_point as u32) {
                        result.push(c);
                    } else {
                        return Err(error::INVALID_ESCAPE);
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
            let chunk = &slice[chunk_start..i];

            if is_ascii {
                // ASCII fast path: skip UTF-8 validation, convert directly
                // SAFETY: caller guarantees all bytes are < 0x80, which is valid UTF-8
                let s = unsafe { std::str::from_utf8_unchecked(chunk) };
                result.push_str(s);
            } else {
                // Non-ASCII: validate UTF-8
                let s = std::str::from_utf8(chunk).map_err(|_| error::INVALID_UTF8)?;
                result.push_str(s);
            }
        }
    }

    Ok(result)
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

/// Internal implementation of f64 parsing.
fn json_jit_parse_f64_impl(input: *const u8, len: usize, pos: usize) -> JsonJitF64Result {
    let mut p = pos;

    // Find the end of the number
    let start = p;
    let mut has_digit = false;

    // Optional minus sign
    if p < len && unsafe { *input.add(p) } == b'-' {
        p += 1;
    }

    // Integer part (at least one digit required)
    while p < len {
        let byte = unsafe { *input.add(p) };
        if byte >= b'0' && byte <= b'9' {
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
            if byte >= b'0' && byte <= b'9' {
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
                if byte >= b'0' && byte <= b'9' {
                    p += 1;
                } else {
                    break;
                }
            }
        }
    }

    if !has_digit {
        return JsonJitF64Result {
            new_pos: pos,
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
                new_pos: pos,
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
            new_pos: pos,
            value: 0.0,
            error: error::NUMBER_OVERFLOW,
        },
    }
}

/// Skip a JSON value (scalar, string, array, or object).
/// Returns: (new_pos, error_code). error_code is 0 on success.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn json_jit_skip_value(
    input: *const u8,
    len: usize,
    pos: usize,
) -> JsonJitPosError {
    // Skip leading whitespace
    let pos = unsafe { json_jit_skip_ws(input, len, pos) };

    if pos >= len {
        return JsonJitPosError {
            new_pos: pos,
            error: error::UNEXPECTED_EOF,
        };
    }

    let byte = unsafe { *input.add(pos) };

    match byte {
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
    }
}

fn skip_string(input: *const u8, len: usize, pos: usize) -> JsonJitPosError {
    // Expect opening quote
    if pos >= len || unsafe { *input.add(pos) } != b'"' {
        return JsonJitPosError {
            new_pos: pos,
            error: error::EXPECTED_STRING,
        };
    }

    let mut p = pos + 1;
    while p < len {
        let byte = unsafe { *input.add(p) };
        if byte == b'"' {
            // Found closing quote
            return JsonJitPosError {
                new_pos: p + 1,
                error: 0,
            };
        } else if byte == b'\\' {
            // Skip escape sequence
            p += 1;
            if p >= len {
                return JsonJitPosError {
                    new_pos: pos,
                    error: error::UNEXPECTED_EOF,
                };
            }
            let escaped = unsafe { *input.add(p) };
            if escaped == b'u' {
                // \uXXXX - skip 4 more bytes
                p += 4;
            }
            p += 1;
        } else {
            p += 1;
        }
    }

    JsonJitPosError {
        new_pos: pos,
        error: error::UNEXPECTED_EOF,
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
        if byte >= b'0' && byte <= b'9' {
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
            if byte >= b'0' && byte <= b'9' {
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
                if byte >= b'0' && byte <= b'9' {
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
        if result.error != 0 {
            return result;
        }
        p = result.new_pos;

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
        if result.error != 0 {
            return result;
        }
        p = result.new_pos;

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
