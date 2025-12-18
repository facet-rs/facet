//! JSON JIT helper functions for Tier-2 format JIT.
//!
//! These extern "C" functions implement JSON parsing operations for direct
//! byte-level parsing by JIT-compiled code.

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
    #[cfg(debug_assertions)]
    eprintln!("[json_jit_seq_is_end] pos={}, len={}", pos, len);
    if pos >= len {
        #[cfg(debug_assertions)]
        eprintln!("[json_jit_seq_is_end] EOF!");
        return JsonJitPosEndError::new(pos, false, error::UNEXPECTED_EOF);
    }

    let byte = unsafe { *input.add(pos) };
    #[cfg(debug_assertions)]
    eprintln!("[json_jit_seq_is_end] byte='{}' ({})", byte as char, byte);
    if byte == b']' {
        // Skip whitespace after ']'
        let pos = unsafe { json_jit_skip_ws(input, len, pos + 1) };
        #[cfg(debug_assertions)]
        eprintln!("[json_jit_seq_is_end] -> is_end=true, new_pos={}", pos);
        JsonJitPosEndError::new(pos, true, 0)
    } else {
        #[cfg(debug_assertions)]
        eprintln!("[json_jit_seq_is_end] -> is_end=false, new_pos={}", pos);
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
