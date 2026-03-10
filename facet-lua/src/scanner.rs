//! Low-level scanning utilities for Lua table constructor syntax.
//!
//! This module contains token-boundary finding, whitespace/comment skipping,
//! escape lookup, and value classification — all stateless functions and types
//! that the parser uses without semantic interpretation.

/// Keywords that open a block requiring a matching `end`.
/// Note: `do` is intentionally excluded — in `for/while ... do ... end`, the
/// `for`/`while` already counts as the opener. Standalone `do...end` blocks
/// in table constructors are vanishingly rare.
pub(crate) const BLOCK_OPENERS: &[&str] = &["function", "if", "for", "while"];

/// Count the `=` signs in a long-bracket opener starting at `input[pos]`.
/// Returns `Some(level)` if `input[pos..]` starts with `[=*[`, else `None`.
pub(crate) fn match_long_bracket_open(input: &[u8], pos: usize) -> Option<usize> {
    if input.get(pos).copied() != Some(b'[') {
        return None;
    }
    let mut p = pos + 1;
    while p < input.len() && input[p] == b'=' {
        p += 1;
    }
    if input.get(p).copied() == Some(b'[') {
        Some(p - pos - 1) // number of '=' signs
    } else {
        None
    }
}

/// Find the end of a long-bracket string/comment body.
/// `pos` should point just past the opening `[=*[`.
/// Returns the position just past the closing `]=*]`.
pub(crate) fn find_long_bracket_close(input: &[u8], mut pos: usize, level: usize) -> Option<usize> {
    while pos < input.len() {
        if input[pos] == b']' {
            let mut eq = 0;
            while pos + 1 + eq < input.len() && input[pos + 1 + eq] == b'=' {
                eq += 1;
            }
            if eq == level && input.get(pos + 1 + eq).copied() == Some(b']') {
                return Some(pos + 2 + level);
            }
        }
        pos += 1;
    }
    None
}

/// Skip whitespace and comments starting at `pos`. Returns the new position.
pub(crate) fn skip_ws(input: &[u8], mut pos: usize) -> usize {
    while pos < input.len() {
        match input[pos] {
            b' ' | b'\t' | b'\n' | b'\r' => pos += 1,
            b'-' if input.get(pos + 1).copied() == Some(b'-') => {
                pos += 2;
                // Block comment: --[=*[...]=*]
                if let Some(level) = match_long_bracket_open(input, pos) {
                    let opener_len = 2 + level; // `[` + `=`*level + `[`
                    pos = find_long_bracket_close(input, pos + opener_len, level)
                        .unwrap_or(input.len());
                } else {
                    // Line comment: skip to end of line
                    while pos < input.len() && input[pos] != b'\n' {
                        pos += 1;
                    }
                }
            }
            _ => break,
        }
    }
    pos
}

/// Look up a simple single-character Lua escape sequence.
pub(crate) fn simple_escape(b: u8) -> Option<char> {
    match b {
        b'"' | b'\'' | b'\\' => Some(b as char),
        b'n' => Some('\n'),
        b'r' => Some('\r'),
        b't' => Some('\t'),
        b'a' => Some('\x07'),
        b'b' => Some('\x08'),
        b'f' => Some('\x0C'),
        b'v' => Some('\x0B'),
        _ => None,
    }
}

/// Determine the expected byte length of a UTF-8 character from its leading byte.
pub(crate) fn utf8_char_len(b: u8) -> usize {
    if b < 0x80 {
        1
    } else if b < 0xE0 {
        2
    } else if b < 0xF0 {
        3
    } else {
        4
    }
}

/// Byte ranges for a scanned hex number (before value computation).
pub(crate) struct HexExtent {
    pub start: usize,
    pub hex_start: usize,
    pub int_end: usize,
    pub frac_end: usize,
    pub has_dot: bool,
    pub has_exp: bool,
}

/// Classification of the next value token, used to DRY the dispatch
/// between `parse_value` and `skip_value_raw`.
pub(crate) enum ValueStart {
    String,
    Table,
    Negative,
    NaN,
    Number,
    Identifier,
}
