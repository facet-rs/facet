//! Rust helper functions that JIT'd code calls back into.
//!
//! All helpers take (input, len, pos) as separate values and return new_pos (>= 0) or error (< 0).
//! This allows the JIT to keep pos in a register instead of going through memory.

/// Result of a parse operation.
///
/// - `>= 0` means success, value is new position.
/// - `< 0` means error code.
pub type ParseResult = isize;

// Error codes (negative values)
pub const ERR_UNEXPECTED_EOF: isize = -1;
pub const ERR_EXPECTED_COLON: isize = -2;
pub const ERR_EXPECTED_COMMA_OR_END: isize = -3;
pub const ERR_EXPECTED_OBJECT_START: isize = -4;
pub const ERR_EXPECTED_ARRAY_START: isize = -5;
pub const ERR_INVALID_NUMBER: isize = -6;
pub const ERR_INVALID_STRING: isize = -7;
pub const ERR_INVALID_BOOL: isize = -8;

/// Get a byte slice from raw parts.
#[inline(always)]
unsafe fn slice(input: *const u8, len: usize, pos: usize) -> &'static [u8] {
    unsafe { std::slice::from_raw_parts(input.add(pos), len - pos) }
}

/// Get full input slice.
#[inline(always)]
unsafe fn full_slice(input: *const u8, len: usize) -> &'static [u8] {
    unsafe { std::slice::from_raw_parts(input, len) }
}

// =============================================================================
// Value parsers - parse a value and write to output pointer
// All take (input, len, pos, out) and return new_pos or error
// =============================================================================

/// Parse f64, write to *out, return new position or error.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jitson_parse_f64(
    input: *const u8,
    len: usize,
    pos: usize,
    out: *mut f64,
) -> ParseResult {
    let bytes = unsafe { slice(input, len, pos) };

    let mut end = 0;
    while end < bytes.len() {
        match bytes[end] {
            b'0'..=b'9' | b'-' | b'+' | b'.' | b'e' | b'E' => end += 1,
            _ => break,
        }
    }

    if end == 0 {
        return ERR_INVALID_NUMBER;
    }

    use lexical_parse_float::FromLexical;
    match f64::from_lexical(&bytes[..end]) {
        Ok(val) => {
            unsafe { *out = val };
            (pos + end) as isize
        }
        Err(_) => ERR_INVALID_NUMBER,
    }
}

/// Parse f32, write to *out, return new position or error.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jitson_parse_f32(
    input: *const u8,
    len: usize,
    pos: usize,
    out: *mut f32,
) -> ParseResult {
    let bytes = unsafe { slice(input, len, pos) };

    let mut end = 0;
    while end < bytes.len() {
        match bytes[end] {
            b'0'..=b'9' | b'-' | b'+' | b'.' | b'e' | b'E' => end += 1,
            _ => break,
        }
    }

    if end == 0 {
        return ERR_INVALID_NUMBER;
    }

    use lexical_parse_float::FromLexical;
    match f32::from_lexical(&bytes[..end]) {
        Ok(val) => {
            unsafe { *out = val };
            (pos + end) as isize
        }
        Err(_) => ERR_INVALID_NUMBER,
    }
}

/// Parse i64, write to *out, return new position or error.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jitson_parse_i64(
    input: *const u8,
    len: usize,
    pos: usize,
    out: *mut i64,
) -> ParseResult {
    let bytes = unsafe { slice(input, len, pos) };

    let mut end = 0;
    while end < bytes.len() {
        match bytes[end] {
            b'0'..=b'9' | b'-' | b'+' => end += 1,
            _ => break,
        }
    }

    if end == 0 {
        return ERR_INVALID_NUMBER;
    }

    use lexical_parse_integer::FromLexical;
    match i64::from_lexical(&bytes[..end]) {
        Ok(val) => {
            unsafe { *out = val };
            (pos + end) as isize
        }
        Err(_) => ERR_INVALID_NUMBER,
    }
}

/// Parse i32, write to *out, return new position or error.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jitson_parse_i32(
    input: *const u8,
    len: usize,
    pos: usize,
    out: *mut i32,
) -> ParseResult {
    let bytes = unsafe { slice(input, len, pos) };

    let mut end = 0;
    while end < bytes.len() {
        match bytes[end] {
            b'0'..=b'9' | b'-' | b'+' => end += 1,
            _ => break,
        }
    }

    if end == 0 {
        return ERR_INVALID_NUMBER;
    }

    use lexical_parse_integer::FromLexical;
    match i32::from_lexical(&bytes[..end]) {
        Ok(val) => {
            unsafe { *out = val };
            (pos + end) as isize
        }
        Err(_) => ERR_INVALID_NUMBER,
    }
}

/// Parse i16, write to *out, return new position or error.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jitson_parse_i16(
    input: *const u8,
    len: usize,
    pos: usize,
    out: *mut i16,
) -> ParseResult {
    let bytes = unsafe { slice(input, len, pos) };

    let mut end = 0;
    while end < bytes.len() {
        match bytes[end] {
            b'0'..=b'9' | b'-' | b'+' => end += 1,
            _ => break,
        }
    }

    if end == 0 {
        return ERR_INVALID_NUMBER;
    }

    use lexical_parse_integer::FromLexical;
    match i16::from_lexical(&bytes[..end]) {
        Ok(val) => {
            unsafe { *out = val };
            (pos + end) as isize
        }
        Err(_) => ERR_INVALID_NUMBER,
    }
}

/// Parse i8, write to *out, return new position or error.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jitson_parse_i8(
    input: *const u8,
    len: usize,
    pos: usize,
    out: *mut i8,
) -> ParseResult {
    let bytes = unsafe { slice(input, len, pos) };

    let mut end = 0;
    while end < bytes.len() {
        match bytes[end] {
            b'0'..=b'9' | b'-' | b'+' => end += 1,
            _ => break,
        }
    }

    if end == 0 {
        return ERR_INVALID_NUMBER;
    }

    use lexical_parse_integer::FromLexical;
    match i8::from_lexical(&bytes[..end]) {
        Ok(val) => {
            unsafe { *out = val };
            (pos + end) as isize
        }
        Err(_) => ERR_INVALID_NUMBER,
    }
}

/// Parse u64, write to *out, return new position or error.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jitson_parse_u64(
    input: *const u8,
    len: usize,
    pos: usize,
    out: *mut u64,
) -> ParseResult {
    let bytes = unsafe { slice(input, len, pos) };

    let mut end = 0;
    while end < bytes.len() {
        match bytes[end] {
            b'0'..=b'9' | b'+' => end += 1,
            _ => break,
        }
    }

    if end == 0 {
        return ERR_INVALID_NUMBER;
    }

    use lexical_parse_integer::FromLexical;
    match u64::from_lexical(&bytes[..end]) {
        Ok(val) => {
            unsafe { *out = val };
            (pos + end) as isize
        }
        Err(_) => ERR_INVALID_NUMBER,
    }
}

/// Parse u32, write to *out, return new position or error.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jitson_parse_u32(
    input: *const u8,
    len: usize,
    pos: usize,
    out: *mut u32,
) -> ParseResult {
    let bytes = unsafe { slice(input, len, pos) };

    let mut end = 0;
    while end < bytes.len() {
        match bytes[end] {
            b'0'..=b'9' | b'+' => end += 1,
            _ => break,
        }
    }

    if end == 0 {
        return ERR_INVALID_NUMBER;
    }

    use lexical_parse_integer::FromLexical;
    match u32::from_lexical(&bytes[..end]) {
        Ok(val) => {
            unsafe { *out = val };
            (pos + end) as isize
        }
        Err(_) => ERR_INVALID_NUMBER,
    }
}

/// Parse u16, write to *out, return new position or error.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jitson_parse_u16(
    input: *const u8,
    len: usize,
    pos: usize,
    out: *mut u16,
) -> ParseResult {
    let bytes = unsafe { slice(input, len, pos) };

    let mut end = 0;
    while end < bytes.len() {
        match bytes[end] {
            b'0'..=b'9' | b'+' => end += 1,
            _ => break,
        }
    }

    if end == 0 {
        return ERR_INVALID_NUMBER;
    }

    use lexical_parse_integer::FromLexical;
    match u16::from_lexical(&bytes[..end]) {
        Ok(val) => {
            unsafe { *out = val };
            (pos + end) as isize
        }
        Err(_) => ERR_INVALID_NUMBER,
    }
}

/// Parse u8, write to *out, return new position or error.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jitson_parse_u8(
    input: *const u8,
    len: usize,
    pos: usize,
    out: *mut u8,
) -> ParseResult {
    let bytes = unsafe { slice(input, len, pos) };

    let mut end = 0;
    while end < bytes.len() {
        match bytes[end] {
            b'0'..=b'9' | b'+' => end += 1,
            _ => break,
        }
    }

    if end == 0 {
        return ERR_INVALID_NUMBER;
    }

    use lexical_parse_integer::FromLexical;
    match u8::from_lexical(&bytes[..end]) {
        Ok(val) => {
            unsafe { *out = val };
            (pos + end) as isize
        }
        Err(_) => ERR_INVALID_NUMBER,
    }
}

/// Parse bool, write to *out (0 or 1), return new position or error.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jitson_parse_bool(
    input: *const u8,
    len: usize,
    pos: usize,
    out: *mut u8,
) -> ParseResult {
    let bytes = unsafe { slice(input, len, pos) };

    if bytes.starts_with(b"true") {
        unsafe { *out = 1 };
        (pos + 4) as isize
    } else if bytes.starts_with(b"false") {
        unsafe { *out = 0 };
        (pos + 5) as isize
    } else {
        ERR_INVALID_BOOL
    }
}

/// Parse String, write to *out, return new position or error.
/// Uses SIMD-accelerated memchr for fast string scanning.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jitson_parse_string(
    input: *const u8,
    len: usize,
    pos: usize,
    out: *mut String,
) -> ParseResult {
    let bytes = unsafe { slice(input, len, pos) };

    // Expect opening quote
    if bytes.is_empty() || bytes[0] != b'"' {
        return ERR_INVALID_STRING;
    }

    let content = &bytes[1..];

    match memchr::memchr2(b'"', b'\\', content) {
        Some(offset) => {
            if content[offset] == b'"' {
                // Found closing quote - no escapes in this string!
                let s = unsafe { std::str::from_utf8_unchecked(&content[..offset]) };
                unsafe { std::ptr::write(out, s.to_owned()) };
                (pos + 1 + offset + 1) as isize // +1 open quote, +1 close quote
            } else {
                // Found backslash - need slow path with escape handling
                match find_string_end_with_escapes(content, offset) {
                    Some(end) => {
                        let result = decode_escaped_string(&content[..end]);
                        unsafe { std::ptr::write(out, result) };
                        (pos + 1 + end + 1) as isize
                    }
                    None => ERR_UNEXPECTED_EOF,
                }
            }
        }
        None => ERR_UNEXPECTED_EOF,
    }
}

/// Find the end of a string that contains escapes (position of closing quote).
#[inline]
fn find_string_end_with_escapes(bytes: &[u8], first_backslash: usize) -> Option<usize> {
    let mut pos = first_backslash;

    loop {
        if pos + 1 >= bytes.len() {
            return None;
        }
        pos += 2; // Skip \X

        match memchr::memchr2(b'"', b'\\', &bytes[pos..]) {
            Some(offset) => {
                let abs_pos = pos + offset;
                if bytes[abs_pos] == b'"' {
                    return Some(abs_pos);
                } else {
                    pos = abs_pos;
                }
            }
            None => return None,
        }
    }
}

/// Decode a string that contains escape sequences.
#[inline]
fn decode_escaped_string(bytes: &[u8]) -> String {
    let mut result = String::with_capacity(bytes.len());
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 1 < bytes.len() {
            match bytes[i + 1] {
                b'"' => {
                    result.push('"');
                    i += 2;
                }
                b'\\' => {
                    result.push('\\');
                    i += 2;
                }
                b'/' => {
                    result.push('/');
                    i += 2;
                }
                b'b' => {
                    result.push('\x08');
                    i += 2;
                }
                b'f' => {
                    result.push('\x0c');
                    i += 2;
                }
                b'n' => {
                    result.push('\n');
                    i += 2;
                }
                b'r' => {
                    result.push('\r');
                    i += 2;
                }
                b't' => {
                    result.push('\t');
                    i += 2;
                }
                b'u' if i + 5 < bytes.len() => {
                    if let Ok(hex) = std::str::from_utf8(&bytes[i + 2..i + 6])
                        && let Ok(code) = u16::from_str_radix(hex, 16)
                        && let Some(c) = char::from_u32(code as u32)
                    {
                        result.push(c);
                    }
                    i += 6;
                }
                _ => {
                    i += 1;
                }
            }
        } else {
            result.push(bytes[i] as char);
            i += 1;
        }
    }

    result
}

// =============================================================================
// Skip value - for unknown fields
// =============================================================================

/// Skip a JSON value (for unknown fields).
/// Returns new position or error.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jitson_skip_value(
    input: *const u8,
    len: usize,
    pos: usize,
) -> ParseResult {
    let bytes = unsafe { full_slice(input, len) };
    let mut i = pos;

    // Skip leading whitespace
    while i < len && matches!(bytes[i], b' ' | b'\t' | b'\n' | b'\r') {
        i += 1;
    }

    if i >= len {
        return ERR_UNEXPECTED_EOF;
    }

    match skip_value_inner(bytes, &mut i) {
        Ok(()) => i as isize,
        Err(e) => e,
    }
}

/// Simpler skip function - just track depth and handle strings specially.
/// For skipping, we don't need to validate JSON structure, just find the end.
#[inline]
fn skip_value_inner(bytes: &[u8], i: &mut usize) -> Result<(), ParseResult> {
    if *i >= bytes.len() {
        return Err(ERR_UNEXPECTED_EOF);
    }

    match bytes[*i] {
        b'"' => skip_string(bytes, i),
        b'{' | b'[' => skip_container(bytes, i),
        b't' => {
            if bytes.len() - *i >= 4 && &bytes[*i..*i + 4] == b"true" {
                *i += 4;
                Ok(())
            } else {
                Err(ERR_INVALID_BOOL)
            }
        }
        b'f' => {
            if bytes.len() - *i >= 5 && &bytes[*i..*i + 5] == b"false" {
                *i += 5;
                Ok(())
            } else {
                Err(ERR_INVALID_BOOL)
            }
        }
        b'n' => {
            if bytes.len() - *i >= 4 && &bytes[*i..*i + 4] == b"null" {
                *i += 4;
                Ok(())
            } else {
                Err(ERR_INVALID_BOOL)
            }
        }
        b'0'..=b'9' | b'-' => {
            skip_number(bytes, i);
            Ok(())
        }
        _ => Err(ERR_UNEXPECTED_EOF),
    }
}

/// Skip an object or array by tracking depth.
/// We don't validate structure, just find matching close bracket.
#[inline]
fn skip_container(bytes: &[u8], i: &mut usize) -> Result<(), ParseResult> {
    let mut depth = 1i32;
    let mut pos = *i + 1; // skip opening { or [
    let len = bytes.len();

    while pos < len {
        let b = bytes[pos];
        pos += 1;

        if b == b'"' {
            // Skip string using memchr2 for speed
            loop {
                match memchr::memchr2(b'"', b'\\', &bytes[pos..]) {
                    Some(offset) => {
                        pos += offset;
                        if bytes[pos] == b'"' {
                            pos += 1;
                            break;
                        } else {
                            // backslash - skip escape
                            pos += 2;
                        }
                    }
                    None => return Err(ERR_UNEXPECTED_EOF),
                }
            }
            // After string, fast-skip non-structural bytes
            pos += skip_to_structural(&bytes[pos..]);
        } else if b == b'{' || b == b'[' {
            depth += 1;
            pos += skip_to_structural(&bytes[pos..]);
        } else if b == b'}' || b == b']' {
            depth -= 1;
            if depth == 0 {
                *i = pos;
                return Ok(());
            }
            pos += skip_to_structural(&bytes[pos..]);
        }
    }

    Err(ERR_UNEXPECTED_EOF)
}

/// Fast-skip to next structural character (" { } [ ]) using SWAR
#[inline(always)]
fn skip_to_structural(bytes: &[u8]) -> usize {
    const STEP: usize = 8;
    const ONE_BYTES: u64 = 0x0101010101010101;

    let mut pos = 0;
    let len = bytes.len();

    // Process 8 bytes at a time
    while pos + STEP <= len {
        let chunk = u64::from_le_bytes(bytes[pos..pos + STEP].try_into().unwrap());

        // Check for any interesting byte: " { } [ ]
        let quote = chunk ^ (ONE_BYTES * b'"' as u64);
        let open_brace = chunk ^ (ONE_BYTES * b'{' as u64);
        let close_brace = chunk ^ (ONE_BYTES * b'}' as u64);
        let open_bracket = chunk ^ (ONE_BYTES * b'[' as u64);
        let close_bracket = chunk ^ (ONE_BYTES * b']' as u64);

        let has_zero = |v: u64| (v.wrapping_sub(ONE_BYTES)) & !v & (ONE_BYTES * 0x80);

        let mask = has_zero(quote)
            | has_zero(open_brace)
            | has_zero(close_brace)
            | has_zero(open_bracket)
            | has_zero(close_bracket);

        if mask != 0 {
            return pos + mask.trailing_zeros() as usize / 8;
        }
        pos += STEP;
    }

    // Check remaining bytes
    while pos < len {
        match bytes[pos] {
            b'"' | b'{' | b'}' | b'[' | b']' => return pos,
            _ => pos += 1,
        }
    }

    pos
}

/// Skip a JSON string (opening quote already peeked, not consumed).
#[inline(always)]
fn skip_string(bytes: &[u8], i: &mut usize) -> Result<(), ParseResult> {
    *i += 1; // skip opening quote
    loop {
        // Use memchr for fast scanning
        match memchr::memchr2(b'"', b'\\', &bytes[*i..]) {
            Some(offset) => {
                if bytes[*i + offset] == b'"' {
                    *i += offset + 1;
                    return Ok(());
                } else {
                    // Backslash - skip the escape sequence
                    *i += offset + 2;
                    if *i > bytes.len() {
                        return Err(ERR_UNEXPECTED_EOF);
                    }
                }
            }
            None => return Err(ERR_UNEXPECTED_EOF),
        }
    }
}

/// Skip a JSON number.
#[inline(always)]
fn skip_number(bytes: &[u8], i: &mut usize) {
    while *i < bytes.len() {
        match bytes[*i] {
            b'0'..=b'9' | b'-' | b'+' | b'.' | b'e' | b'E' => *i += 1,
            _ => break,
        }
    }
}

#[inline(always)]
fn skip_ws_inline(bytes: &[u8], i: &mut usize) {
    while *i < bytes.len() {
        let b = bytes[*i];
        if b == b' ' || b == b'\t' || b == b'\n' || b == b'\r' {
            *i += 1;
        } else {
            break;
        }
    }
}

// =============================================================================
// Vec parsers
// =============================================================================

/// Parse `Vec<f64>`, write to *out, return new position or error.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jitson_parse_vec_f64(
    input: *const u8,
    len: usize,
    pos: usize,
    out: *mut Vec<f64>,
) -> ParseResult {
    let bytes = unsafe { slice(input, len, pos) };
    let mut i = 0;

    if bytes.is_empty() || bytes[0] != b'[' {
        return ERR_EXPECTED_ARRAY_START;
    }
    i += 1;
    skip_ws_inline(bytes, &mut i);

    let mut vec = Vec::with_capacity(16);

    if i < bytes.len() && bytes[i] == b']' {
        unsafe { std::ptr::write(out, vec) };
        return (pos + i + 1) as isize;
    }

    loop {
        // Parse number directly
        let mut end = i;
        while end < bytes.len() {
            match bytes[end] {
                b'0'..=b'9' | b'-' | b'+' | b'.' | b'e' | b'E' => end += 1,
                _ => break,
            }
        }

        if end == i {
            return ERR_INVALID_NUMBER;
        }

        use lexical_parse_float::FromLexical;
        match f64::from_lexical(&bytes[i..end]) {
            Ok(val) => vec.push(val),
            Err(_) => return ERR_INVALID_NUMBER,
        }
        i = end;

        skip_ws_inline(bytes, &mut i);
        if i >= bytes.len() {
            return ERR_EXPECTED_COMMA_OR_END;
        }
        match bytes[i] {
            b',' => {
                i += 1;
                skip_ws_inline(bytes, &mut i);
            }
            b']' => {
                unsafe { std::ptr::write(out, vec) };
                return (pos + i + 1) as isize;
            }
            _ => return ERR_EXPECTED_COMMA_OR_END,
        }
    }
}

/// Parse `Vec<i64>`, write to *out, return new position or error.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jitson_parse_vec_i64(
    input: *const u8,
    len: usize,
    pos: usize,
    out: *mut Vec<i64>,
) -> ParseResult {
    unsafe {
        let bytes = slice(input, len, pos);
        let mut i = 0;

        if bytes.is_empty() || bytes[0] != b'[' {
            return ERR_EXPECTED_ARRAY_START;
        }
        i += 1;
        skip_ws_inline(bytes, &mut i);

        let mut vec = Vec::with_capacity(16);

        if i < bytes.len() && bytes[i] == b']' {
            std::ptr::write(out, vec);
            return (pos + i + 1) as isize;
        }

        loop {
            // Parse number directly
            let mut end = i;
            while end < bytes.len() {
                match bytes[end] {
                    b'0'..=b'9' | b'-' | b'+' => end += 1,
                    _ => break,
                }
            }

            if end == i {
                return ERR_INVALID_NUMBER;
            }

            use lexical_parse_integer::FromLexical;
            match i64::from_lexical(&bytes[i..end]) {
                Ok(val) => vec.push(val),
                Err(_) => return ERR_INVALID_NUMBER,
            }
            i = end;

            skip_ws_inline(bytes, &mut i);
            if i >= bytes.len() {
                return ERR_EXPECTED_COMMA_OR_END;
            }
            match bytes[i] {
                b',' => {
                    i += 1;
                    skip_ws_inline(bytes, &mut i);
                }
                b']' => {
                    std::ptr::write(out, vec);
                    return (pos + i + 1) as isize;
                }
                _ => return ERR_EXPECTED_COMMA_OR_END,
            }
        }
    }
}

/// Parse `Vec<u64>`, write to *out, return new position or error.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jitson_parse_vec_u64(
    input: *const u8,
    len: usize,
    pos: usize,
    out: *mut Vec<u64>,
) -> ParseResult {
    unsafe {
        let bytes = slice(input, len, pos);
        let mut i = 0;

        if bytes.is_empty() || bytes[0] != b'[' {
            return ERR_EXPECTED_ARRAY_START;
        }
        i += 1;
        skip_ws_inline(bytes, &mut i);

        let mut vec = Vec::with_capacity(16);

        if i < bytes.len() && bytes[i] == b']' {
            std::ptr::write(out, vec);
            return (pos + i + 1) as isize;
        }

        loop {
            // Parse number directly
            let mut end = i;
            while end < bytes.len() {
                match bytes[end] {
                    b'0'..=b'9' | b'+' => end += 1,
                    _ => break,
                }
            }

            if end == i {
                return ERR_INVALID_NUMBER;
            }

            use lexical_parse_integer::FromLexical;
            match u64::from_lexical(&bytes[i..end]) {
                Ok(val) => vec.push(val),
                Err(_) => return ERR_INVALID_NUMBER,
            }
            i = end;

            skip_ws_inline(bytes, &mut i);
            if i >= bytes.len() {
                return ERR_EXPECTED_COMMA_OR_END;
            }
            match bytes[i] {
                b',' => {
                    i += 1;
                    skip_ws_inline(bytes, &mut i);
                }
                b']' => {
                    std::ptr::write(out, vec);
                    return (pos + i + 1) as isize;
                }
                _ => return ERR_EXPECTED_COMMA_OR_END,
            }
        }
    }
}

/// Parse `Vec<bool>`, write to *out, return new position or error.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jitson_parse_vec_bool(
    input: *const u8,
    len: usize,
    pos: usize,
    out: *mut Vec<bool>,
) -> ParseResult {
    unsafe {
        let bytes = slice(input, len, pos);
        let mut i = 0;

        if bytes.is_empty() || bytes[0] != b'[' {
            return ERR_EXPECTED_ARRAY_START;
        }
        i += 1;
        skip_ws_inline(bytes, &mut i);

        let mut vec = Vec::with_capacity(16);

        if i < bytes.len() && bytes[i] == b']' {
            std::ptr::write(out, vec);
            return (pos + i + 1) as isize;
        }

        loop {
            // Parse boolean
            if i + 4 <= bytes.len() && &bytes[i..i + 4] == b"true" {
                vec.push(true);
                i += 4;
            } else if i + 5 <= bytes.len() && &bytes[i..i + 5] == b"false" {
                vec.push(false);
                i += 5;
            } else {
                return ERR_INVALID_BOOL;
            }

            skip_ws_inline(bytes, &mut i);
            if i >= bytes.len() {
                return ERR_EXPECTED_COMMA_OR_END;
            }
            match bytes[i] {
                b',' => {
                    i += 1;
                    skip_ws_inline(bytes, &mut i);
                }
                b']' => {
                    std::ptr::write(out, vec);
                    return (pos + i + 1) as isize;
                }
                _ => return ERR_EXPECTED_COMMA_OR_END,
            }
        }
    }
}

/// Parse `Vec<String>`, write to *out, return new position or error.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jitson_parse_vec_string(
    input: *const u8,
    len: usize,
    pos: usize,
    out: *mut Vec<String>,
) -> ParseResult {
    unsafe {
        let bytes = slice(input, len, pos);
        let mut i = 0;

        if bytes.is_empty() || bytes[0] != b'[' {
            return ERR_EXPECTED_ARRAY_START;
        }
        i += 1;
        skip_ws_inline(bytes, &mut i);

        let mut vec = Vec::with_capacity(16);

        if i < bytes.len() && bytes[i] == b']' {
            std::ptr::write(out, vec);
            return (pos + i + 1) as isize;
        }

        loop {
            // Parse string
            if i >= bytes.len() || bytes[i] != b'"' {
                return ERR_INVALID_STRING;
            }
            i += 1;

            let content = &bytes[i..];
            match memchr::memchr2(b'"', b'\\', content) {
                Some(offset) => {
                    if content[offset] == b'"' {
                        // No escapes
                        let s = std::str::from_utf8_unchecked(&content[..offset]);
                        vec.push(s.to_owned());
                        i += offset + 1;
                    } else {
                        // Has escapes - use slow path
                        match find_string_end_with_escapes(content, offset) {
                            Some(end) => {
                                vec.push(decode_escaped_string(&content[..end]));
                                i += end + 1;
                            }
                            None => return ERR_UNEXPECTED_EOF,
                        }
                    }
                }
                None => return ERR_UNEXPECTED_EOF,
            }

            skip_ws_inline(bytes, &mut i);
            if i >= bytes.len() {
                return ERR_EXPECTED_COMMA_OR_END;
            }
            match bytes[i] {
                b',' => {
                    i += 1;
                    skip_ws_inline(bytes, &mut i);
                }
                b']' => {
                    std::ptr::write(out, vec);
                    return (pos + i + 1) as isize;
                }
                _ => return ERR_EXPECTED_COMMA_OR_END,
            }
        }
    }
}

/// Parse `Vec<Vec<f64>>`, write to *out, return new position or error.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jitson_parse_vec_vec_f64(
    input: *const u8,
    len: usize,
    pos: usize,
    out: *mut Vec<Vec<f64>>,
) -> ParseResult {
    unsafe {
        let bytes = slice(input, len, pos);
        let mut i = 0;

        if bytes.is_empty() || bytes[0] != b'[' {
            return ERR_EXPECTED_ARRAY_START;
        }
        i += 1;
        skip_ws_inline(bytes, &mut i);

        let mut outer = Vec::with_capacity(16);

        if i < bytes.len() && bytes[i] == b']' {
            std::ptr::write(out, outer);
            return (pos + i + 1) as isize;
        }

        loop {
            // Parse inner array
            if i >= bytes.len() || bytes[i] != b'[' {
                return ERR_EXPECTED_ARRAY_START;
            }
            i += 1;
            skip_ws_inline(bytes, &mut i);

            let mut inner = Vec::with_capacity(16);

            if i < bytes.len() && bytes[i] == b']' {
                i += 1;
            } else {
                loop {
                    let mut end = i;
                    while end < bytes.len() {
                        match bytes[end] {
                            b'0'..=b'9' | b'-' | b'+' | b'.' | b'e' | b'E' => end += 1,
                            _ => break,
                        }
                    }

                    if end == i {
                        return ERR_INVALID_NUMBER;
                    }

                    use lexical_parse_float::FromLexical;
                    match f64::from_lexical(&bytes[i..end]) {
                        Ok(val) => inner.push(val),
                        Err(_) => return ERR_INVALID_NUMBER,
                    }
                    i = end;

                    skip_ws_inline(bytes, &mut i);
                    if i >= bytes.len() {
                        return ERR_EXPECTED_COMMA_OR_END;
                    }
                    match bytes[i] {
                        b',' => {
                            i += 1;
                            skip_ws_inline(bytes, &mut i);
                        }
                        b']' => {
                            i += 1;
                            break;
                        }
                        _ => return ERR_EXPECTED_COMMA_OR_END,
                    }
                }
            }

            outer.push(inner);

            skip_ws_inline(bytes, &mut i);
            if i >= bytes.len() {
                return ERR_EXPECTED_COMMA_OR_END;
            }
            match bytes[i] {
                b',' => {
                    i += 1;
                    skip_ws_inline(bytes, &mut i);
                }
                b']' => {
                    std::ptr::write(out, outer);
                    return (pos + i + 1) as isize;
                }
                _ => return ERR_EXPECTED_COMMA_OR_END,
            }
        }
    }
}

/// Parse `Vec<Vec<Vec<f64>>>`, write to *out, return new position or error.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jitson_parse_vec_vec_vec_f64(
    input: *const u8,
    len: usize,
    pos: usize,
    out: *mut Vec<Vec<Vec<f64>>>,
) -> ParseResult {
    unsafe {
        let bytes = slice(input, len, pos);
        let mut i = 0;

        if bytes.is_empty() || bytes[0] != b'[' {
            return ERR_EXPECTED_ARRAY_START;
        }
        i += 1;
        skip_ws_inline(bytes, &mut i);

        let mut outer = Vec::with_capacity(16);

        if i < bytes.len() && bytes[i] == b']' {
            std::ptr::write(out, outer);
            return (pos + i + 1) as isize;
        }

        loop {
            // Parse middle array
            if i >= bytes.len() || bytes[i] != b'[' {
                return ERR_EXPECTED_ARRAY_START;
            }
            i += 1;
            skip_ws_inline(bytes, &mut i);

            let mut middle = Vec::with_capacity(16);

            if i < bytes.len() && bytes[i] == b']' {
                i += 1;
            } else {
                loop {
                    // Parse inner array
                    if i >= bytes.len() || bytes[i] != b'[' {
                        return ERR_EXPECTED_ARRAY_START;
                    }
                    i += 1;
                    skip_ws_inline(bytes, &mut i);

                    let mut inner = Vec::with_capacity(16);

                    if i < bytes.len() && bytes[i] == b']' {
                        i += 1;
                    } else {
                        loop {
                            let mut end = i;
                            while end < bytes.len() {
                                match bytes[end] {
                                    b'0'..=b'9' | b'-' | b'+' | b'.' | b'e' | b'E' => end += 1,
                                    _ => break,
                                }
                            }

                            if end == i {
                                return ERR_INVALID_NUMBER;
                            }

                            use lexical_parse_float::FromLexical;
                            match f64::from_lexical(&bytes[i..end]) {
                                Ok(val) => inner.push(val),
                                Err(_) => return ERR_INVALID_NUMBER,
                            }
                            i = end;

                            skip_ws_inline(bytes, &mut i);
                            if i >= bytes.len() {
                                return ERR_EXPECTED_COMMA_OR_END;
                            }
                            match bytes[i] {
                                b',' => {
                                    i += 1;
                                    skip_ws_inline(bytes, &mut i);
                                }
                                b']' => {
                                    i += 1;
                                    break;
                                }
                                _ => return ERR_EXPECTED_COMMA_OR_END,
                            }
                        }
                    }

                    middle.push(inner);

                    skip_ws_inline(bytes, &mut i);
                    if i >= bytes.len() {
                        return ERR_EXPECTED_COMMA_OR_END;
                    }
                    match bytes[i] {
                        b',' => {
                            i += 1;
                            skip_ws_inline(bytes, &mut i);
                        }
                        b']' => {
                            i += 1;
                            break;
                        }
                        _ => return ERR_EXPECTED_COMMA_OR_END,
                    }
                }
            }

            outer.push(middle);

            skip_ws_inline(bytes, &mut i);
            if i >= bytes.len() {
                return ERR_EXPECTED_COMMA_OR_END;
            }
            match bytes[i] {
                b',' => {
                    i += 1;
                    skip_ws_inline(bytes, &mut i);
                }
                b']' => {
                    std::ptr::write(out, outer);
                    return (pos + i + 1) as isize;
                }
                _ => return ERR_EXPECTED_COMMA_OR_END,
            }
        }
    }
}

/// Parse `Vec<Struct>` using a provided deserializer function.
/// elem_size and elem_align describe the struct layout.
/// func_ptr is the JIT-compiled deserializer for the element type.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jitson_parse_vec_struct(
    input: *const u8,
    len: usize,
    pos: usize,
    out: *mut u8, // Actually *mut Vec<T>, but we don't know T
    elem_size: usize,
    elem_align: usize,
    func_ptr: *const u8,
) -> ParseResult {
    unsafe {
        // The func_ptr is a compiled deserializer: fn(input, len, pos, out) -> isize
        type DeserFn = unsafe extern "C" fn(*const u8, usize, usize, *mut u8) -> isize;
        let deser: DeserFn = std::mem::transmute(func_ptr);

        let bytes = slice(input, len, pos);
        let mut i = 0;

        if bytes.is_empty() || bytes[0] != b'[' {
            return ERR_EXPECTED_ARRAY_START;
        }
        i += 1;
        skip_ws_inline(bytes, &mut i);

        // Build a Vec manually with proper layout
        let mut capacity = 16usize;
        let mut len_items = 0usize;
        let mut data = if elem_size > 0 {
            std::alloc::alloc(
                std::alloc::Layout::from_size_align(elem_size * capacity, elem_align).unwrap(),
            )
        } else {
            std::ptr::null_mut()
        };

        if i < bytes.len() && bytes[i] == b']' {
            // Empty array
            let vec_ptr = out as *mut (usize, *mut u8, usize);
            std::ptr::write(vec_ptr, (capacity, data, 0));
            return (pos + i + 1) as isize;
        }

        loop {
            // Grow if needed
            if len_items >= capacity {
                let new_cap = capacity * 2;
                let new_data = std::alloc::alloc(
                    std::alloc::Layout::from_size_align(elem_size * new_cap, elem_align).unwrap(),
                );
                if elem_size > 0 && !data.is_null() {
                    std::ptr::copy_nonoverlapping(data, new_data, elem_size * len_items);
                    std::alloc::dealloc(
                        data,
                        std::alloc::Layout::from_size_align(elem_size * capacity, elem_align)
                            .unwrap(),
                    );
                }
                data = new_data;
                capacity = new_cap;
            }

            // Parse element
            let elem_ptr = data.add(len_items * elem_size);
            let result = deser(input, len, pos + i, elem_ptr);
            if result < 0 {
                // Clean up on error
                if elem_size > 0 && !data.is_null() {
                    std::alloc::dealloc(
                        data,
                        std::alloc::Layout::from_size_align(elem_size * capacity, elem_align)
                            .unwrap(),
                    );
                }
                return result;
            }
            i = (result as usize) - pos;
            len_items += 1;

            skip_ws_inline(bytes, &mut i);
            if i >= bytes.len() {
                if elem_size > 0 && !data.is_null() {
                    std::alloc::dealloc(
                        data,
                        std::alloc::Layout::from_size_align(elem_size * capacity, elem_align)
                            .unwrap(),
                    );
                }
                return ERR_EXPECTED_COMMA_OR_END;
            }
            match bytes[i] {
                b',' => {
                    i += 1;
                    skip_ws_inline(bytes, &mut i);
                }
                b']' => {
                    // Write the Vec
                    let vec_ptr = out as *mut (usize, *mut u8, usize);
                    std::ptr::write(vec_ptr, (capacity, data, len_items));
                    return (pos + i + 1) as isize;
                }
                _ => {
                    if elem_size > 0 && !data.is_null() {
                        std::alloc::dealloc(
                            data,
                            std::alloc::Layout::from_size_align(elem_size * capacity, elem_align)
                                .unwrap(),
                        );
                    }
                    return ERR_EXPECTED_COMMA_OR_END;
                }
            }
        }
    }
}

/// Parse a nested struct by calling its compiled deserializer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jitson_parse_nested_struct(
    input: *const u8,
    len: usize,
    pos: usize,
    out: *mut u8,
    func_ptr: *const u8,
) -> ParseResult {
    unsafe {
        type DeserFn = unsafe extern "C" fn(*const u8, usize, usize, *mut u8) -> isize;
        let deser: DeserFn = std::mem::transmute(func_ptr);
        deser(input, len, pos, out)
    }
}

/// Parse an `Option<T>` field.
///
/// This uses facet's OptionVTable to properly initialize the Option.
/// - If JSON value is `null`, initializes as None
/// - Otherwise, parses the inner value and initializes as Some(value)
///
/// Parameters:
/// - input, len, pos: standard JSON input parameters
/// - out: pointer to uninitialized `Option<T>`
/// - option_shape: pointer to the Shape of `Option<T>` (used to get vtable and inner type)
/// - inner_deser_fn: optional function pointer to deserialize inner value (can be null for fallback)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jitson_init_option_none(
    out: *mut u8,
    option_shape: *const facet_core::Shape,
) {
    unsafe {
        use facet_core::{Def, PtrUninit};

        let shape = &*option_shape;
        let Def::Option(option_def) = shape.def else {
            return;
        };

        let vtable = option_def.vtable;
        let out_uninit = PtrUninit::new(out);
        (vtable.init_none)(out_uninit);
    }
}

pub unsafe extern "C" fn jitson_parse_option(
    input: *const u8,
    len: usize,
    pos: usize,
    out: *mut u8,
    option_shape: *const facet_core::Shape,
    inner_deser_fn: *const u8, // nullable - if null, we dispatch on inner type
) -> ParseResult {
    unsafe {
        use facet_core::{
            Def, NumericType, PrimitiveType, PtrConst, PtrMut, PtrUninit, Type, UserType,
        };

        let bytes = slice(input, len, pos);
        let mut i = 0;

        // Skip whitespace
        skip_ws_inline(bytes, &mut i);

        if i >= bytes.len() {
            return ERR_UNEXPECTED_EOF;
        }

        // Get the OptionDef from the shape
        let shape = &*option_shape;
        let Def::Option(option_def) = shape.def else {
            // This shouldn't happen if called correctly
            return ERR_UNEXPECTED_EOF;
        };

        let vtable = option_def.vtable;
        let inner_shape = option_def.t();

        // Check for null
        if bytes.len() - i >= 4 && &bytes[i..i + 4] == b"null" {
            // Initialize as None
            let out_uninit = PtrUninit::new(out);
            (vtable.init_none)(out_uninit);
            return (pos + i + 4) as isize;
        }

        // Not null - need to parse the inner value
        // Get the inner type's size and alignment for allocation
        let inner_layout = inner_shape
            .layout
            .sized_layout()
            .expect("Option inner type must be sized");

        // Allocate temporary buffer for inner value
        let inner_buf = if inner_layout.size() > 0 {
            std::alloc::alloc(std::alloc::Layout::from_size_align_unchecked(
                inner_layout.size(),
                inner_layout.align(),
            ))
        } else {
            // For ZSTs, use a non-null dangling pointer
            inner_layout.align() as *mut u8
        };

        // Parse the inner value based on the inner type
        let inner_result = if !inner_deser_fn.is_null() {
            // Use JIT-compiled deserializer for inner value (for structs)
            type DeserFn = unsafe extern "C" fn(*const u8, usize, usize, *mut u8) -> isize;
            let deser: DeserFn = std::mem::transmute(inner_deser_fn);
            deser(input, len, pos + i, inner_buf)
        } else {
            // Dispatch based on inner type
            match &inner_shape.ty {
                Type::Primitive(PrimitiveType::Numeric(NumericType::Float)) => {
                    match inner_layout.size() {
                        8 => jitson_parse_f64(input, len, pos + i, inner_buf.cast()),
                        4 => jitson_parse_f32(input, len, pos + i, inner_buf.cast()),
                        _ => ERR_UNEXPECTED_EOF,
                    }
                }
                Type::Primitive(PrimitiveType::Numeric(NumericType::Integer { signed: true })) => {
                    match inner_layout.size() {
                        8 => jitson_parse_i64(input, len, pos + i, inner_buf.cast()),
                        4 => jitson_parse_i32(input, len, pos + i, inner_buf.cast()),
                        2 => jitson_parse_i16(input, len, pos + i, inner_buf.cast()),
                        1 => jitson_parse_i8(input, len, pos + i, inner_buf.cast()),
                        _ => ERR_UNEXPECTED_EOF,
                    }
                }
                Type::Primitive(PrimitiveType::Numeric(NumericType::Integer { signed: false })) => {
                    match inner_layout.size() {
                        8 => jitson_parse_u64(input, len, pos + i, inner_buf.cast()),
                        4 => jitson_parse_u32(input, len, pos + i, inner_buf.cast()),
                        2 => jitson_parse_u16(input, len, pos + i, inner_buf.cast()),
                        1 => jitson_parse_u8(input, len, pos + i, inner_buf.cast()),
                        _ => ERR_UNEXPECTED_EOF,
                    }
                }
                Type::Primitive(PrimitiveType::Boolean) => {
                    jitson_parse_bool(input, len, pos + i, inner_buf.cast())
                }
                Type::User(UserType::Opaque) if inner_shape.type_identifier == "String" => {
                    jitson_parse_string(input, len, pos + i, inner_buf.cast())
                }
                Type::User(UserType::Opaque) if inner_shape.type_identifier == "Box" => {
                    // Handle Box<T> - parse the inner value and box it
                    if let Def::Pointer(ptr_def) = inner_shape.def {
                        if let Some(pointee_shape) = ptr_def.pointee {
                            if let Some(new_into_fn) = ptr_def.vtable.new_into_fn {
                                let pointee_layout = pointee_shape
                                    .layout
                                    .sized_layout()
                                    .expect("Box pointee must be sized");

                                // Allocate temp buffer for pointee
                                let pointee_buf = if pointee_layout.size() > 0 {
                                    std::alloc::alloc(
                                        std::alloc::Layout::from_size_align_unchecked(
                                            pointee_layout.size(),
                                            pointee_layout.align(),
                                        ),
                                    )
                                } else {
                                    pointee_layout.align() as *mut u8
                                };

                                // Recursively parse inner value - reuse jitson_parse_option if inner is Option
                                let pointee_result = if let Def::Option(_) = pointee_shape.def {
                                    jitson_parse_option(
                                        input,
                                        len,
                                        pos + i,
                                        pointee_buf,
                                        pointee_shape,
                                        std::ptr::null(),
                                    )
                                } else {
                                    // Try to parse as primitive or fail
                                    match &pointee_shape.ty {
                                        Type::Primitive(PrimitiveType::Numeric(
                                            NumericType::Integer { signed: false },
                                        )) => match pointee_layout.size() {
                                            8 => jitson_parse_u64(
                                                input,
                                                len,
                                                pos + i,
                                                pointee_buf.cast(),
                                            ),
                                            4 => jitson_parse_u32(
                                                input,
                                                len,
                                                pos + i,
                                                pointee_buf.cast(),
                                            ),
                                            2 => jitson_parse_u16(
                                                input,
                                                len,
                                                pos + i,
                                                pointee_buf.cast(),
                                            ),
                                            1 => jitson_parse_u8(
                                                input,
                                                len,
                                                pos + i,
                                                pointee_buf.cast(),
                                            ),
                                            _ => ERR_UNEXPECTED_EOF,
                                        },
                                        Type::Primitive(PrimitiveType::Numeric(
                                            NumericType::Integer { signed: true },
                                        )) => match pointee_layout.size() {
                                            8 => jitson_parse_i64(
                                                input,
                                                len,
                                                pos + i,
                                                pointee_buf.cast(),
                                            ),
                                            4 => jitson_parse_i32(
                                                input,
                                                len,
                                                pos + i,
                                                pointee_buf.cast(),
                                            ),
                                            2 => jitson_parse_i16(
                                                input,
                                                len,
                                                pos + i,
                                                pointee_buf.cast(),
                                            ),
                                            1 => jitson_parse_i8(
                                                input,
                                                len,
                                                pos + i,
                                                pointee_buf.cast(),
                                            ),
                                            _ => ERR_UNEXPECTED_EOF,
                                        },
                                        Type::Primitive(PrimitiveType::Numeric(
                                            NumericType::Float,
                                        )) => match pointee_layout.size() {
                                            8 => jitson_parse_f64(
                                                input,
                                                len,
                                                pos + i,
                                                pointee_buf.cast(),
                                            ),
                                            4 => jitson_parse_f32(
                                                input,
                                                len,
                                                pos + i,
                                                pointee_buf.cast(),
                                            ),
                                            _ => ERR_UNEXPECTED_EOF,
                                        },
                                        Type::Primitive(PrimitiveType::Boolean) => {
                                            jitson_parse_bool(
                                                input,
                                                len,
                                                pos + i,
                                                pointee_buf.cast(),
                                            )
                                        }
                                        Type::User(UserType::Opaque)
                                            if pointee_shape.type_identifier == "String" =>
                                        {
                                            jitson_parse_string(
                                                input,
                                                len,
                                                pos + i,
                                                pointee_buf.cast(),
                                            )
                                        }
                                        _ => ERR_UNEXPECTED_EOF,
                                    }
                                };

                                if pointee_result < 0 {
                                    if pointee_layout.size() > 0 {
                                        std::alloc::dealloc(
                                            pointee_buf,
                                            std::alloc::Layout::from_size_align_unchecked(
                                                pointee_layout.size(),
                                                pointee_layout.align(),
                                            ),
                                        );
                                    }
                                    // Cleanup outer buffer too
                                    if inner_layout.size() > 0 {
                                        std::alloc::dealloc(
                                            inner_buf,
                                            std::alloc::Layout::from_size_align_unchecked(
                                                inner_layout.size(),
                                                inner_layout.align(),
                                            ),
                                        );
                                    }
                                    return pointee_result;
                                }

                                // Create the Box using new_into_fn
                                let inner_uninit = PtrUninit::new(inner_buf);
                                let pointee_ptr = PtrMut::new(pointee_buf);
                                new_into_fn(inner_uninit, pointee_ptr);

                                // Deallocate pointee buffer (value moved into Box)
                                if pointee_layout.size() > 0 {
                                    std::alloc::dealloc(
                                        pointee_buf,
                                        std::alloc::Layout::from_size_align_unchecked(
                                            pointee_layout.size(),
                                            pointee_layout.align(),
                                        ),
                                    );
                                }

                                pointee_result
                            } else {
                                ERR_UNEXPECTED_EOF
                            }
                        } else {
                            ERR_UNEXPECTED_EOF
                        }
                    } else {
                        ERR_UNEXPECTED_EOF
                    }
                }
                Type::User(UserType::Struct(_)) => {
                    // Look up pre-compiled deserializer from cache
                    if let Some(func) = crate::cranelift::cache::get_by_shape(inner_shape) {
                        type DeserFn =
                            unsafe extern "C" fn(*const u8, usize, usize, *mut u8) -> isize;
                        let deser: DeserFn = std::mem::transmute(func.ptr());
                        deser(input, len, pos + i, inner_buf)
                    } else {
                        if inner_layout.size() > 0 {
                            std::alloc::dealloc(
                                inner_buf,
                                std::alloc::Layout::from_size_align_unchecked(
                                    inner_layout.size(),
                                    inner_layout.align(),
                                ),
                            );
                        }
                        return ERR_UNEXPECTED_EOF;
                    }
                }
                _ => {
                    // Check if it's a nested Option
                    if let Def::Option(_) = inner_shape.def {
                        // Recursively handle nested Option
                        jitson_parse_option(
                            input,
                            len,
                            pos + i,
                            inner_buf,
                            inner_shape,
                            std::ptr::null(),
                        )
                    } else {
                        // Unsupported inner type - cleanup and fail
                        if inner_layout.size() > 0 {
                            std::alloc::dealloc(
                                inner_buf,
                                std::alloc::Layout::from_size_align_unchecked(
                                    inner_layout.size(),
                                    inner_layout.align(),
                                ),
                            );
                        }
                        return ERR_UNEXPECTED_EOF;
                    }
                }
            }
        };

        if inner_result < 0 {
            // Cleanup on error
            if inner_layout.size() > 0 {
                std::alloc::dealloc(
                    inner_buf,
                    std::alloc::Layout::from_size_align_unchecked(
                        inner_layout.size(),
                        inner_layout.align(),
                    ),
                );
            }
            return inner_result;
        }

        // Initialize Option as Some with the parsed inner value
        let out_uninit = PtrUninit::new(out);
        let inner_ptr = PtrConst::new(inner_buf as *const u8);
        (vtable.init_some)(out_uninit, inner_ptr);

        // Deallocate the temporary buffer (value has been moved into Option)
        if inner_layout.size() > 0 {
            std::alloc::dealloc(
                inner_buf,
                std::alloc::Layout::from_size_align_unchecked(
                    inner_layout.size(),
                    inner_layout.align(),
                ),
            );
        }

        inner_result
    }
}

// =============================================================================
// Helper registration for Cranelift
// =============================================================================

/// Register all helper functions with a JITBuilder.
pub fn register_helpers(builder: &mut cranelift_jit::JITBuilder) {
    builder.symbol("jitson_parse_f64", jitson_parse_f64 as *const u8);
    builder.symbol("jitson_parse_f32", jitson_parse_f32 as *const u8);
    builder.symbol("jitson_parse_i64", jitson_parse_i64 as *const u8);
    builder.symbol("jitson_parse_i32", jitson_parse_i32 as *const u8);
    builder.symbol("jitson_parse_i16", jitson_parse_i16 as *const u8);
    builder.symbol("jitson_parse_i8", jitson_parse_i8 as *const u8);
    builder.symbol("jitson_parse_u64", jitson_parse_u64 as *const u8);
    builder.symbol("jitson_parse_u32", jitson_parse_u32 as *const u8);
    builder.symbol("jitson_parse_u16", jitson_parse_u16 as *const u8);
    builder.symbol("jitson_parse_u8", jitson_parse_u8 as *const u8);
    builder.symbol("jitson_parse_bool", jitson_parse_bool as *const u8);
    builder.symbol("jitson_parse_string", jitson_parse_string as *const u8);
    builder.symbol("jitson_parse_vec_f64", jitson_parse_vec_f64 as *const u8);
    builder.symbol("jitson_parse_vec_i64", jitson_parse_vec_i64 as *const u8);
    builder.symbol("jitson_parse_vec_u64", jitson_parse_vec_u64 as *const u8);
    builder.symbol("jitson_parse_vec_bool", jitson_parse_vec_bool as *const u8);
    builder.symbol(
        "jitson_parse_vec_string",
        jitson_parse_vec_string as *const u8,
    );
    builder.symbol(
        "jitson_parse_vec_vec_f64",
        jitson_parse_vec_vec_f64 as *const u8,
    );
    builder.symbol(
        "jitson_parse_vec_vec_vec_f64",
        jitson_parse_vec_vec_vec_f64 as *const u8,
    );
    builder.symbol(
        "jitson_parse_vec_struct",
        jitson_parse_vec_struct as *const u8,
    );
    builder.symbol(
        "jitson_parse_nested_struct",
        jitson_parse_nested_struct as *const u8,
    );
    builder.symbol("jitson_parse_option", jitson_parse_option as *const u8);
    builder.symbol(
        "jitson_init_option_none",
        jitson_init_option_none as *const u8,
    );
    builder.symbol("jitson_skip_value", jitson_skip_value as *const u8);
}
