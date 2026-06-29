//! Safe test host for compiled Tree-sitter external scanner fixtures.

use std::{
    ffi::{c_char, c_uint, c_void},
    ptr,
};

/// Pinned CSS scanner source compiled by this crate.
pub const CSS_SCANNER_SOURCE: &str =
    include_str!("../../snark/tests/fixtures/packages/tree-sitter-css-reduced/src/scanner.c");

const SERIALIZATION_BUFFER_SIZE: usize = 1024;
const CSS_EXTERNAL_SYMBOL_COUNT: usize = 3;

unsafe extern "C" {
    fn tree_sitter_css_external_scanner_create() -> *mut c_void;
    fn tree_sitter_css_external_scanner_destroy(payload: *mut c_void);
    fn tree_sitter_css_external_scanner_scan(
        payload: *mut c_void,
        lexer: *mut TSLexer,
        valid_symbols: *const bool,
    ) -> bool;
    fn tree_sitter_css_external_scanner_serialize(
        payload: *mut c_void,
        buffer: *mut c_char,
    ) -> c_uint;
    fn tree_sitter_css_external_scanner_deserialize(
        payload: *mut c_void,
        buffer: *const c_char,
        length: c_uint,
    );
}

/// Compiled scanner host for the pinned reduced CSS fixture.
pub struct CssScanner {
    payload: *mut c_void,
}

impl CssScanner {
    /// Create a scanner payload.
    pub fn new() -> Self {
        let payload = unsafe { tree_sitter_css_external_scanner_create() };
        Self { payload }
    }

    /// Run one scanner call from `byte_position` with Tree-sitter valid-symbol bits.
    pub fn scan(
        &mut self,
        input: &str,
        byte_position: usize,
        valid_symbols: &[bool],
        snapshot: &[u8],
    ) -> Result<CssScan, CssScanError> {
        if valid_symbols.len() < CSS_EXTERNAL_SYMBOL_COUNT {
            return Err(CssScanError::ValidSymbolMaskTooShort {
                len: valid_symbols.len(),
                required: CSS_EXTERNAL_SYMBOL_COUNT,
            });
        }
        let snapshot_len = u32::try_from(snapshot.len())
            .expect("scanner snapshot length must fit Tree-sitter ABI");
        unsafe {
            tree_sitter_css_external_scanner_deserialize(
                self.payload,
                snapshot.as_ptr().cast::<c_char>(),
                snapshot_len,
            );
        }

        let mut host = LexerHost::new(input, byte_position);
        let accepted = unsafe {
            tree_sitter_css_external_scanner_scan(
                self.payload,
                &mut host.lexer,
                valid_symbols.as_ptr(),
            )
        };
        let mut buffer = [0u8; SERIALIZATION_BUFFER_SIZE];
        let serialized_len = unsafe {
            tree_sitter_css_external_scanner_serialize(
                self.payload,
                buffer.as_mut_ptr().cast::<c_char>(),
            )
        } as usize;
        let serialized_state = buffer[..serialized_len].to_vec();
        let result_symbol = accepted.then_some(host.lexer.result_symbol as usize);
        let end_byte = if accepted {
            host.marked_end.unwrap_or(host.cursor)
        } else {
            byte_position
        };
        Ok(CssScan {
            accepted,
            result_symbol,
            end_byte,
            peek_byte: host.cursor,
            serialized_state,
        })
    }
}

impl Default for CssScanner {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for CssScanner {
    fn drop(&mut self) {
        unsafe {
            tree_sitter_css_external_scanner_destroy(self.payload);
        }
    }
}

/// Result of one compiled scanner call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CssScan {
    accepted: bool,
    result_symbol: Option<usize>,
    end_byte: usize,
    peek_byte: usize,
    serialized_state: Vec<u8>,
}

/// Invalid scanner host request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CssScanError {
    /// The reduced CSS scanner reads valid-symbol ordinals 0, 1, and 2.
    ValidSymbolMaskTooShort { len: usize, required: usize },
}

impl std::fmt::Display for CssScanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ValidSymbolMaskTooShort { len, required } => write!(
                f,
                "valid-symbol mask has length {len}, but reduced CSS scanner requires {required}"
            ),
        }
    }
}

impl std::error::Error for CssScanError {}

impl CssScan {
    /// Whether the scanner accepted an external token.
    pub const fn accepted(&self) -> bool {
        self.accepted
    }

    /// External scanner ordinal selected by the scanner.
    pub const fn result_symbol(&self) -> Option<usize> {
        self.result_symbol
    }

    /// Accepted token end byte after `mark_end` handling.
    pub const fn end_byte(&self) -> usize {
        self.end_byte
    }

    /// Scanner peek cursor after the call.
    pub const fn peek_byte(&self) -> usize {
        self.peek_byte
    }

    /// Serialized scanner state after the call.
    pub fn serialized_state(&self) -> &[u8] {
        &self.serialized_state
    }
}

#[repr(C)]
struct TSLexer {
    lookahead: i32,
    result_symbol: u16,
    advance: unsafe extern "C" fn(*mut TSLexer, bool),
    mark_end: unsafe extern "C" fn(*mut TSLexer),
    get_column: unsafe extern "C" fn(*mut TSLexer) -> u32,
    is_at_included_range_start: unsafe extern "C" fn(*const TSLexer) -> bool,
    eof: unsafe extern "C" fn(*const TSLexer) -> bool,
    log: *const c_void,
}

#[repr(C)]
struct LexerHost<'a> {
    lexer: TSLexer,
    input: &'a str,
    cursor: usize,
    marked_end: Option<usize>,
}

impl<'a> LexerHost<'a> {
    fn new(input: &'a str, cursor: usize) -> Self {
        let mut host = Self {
            lexer: TSLexer {
                lookahead: 0,
                result_symbol: 0,
                advance: lexer_advance,
                mark_end: lexer_mark_end,
                get_column: lexer_get_column,
                is_at_included_range_start: lexer_is_at_included_range_start,
                eof: lexer_eof,
                log: ptr::null(),
            },
            input,
            cursor,
            marked_end: None,
        };
        host.refresh_lookahead();
        host
    }

    fn refresh_lookahead(&mut self) {
        self.lexer.lookahead = self
            .input
            .get(self.cursor..)
            .and_then(|rest| rest.chars().next())
            .map_or(0, |ch| ch as i32);
    }
}

unsafe extern "C" fn lexer_advance(lexer: *mut TSLexer, _skip: bool) {
    let host = unsafe { &mut *(lexer.cast::<LexerHost<'_>>()) };
    if let Some(ch) = host.input[host.cursor..].chars().next() {
        host.cursor += ch.len_utf8();
    }
    host.refresh_lookahead();
}

unsafe extern "C" fn lexer_mark_end(lexer: *mut TSLexer) {
    let host = unsafe { &mut *(lexer.cast::<LexerHost<'_>>()) };
    host.marked_end = Some(host.cursor);
}

unsafe extern "C" fn lexer_get_column(lexer: *mut TSLexer) -> u32 {
    let host = unsafe { &mut *(lexer.cast::<LexerHost<'_>>()) };
    host.input[..host.cursor]
        .rsplit_once('\n')
        .map_or(host.cursor, |(_, tail)| tail.len()) as u32
}

unsafe extern "C" fn lexer_is_at_included_range_start(_lexer: *const TSLexer) -> bool {
    false
}

unsafe extern "C" fn lexer_eof(lexer: *const TSLexer) -> bool {
    let host = unsafe { &*(lexer.cast::<LexerHost<'_>>()) };
    host.cursor >= host.input.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_short_valid_symbol_masks_before_calling_c() {
        let mut scanner = CssScanner::new();
        let error = scanner.scan("", 0, &[], &[]).unwrap_err();

        assert_eq!(
            error,
            CssScanError::ValidSymbolMaskTooShort {
                len: 0,
                required: CSS_EXTERNAL_SYMBOL_COUNT,
            }
        );
    }
}
