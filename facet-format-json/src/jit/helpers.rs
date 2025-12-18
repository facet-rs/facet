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
    let mut p = start;
    let mut has_escapes = false;

    // Scan to find closing quote and check for escapes
    while p < len {
        let byte = unsafe { *input.add(p) };
        if byte == b'"' {
            // Found closing quote
            if has_escapes {
                // Need to decode escapes
                let slice = unsafe { std::slice::from_raw_parts(input.add(start), p - start) };
                match decode_json_string(slice) {
                    Ok(s) => return JsonJitStringResult::owned(p + 1, s),
                    Err(code) => return JsonJitStringResult::error(pos, code),
                }
            } else {
                // No escapes, return borrowed slice
                let ptr = unsafe { input.add(start) };
                return JsonJitStringResult::borrowed(p + 1, ptr, p - start);
            }
        } else if byte == b'\\' {
            has_escapes = true;
            p += 1; // Skip the backslash
            if p >= len {
                return JsonJitStringResult::error(pos, error::UNEXPECTED_EOF);
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

    // Reached end without closing quote
    JsonJitStringResult::error(pos, error::UNEXPECTED_EOF)
}

/// Decode a JSON string with escape sequences.
fn decode_json_string(slice: &[u8]) -> Result<String, i32> {
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
            // Regular byte - push as UTF-8
            result.push(byte as char);
            i += 1;
        }
    }

    Ok(result)
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
