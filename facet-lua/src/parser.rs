//! Parse Lua table constructor syntax into Rust values.

extern crate alloc;

use alloc::borrow::Cow;
use alloc::collections::VecDeque;
use alloc::string::String;
use alloc::vec::Vec;

use facet_format::{
    ContainerKind, DeserializeErrorKind, FieldKey, FieldLocationHint, FormatParser, ParseError,
    ParseEvent, ParseEventKind, SavePoint, ScalarValue,
};
use facet_reflect::Span;

use crate::consts;
use crate::scanner::{
    BLOCK_OPENERS, HexExtent, ValueStart, find_long_bracket_close, match_long_bracket_open,
    simple_escape, skip_ws, utf8_char_len,
};

/// A parser for Lua table constructor syntax.
///
/// Parses the subset of Lua that [`LuaSerializer`](crate::LuaSerializer) produces,
/// plus common Lua extensions:
/// - Table constructors: `{ ... }`
/// - Struct fields: `ident = value` or `["string"] = value`
/// - Sequence elements: bare values separated by `,`
/// - Scalars: `nil`, `true`/`false`, integers, floats, quoted strings, long strings
/// - Hex integers: `0xFF`, `0X1A` and hex floats: `0x1.8p1`
/// - Special floats: `math.huge`, `-math.huge`, `0/0`
/// - String escapes: `\n`, `\t`, `\a`, `\b`, `\f`, `\v`, `\xNN`, `\u{XXXX}`, `\z`, `\ddd`
/// - Separators: `,` and `;`
/// - Comments: `-- line` and `--[[ block ]]`
pub struct LuaParser<'de> {
    input: &'de [u8],
    pos: usize,
    state: ParserState<'de>,
    save_counter: u64,
    saved_states: Vec<(u64, ParserState<'de>, usize)>,
}

#[derive(Clone)]
struct ParserState<'de> {
    stack: Vec<ContextState>,
    event_peek: Option<ParseEvent<'de>>,
    root_started: bool,
    root_complete: bool,
    last_token_start: usize,
}

#[derive(Debug, Clone)]
enum ContextState {
    Struct(StructState),
    Seq(SeqState),
}

#[derive(Debug, Clone, Copy)]
enum StructState {
    /// Expecting a field key or `}`
    KeyOrEnd,
    /// Expecting the value for the current field (after `key =`)
    Value,
    /// Expecting `,` or `}`
    CommaOrEnd,
}

#[derive(Debug, Clone, Copy)]
enum SeqState {
    /// Expecting a value or `}`
    ValueOrEnd,
    /// Expecting `,` or `}`
    CommaOrEnd,
}

impl<'de> LuaParser<'de> {
    /// Create a new Lua parser from a string slice.
    pub fn new(input: &'de str) -> Self {
        Self {
            input: input.as_bytes(),
            pos: 0,
            state: ParserState {
                stack: Vec::new(),
                event_peek: None,
                root_started: false,
                root_complete: false,
                last_token_start: 0,
            },
            save_counter: 0,
            saved_states: Vec::new(),
        }
    }

    fn skip_whitespace(&mut self) {
        self.pos = skip_ws(self.input, self.pos);
    }

    fn peek_byte(&self) -> Option<u8> {
        self.input.get(self.pos).copied()
    }

    fn advance(&mut self) -> Option<u8> {
        let b = self.input.get(self.pos).copied()?;
        self.pos += 1;
        Some(b)
    }

    fn expect_byte(&mut self, expected: u8, label: &'static str) -> Result<(), ParseError> {
        match self.advance() {
            Some(b) if b == expected => Ok(()),
            Some(b) => Err(self.error(DeserializeErrorKind::UnexpectedChar {
                ch: b as char,
                expected: label,
            })),
            None => Err(self.error(DeserializeErrorKind::UnexpectedEof { expected: label })),
        }
    }

    fn starts_with(&self, s: &[u8]) -> bool {
        self.input[self.pos..].starts_with(s)
    }

    fn peek_ahead(&self, offset: usize) -> Option<u8> {
        self.input.get(self.pos + offset).copied()
    }

    fn span_at(&self, offset: usize, len: usize) -> Span {
        Span::new(offset, len)
    }

    fn current_span_pos(&self) -> Span {
        Span::new(self.pos, 0)
    }

    fn error(&self, kind: DeserializeErrorKind) -> ParseError {
        ParseError::new(self.current_span_pos(), kind)
    }

    fn err_invalid_value(&self, start: usize, message: &'static str) -> ParseError {
        ParseError::new(
            self.span_at(start, self.pos - start),
            DeserializeErrorKind::InvalidValue {
                message: message.into(),
            },
        )
    }

    fn err_unexpected_byte(&self, expected: &'static str) -> ParseError {
        match self.peek_byte() {
            Some(b) => ParseError::new(
                self.span_at(self.pos, 1),
                DeserializeErrorKind::UnexpectedChar {
                    ch: b as char,
                    expected,
                },
            ),
            None => self.error(DeserializeErrorKind::UnexpectedEof { expected }),
        }
    }

    fn parse_hex_digits_as_u64(
        &self,
        from: usize,
        to: usize,
        span_start: usize,
    ) -> Result<u64, ParseError> {
        if from == to {
            return Ok(0);
        }
        let s = core::str::from_utf8(&self.input[from..to]).map_err(|_| {
            ParseError::new(
                self.span_at(from, to - from),
                DeserializeErrorKind::InvalidUtf8 {
                    context: [0u8; 16],
                    context_len: 0,
                },
            )
        })?;
        u64::from_str_radix(s, 16)
            .map_err(|_| self.err_invalid_value(span_start, "invalid hex digits"))
    }

    /// Parse a quoted Lua string (double or single quotes). Returns borrowed if no escapes.
    fn parse_quoted_string(&mut self) -> Result<Cow<'de, str>, ParseError> {
        let quote_pos = self.pos;
        let quote_char = self.advance().ok_or_else(|| {
            self.error(DeserializeErrorKind::UnexpectedEof { expected: "string" })
        })?;
        debug_assert!(quote_char == b'"' || quote_char == b'\'');
        let start = self.pos;

        let (end, has_escapes) = self.scan_string_extent(quote_char, quote_pos)?;

        if !has_escapes {
            let s = core::str::from_utf8(&self.input[start..end]).map_err(|_| {
                ParseError::new(
                    self.span_at(start, end - start),
                    DeserializeErrorKind::InvalidUtf8 {
                        context: [0u8; 16],
                        context_len: 0,
                    },
                )
            })?;
            self.pos = end + 1;
            return Ok(Cow::Borrowed(s));
        }

        self.pos = start;
        let buf = self.decode_string_escapes(end)?;
        self.pos = end + 1;
        Ok(Cow::Owned(buf))
    }

    /// Scan forward to find the closing quote, returning `(end_pos, has_escapes)`.
    /// Advances past escape sequences without decoding them.
    fn scan_string_extent(
        &self,
        quote_char: u8,
        quote_pos: usize,
    ) -> Result<(usize, bool), ParseError> {
        let mut scan = self.pos;
        let mut has_escapes = false;

        loop {
            if scan >= self.input.len() {
                return Err(ParseError::new(
                    self.span_at(quote_pos, scan - quote_pos),
                    DeserializeErrorKind::UnexpectedEof {
                        expected: "closing quote",
                    },
                ));
            }
            match self.input[scan] {
                b if b == quote_char => return Ok((scan, has_escapes)),
                b'\\' => {
                    has_escapes = true;
                    scan += 1;
                    if scan >= self.input.len() {
                        return Err(ParseError::new(
                            self.span_at(quote_pos, scan - quote_pos),
                            DeserializeErrorKind::UnexpectedEof {
                                expected: "escape character",
                            },
                        ));
                    }
                    scan = self.scan_past_escape(scan);
                }
                _ => scan += 1,
            }
        }
    }

    /// Starting right after the `\`, advance `scan` past one escape sequence.
    /// Does not validate — that happens in the decode pass.
    fn scan_past_escape(&self, scan: usize) -> usize {
        match self.input[scan] {
            b'x' => {
                // \xNN — always exactly 2 hex digits
                let mut p = scan + 1;
                if p + 1 < self.input.len()
                    && self.input[p].is_ascii_hexdigit()
                    && self.input[p + 1].is_ascii_hexdigit()
                {
                    p += 2;
                } else {
                    p += 1; // will error during decode pass
                }
                p
            }
            b'u' if self.input.get(scan + 1).copied() == Some(b'{') => {
                // \u{XXXX}
                let mut p = scan + 2;
                while p < self.input.len() && self.input[p].is_ascii_hexdigit() {
                    p += 1;
                }
                if p < self.input.len() && self.input[p] == b'}' {
                    p += 1;
                }
                p
            }
            b'z' => {
                // \z skip following whitespace
                let mut p = scan + 1;
                while p < self.input.len() && matches!(self.input[p], b' ' | b'\t' | b'\n' | b'\r')
                {
                    p += 1;
                }
                p
            }
            d if d.is_ascii_digit() => {
                // \ddd — up to 3 decimal digits
                let mut p = scan + 1;
                if p < self.input.len() && self.input[p].is_ascii_digit() {
                    p += 1;
                    if p < self.input.len() && self.input[p].is_ascii_digit() {
                        p += 1;
                    }
                }
                p
            }
            _ => scan + 1,
        }
    }

    /// Decode the escape sequences between `self.pos` and `end`, returning the owned string.
    fn decode_string_escapes(&mut self, end: usize) -> Result<String, ParseError> {
        let mut buf = String::new();
        while self.pos < end {
            if self.input[self.pos] == b'\\' {
                self.pos += 1;
                self.decode_one_escape(&mut buf, end)?;
            } else {
                self.push_utf8_char(&mut buf, end)?;
            }
        }
        Ok(buf)
    }

    /// Decode a single escape sequence (positioned right after the `\`).
    fn decode_one_escape(&mut self, buf: &mut String, end: usize) -> Result<(), ParseError> {
        let esc = self.input[self.pos];
        if let Some(ch) = simple_escape(esc) {
            buf.push(ch);
            self.pos += 1;
            return Ok(());
        }
        match esc {
            b'x' => self.decode_hex_escape(buf, end),
            b'u' if self.input.get(self.pos + 1).copied() == Some(b'{') => {
                self.decode_unicode_escape(buf, end)
            }
            b'z' => {
                self.pos += 1;
                while self.pos < end && matches!(self.input[self.pos], b' ' | b'\t' | b'\n' | b'\r')
                {
                    self.pos += 1;
                }
                Ok(())
            }
            b'0' if self.pos + 1 >= end || !self.input[self.pos + 1].is_ascii_digit() => {
                buf.push('\0');
                self.pos += 1;
                Ok(())
            }
            d if d.is_ascii_digit() => self.decode_decimal_escape(buf, end),
            _ => Err(self.err_invalid_value(self.pos, "unknown escape sequence")),
        }
    }

    /// Decode `\xNN` hex escape.
    fn decode_hex_escape(&mut self, buf: &mut String, end: usize) -> Result<(), ParseError> {
        self.pos += 1; // skip 'x'
        if self.pos + 1 >= end
            || !self.input[self.pos].is_ascii_hexdigit()
            || !self.input[self.pos + 1].is_ascii_hexdigit()
        {
            return Err(ParseError::new(
                self.span_at(self.pos - 2, 4),
                DeserializeErrorKind::InvalidValue {
                    message: "\\x requires exactly 2 hex digits".into(),
                },
            ));
        }
        let hi = (self.input[self.pos] as char).to_digit(16).unwrap() as u8;
        let lo = (self.input[self.pos + 1] as char).to_digit(16).unwrap() as u8;
        buf.push((hi * 16 + lo) as char);
        self.pos += 2;
        Ok(())
    }

    /// Decode `\u{XXXX}` unicode escape.
    fn decode_unicode_escape(&mut self, buf: &mut String, end: usize) -> Result<(), ParseError> {
        self.pos += 2; // skip 'u{'
        let hex_start = self.pos;
        while self.pos < end && self.input[self.pos].is_ascii_hexdigit() {
            self.pos += 1;
        }
        if self.pos >= end || self.input[self.pos] != b'}' {
            return Err(ParseError::new(
                self.span_at(hex_start - 3, self.pos - hex_start + 3),
                DeserializeErrorKind::InvalidValue {
                    message: "\\u{} requires hex digits and closing '}'".into(),
                },
            ));
        }
        let hex_str = core::str::from_utf8(&self.input[hex_start..self.pos]).unwrap();
        let val = u32::from_str_radix(hex_str, 16)
            .map_err(|_| self.err_invalid_value(hex_start, "invalid unicode escape"))?;
        let c = char::from_u32(val)
            .ok_or_else(|| self.err_invalid_value(hex_start, "invalid unicode code point"))?;
        buf.push(c);
        self.pos += 1; // skip '}'
        Ok(())
    }

    /// Decode `\ddd` decimal escape (1-3 digits).
    fn decode_decimal_escape(&mut self, buf: &mut String, end: usize) -> Result<(), ParseError> {
        let d = self.input[self.pos];
        let mut val: u32 = (d - b'0') as u32;
        self.pos += 1;
        if self.pos < end && self.input[self.pos].is_ascii_digit() {
            val = val * 10 + (self.input[self.pos] - b'0') as u32;
            self.pos += 1;
            if self.pos < end && self.input[self.pos].is_ascii_digit() {
                val = val * 10 + (self.input[self.pos] - b'0') as u32;
                self.pos += 1;
            }
        }
        let c = char::from_u32(val)
            .ok_or_else(|| self.err_invalid_value(self.pos - 1, "invalid decimal escape value"))?;
        buf.push(c);
        Ok(())
    }

    /// Push one UTF-8 character from `self.pos` into `buf`.
    fn push_utf8_char(&mut self, buf: &mut String, end: usize) -> Result<(), ParseError> {
        let char_end = self.pos + utf8_char_len(self.input[self.pos]);
        if char_end > end {
            return Err(self.invalid_utf8_at(self.pos));
        }
        let s = core::str::from_utf8(&self.input[self.pos..char_end])
            .map_err(|_| self.invalid_utf8_at(self.pos))?;
        buf.push_str(s);
        self.pos = char_end;
        Ok(())
    }

    fn invalid_utf8_at(&self, pos: usize) -> ParseError {
        ParseError::new(
            self.span_at(pos, 1),
            DeserializeErrorKind::InvalidUtf8 {
                context: [0u8; 16],
                context_len: 0,
            },
        )
    }

    /// Parse a long bracket string `[=*[...]=*]`. Always borrows from input.
    fn parse_long_string(&mut self) -> Result<Cow<'de, str>, ParseError> {
        let start = self.pos;
        let level = match_long_bracket_open(self.input, self.pos).ok_or_else(|| {
            self.error(DeserializeErrorKind::UnexpectedToken {
                got: "'['".into(),
                expected: "long string opening bracket",
            })
        })?;
        let opener_len = 2 + level; // `[` + `=`*level + `[`
        let body_start = self.pos + opener_len;

        // Skip leading newline (Lua spec: first newline after opener is ignored)
        let content_start = if self.input.get(body_start).copied() == Some(b'\n') {
            body_start + 1
        } else if self.input.get(body_start).copied() == Some(b'\r') {
            if self.input.get(body_start + 1).copied() == Some(b'\n') {
                body_start + 2
            } else {
                body_start + 1
            }
        } else {
            body_start
        };

        let close_pos =
            find_long_bracket_close(self.input, body_start, level).ok_or_else(|| {
                ParseError::new(
                    self.span_at(start, self.input.len() - start),
                    DeserializeErrorKind::UnexpectedEof {
                        expected: "closing long bracket",
                    },
                )
            })?;

        // Content ends at the `]` of the closing bracket
        let content_end = close_pos - 2 - level;
        self.pos = close_pos;

        let s = core::str::from_utf8(&self.input[content_start..content_end]).map_err(|_| {
            ParseError::new(
                self.span_at(content_start, content_end - content_start),
                DeserializeErrorKind::InvalidUtf8 {
                    context: [0u8; 16],
                    context_len: 0,
                },
            )
        })?;
        Ok(Cow::Borrowed(s))
    }

    /// Parse any Lua string: quoted (`"..."` / `'...'`) or long bracket (`[[...]]`).
    fn parse_string(&mut self) -> Result<Cow<'de, str>, ParseError> {
        match self.peek_byte() {
            Some(b'"') | Some(b'\'') => self.parse_quoted_string(),
            Some(b'[') => self.parse_long_string(),
            _ => Err(self.err_unexpected_byte("string")),
        }
    }

    fn parse_number(&mut self, negative: bool) -> Result<ScalarValue<'de>, ParseError> {
        if self.pos + 1 < self.input.len()
            && self.input[self.pos] == b'0'
            && (self.input[self.pos + 1] == b'x' || self.input[self.pos + 1] == b'X')
        {
            self.parse_hex_number(negative)
        } else {
            self.parse_decimal_number(negative)
        }
    }

    /// Parse a hex integer (`0xFF`) or hex float (`0x1.8p1`).
    fn parse_hex_number(&mut self, negative: bool) -> Result<ScalarValue<'de>, ParseError> {
        let start = self.pos;
        self.pos += 2; // skip "0x"
        let hex_start = self.pos;

        while self.pos < self.input.len() && self.input[self.pos].is_ascii_hexdigit() {
            self.pos += 1;
        }
        let int_end = self.pos;

        let has_dot = self.pos < self.input.len() && self.input[self.pos] == b'.';
        if has_dot {
            self.pos += 1;
            while self.pos < self.input.len() && self.input[self.pos].is_ascii_hexdigit() {
                self.pos += 1;
            }
        }
        let frac_end = self.pos;

        let has_exp = self.pos < self.input.len() && matches!(self.input[self.pos], b'p' | b'P');
        if has_exp {
            self.pos += 1;
            if self.pos < self.input.len() && matches!(self.input[self.pos], b'+' | b'-') {
                self.pos += 1;
            }
            if self.pos >= self.input.len() || !self.input[self.pos].is_ascii_digit() {
                return Err(self.err_invalid_value(start, "expected digits after exponent"));
            }
            while self.pos < self.input.len() && self.input[self.pos].is_ascii_digit() {
                self.pos += 1;
            }
        }

        if int_end == hex_start && !has_dot {
            return Err(self.err_invalid_value(start, "expected hex digits after 0x"));
        }

        if has_dot || has_exp {
            let ext = HexExtent {
                start,
                hex_start,
                int_end,
                frac_end,
                has_dot,
                has_exp,
            };
            return self.parse_hex_float_value(&ext, negative);
        }

        // Plain hex integer
        let val = self.parse_hex_digits_as_u64(hex_start, int_end, start)?;
        if negative {
            Ok(ScalarValue::I64(-(val as i64)))
        } else {
            Ok(ScalarValue::U64(val))
        }
    }

    /// Compute the value of a hex float once the extent has been scanned.
    fn parse_hex_float_value(
        &self,
        ext: &HexExtent,
        negative: bool,
    ) -> Result<ScalarValue<'de>, ParseError> {
        let int_part = self.parse_hex_digits_as_u64(ext.hex_start, ext.int_end, ext.start)?;
        let frac_val = if ext.has_dot {
            let frac_start = ext.int_end + 1; // skip '.'
            let frac_len = ext.frac_end - frac_start;
            if frac_len == 0 {
                0.0
            } else {
                let frac_int = self.parse_hex_digits_as_u64(frac_start, ext.frac_end, ext.start)?;
                frac_int as f64 / 16_f64.powi(frac_len as i32)
            }
        } else {
            0.0
        };
        let exp = if ext.has_exp {
            let exp_str =
                core::str::from_utf8(&self.input[ext.frac_end + 1..self.pos]).unwrap_or("0");
            exp_str
                .parse::<i32>()
                .map_err(|_| self.err_invalid_value(ext.start, "invalid hex float exponent"))?
        } else {
            0
        };
        let val = (int_part as f64 + frac_val) * 2_f64.powi(exp);
        Ok(ScalarValue::F64(if negative { -val } else { val }))
    }

    /// Parse a decimal integer or float.
    fn parse_decimal_number(&mut self, negative: bool) -> Result<ScalarValue<'de>, ParseError> {
        let start = self.pos;
        let mut has_dot = false;
        let mut has_e = false;

        while self.pos < self.input.len() {
            match self.input[self.pos] {
                b'0'..=b'9' => self.pos += 1,
                b'.' => {
                    has_dot = true;
                    self.pos += 1;
                }
                b'e' | b'E' => {
                    has_e = true;
                    self.pos += 1;
                    if self.pos < self.input.len()
                        && (self.input[self.pos] == b'+' || self.input[self.pos] == b'-')
                    {
                        self.pos += 1;
                    }
                }
                _ => break,
            }
        }

        let num_str = core::str::from_utf8(&self.input[start..self.pos]).map_err(|_| {
            ParseError::new(
                self.span_at(start, self.pos - start),
                DeserializeErrorKind::InvalidUtf8 {
                    context: [0u8; 16],
                    context_len: 0,
                },
            )
        })?;

        if has_dot || has_e {
            let val: f64 = num_str
                .parse()
                .map_err(|_| self.err_invalid_value(start, "invalid float"))?;
            return Ok(ScalarValue::F64(if negative { -val } else { val }));
        }

        if negative {
            return self.parse_negative_integer(num_str, start);
        }

        if let Ok(val) = num_str.parse::<u64>() {
            return Ok(ScalarValue::U64(val));
        }
        if let Ok(val) = num_str.parse::<i64>() {
            return Ok(ScalarValue::I64(val));
        }
        if let Ok(val) = num_str.parse::<u128>() {
            return Ok(ScalarValue::U128(val));
        }
        if let Ok(val) = num_str.parse::<i128>() {
            return Ok(ScalarValue::I128(val));
        }
        Err(self.err_invalid_value(start, "integer out of range"))
    }

    /// Parse unsigned digits as a negative integer, avoiding format!() allocation.
    /// Handles the i64::MIN edge case where the unsigned magnitude doesn't fit in i64.
    fn parse_negative_integer(
        &self,
        num_str: &str,
        start: usize,
    ) -> Result<ScalarValue<'de>, ParseError> {
        if let Ok(val) = num_str.parse::<u64>() {
            // val == i64::MAX + 1 is the i64::MIN magnitude
            if val <= i64::MAX as u64 {
                return Ok(ScalarValue::I64(-(val as i64)));
            }
            if val == i64::MAX as u64 + 1 {
                return Ok(ScalarValue::I64(i64::MIN));
            }
            // Doesn't fit in i64, use i128
            return Ok(ScalarValue::I128(-(val as i128)));
        }
        if let Ok(val) = num_str.parse::<u128>() {
            if val <= i128::MAX as u128 {
                return Ok(ScalarValue::I128(-(val as i128)));
            }
            if val == i128::MAX as u128 + 1 {
                return Ok(ScalarValue::I128(i128::MIN));
            }
        }
        Err(self.err_invalid_value(start, "integer out of range"))
    }

    fn parse_identifier(&mut self) -> Result<&'de str, ParseError> {
        let start = self.pos;
        if self.pos >= self.input.len()
            || (!self.input[self.pos].is_ascii_alphabetic() && self.input[self.pos] != b'_')
        {
            return Err(self.err_unexpected_byte("identifier"));
        }
        self.pos += 1;
        while self.pos < self.input.len()
            && (self.input[self.pos].is_ascii_alphanumeric() || self.input[self.pos] == b'_')
        {
            self.pos += 1;
        }
        core::str::from_utf8(&self.input[start..self.pos]).map_err(|_| {
            ParseError::new(
                self.span_at(start, self.pos - start),
                DeserializeErrorKind::InvalidUtf8 {
                    context: [0u8; 16],
                    context_len: 0,
                },
            )
        })
    }

    /// After consuming `{`, peek ahead to decide struct vs sequence.
    fn peek_table_is_struct(&self) -> bool {
        let p = skip_ws(self.input, self.pos);

        if p >= self.input.len() {
            return true; // EOF → will error later
        }

        match self.input[p] {
            b'}' => true, // empty table → struct
            b'[' => {
                // `["key"] = ...` or `['key'] = ...` → struct key
                // `[[long string]]` as first element → sequence
                let next = self.input.get(p + 1).copied();
                matches!(next, Some(b'"') | Some(b'\''))
            }
            b if b.is_ascii_alphabetic() || b == b'_' => {
                // Scan the identifier
                let mut p = p + 1;
                while p < self.input.len()
                    && (self.input[p].is_ascii_alphanumeric() || self.input[p] == b'_')
                {
                    p += 1;
                }

                let p = skip_ws(self.input, p);

                // If followed by `=` (but not `==`), it's a struct field
                if p < self.input.len() && self.input[p] == b'=' {
                    !(p + 1 < self.input.len() && self.input[p + 1] == b'=')
                } else {
                    false // bare value → sequence
                }
            }
            _ => false, // number, string, `-`, etc. → sequence
        }
    }

    /// Returns true if `self.pos` is at the start of a string literal
    /// (quoted or long bracket).
    fn at_string_start(&self) -> bool {
        match self.peek_byte() {
            Some(b'"') | Some(b'\'') => true,
            Some(b'[') => match_long_bracket_open(self.input, self.pos).is_some(),
            _ => false,
        }
    }

    fn parse_value(&mut self) -> Result<ParseEvent<'de>, ParseError> {
        self.skip_whitespace();
        self.state.last_token_start = self.pos;
        self.state.root_started = true;
        let span_start = self.pos;

        match self.classify_value()? {
            ValueStart::String => {
                let s = self.parse_string()?;
                let span = self.span_at(span_start, self.pos - span_start);
                self.finish_value_in_parent();
                Ok(ParseEvent::new(
                    ParseEventKind::Scalar(ScalarValue::Str(s)),
                    span,
                ))
            }
            ValueStart::Table => {
                self.pos += 1;
                let span = self.span_at(span_start, 1);
                if self.peek_table_is_struct() {
                    self.state
                        .stack
                        .push(ContextState::Struct(StructState::KeyOrEnd));
                    Ok(ParseEvent::new(
                        ParseEventKind::StructStart(ContainerKind::Object),
                        span,
                    ))
                } else {
                    self.state
                        .stack
                        .push(ContextState::Seq(SeqState::ValueOrEnd));
                    Ok(ParseEvent::new(
                        ParseEventKind::SequenceStart(ContainerKind::Array),
                        span,
                    ))
                }
            }
            ValueStart::Negative => {
                self.pos += 1;
                self.skip_whitespace();
                if self.starts_with(consts::MATH_HUGE) {
                    self.pos += consts::MATH_HUGE.len();
                    let span = self.span_at(span_start, self.pos - span_start);
                    self.finish_value_in_parent();
                    return Ok(ParseEvent::new(
                        ParseEventKind::Scalar(ScalarValue::F64(f64::NEG_INFINITY)),
                        span,
                    ));
                }
                if self.peek_byte().is_some_and(|b| b.is_ascii_digit()) {
                    let scalar = self.parse_number(true)?;
                    let span = self.span_at(span_start, self.pos - span_start);
                    self.finish_value_in_parent();
                    return Ok(ParseEvent::new(ParseEventKind::Scalar(scalar), span));
                }
                Err(ParseError::new(
                    self.span_at(span_start, 1),
                    DeserializeErrorKind::UnexpectedToken {
                        got: "'-'".into(),
                        expected: "number or 'math.huge'",
                    },
                ))
            }
            ValueStart::NaN => {
                self.pos += 3;
                let span = self.span_at(span_start, 3);
                self.finish_value_in_parent();
                Ok(ParseEvent::new(
                    ParseEventKind::Scalar(ScalarValue::F64(f64::NAN)),
                    span,
                ))
            }
            ValueStart::Number => {
                let scalar = self.parse_number(false)?;
                let span = self.span_at(span_start, self.pos - span_start);
                self.finish_value_in_parent();
                Ok(ParseEvent::new(ParseEventKind::Scalar(scalar), span))
            }
            ValueStart::Identifier => self.parse_keyword_or_ident_value(span_start),
        }
    }

    /// Dispatch an identifier-starting value: keywords (`nil`, `true`, `false`, `math.huge`,
    /// `function`) or bare identifiers treated as strings.
    fn parse_keyword_or_ident_value(
        &mut self,
        span_start: usize,
    ) -> Result<ParseEvent<'de>, ParseError> {
        let ident = self.parse_identifier()?;
        let span = self.span_at(span_start, self.pos - span_start);
        let scalar = match ident {
            "nil" => ScalarValue::Null,
            "true" => ScalarValue::Bool(true),
            "false" => ScalarValue::Bool(false),
            "math" => {
                self.expect_byte(b'.', "'.'")?;
                let rest = self.parse_identifier()?;
                if rest != "huge" {
                    return Err(self.err_invalid_value(span_start, "expected math.huge"));
                }
                let span = self.span_at(span_start, self.pos - span_start);
                self.finish_value_in_parent();
                return Ok(ParseEvent::new(
                    ParseEventKind::Scalar(ScalarValue::F64(f64::INFINITY)),
                    span,
                ));
            }
            "function" => {
                self.skip_function_body()?;
                let span = self.span_at(span_start, self.pos - span_start);
                self.finish_value_in_parent();
                return Ok(ParseEvent::new(
                    ParseEventKind::Scalar(ScalarValue::Null),
                    span,
                ));
            }
            _ => ScalarValue::Str(Cow::Borrowed(ident)),
        };
        self.finish_value_in_parent();
        Ok(ParseEvent::new(ParseEventKind::Scalar(scalar), span))
    }

    fn finish_value_in_parent(&mut self) {
        match self.state.stack.last_mut() {
            Some(ContextState::Struct(state)) => *state = StructState::CommaOrEnd,
            Some(ContextState::Seq(state)) => *state = SeqState::CommaOrEnd,
            None if self.state.root_started => self.state.root_complete = true,
            _ => {}
        }
    }

    /// Classify what kind of value token is next (after whitespace is skipped).
    fn classify_value(&self) -> Result<ValueStart, ParseError> {
        if self.at_string_start() {
            return Ok(ValueStart::String);
        }
        match self.peek_byte() {
            None => Err(ParseError::new(
                self.current_span_pos(),
                DeserializeErrorKind::UnexpectedEof { expected: "value" },
            )),
            Some(b'{') => Ok(ValueStart::Table),
            Some(b'-') => Ok(ValueStart::Negative),
            Some(b'0') if self.peek_ahead(1) == Some(b'/') && self.peek_ahead(2) == Some(b'0') => {
                Ok(ValueStart::NaN)
            }
            Some(b) if b.is_ascii_digit() => Ok(ValueStart::Number),
            Some(b) if b.is_ascii_alphabetic() || b == b'_' => Ok(ValueStart::Identifier),
            Some(b) => Err(ParseError::new(
                self.span_at(self.pos, 1),
                DeserializeErrorKind::UnexpectedChar {
                    ch: b as char,
                    expected: "value",
                },
            )),
        }
    }

    /// Consume `}` and return the appropriate end event for the current container.
    fn close_container(&mut self) -> ParseEvent<'de> {
        let span = self.span_at(self.pos, 1);
        self.pos += 1;
        let end_kind = match self.state.stack.pop() {
            Some(ContextState::Struct(_)) => ParseEventKind::StructEnd,
            Some(ContextState::Seq(_)) => ParseEventKind::SequenceEnd,
            None => unreachable!("close_container called without container on stack"),
        };
        self.finish_value_in_parent();
        ParseEvent::new(end_kind, span)
    }

    /// Handle the `CommaOrEnd` state for both struct and sequence containers.
    /// Returns `Ok(None)` when a separator was consumed (caller should continue),
    /// `Ok(Some(event))` when the container was closed.
    fn consume_separator_or_close(&mut self) -> Result<Option<ParseEvent<'de>>, ParseError> {
        self.skip_whitespace();
        match self.peek_byte() {
            Some(b',' | b';') => {
                self.pos += 1;
                match self.state.stack.last_mut() {
                    Some(ContextState::Struct(state)) => *state = StructState::KeyOrEnd,
                    Some(ContextState::Seq(state)) => *state = SeqState::ValueOrEnd,
                    _ => {}
                }
                Ok(None)
            }
            Some(b'}') => Ok(Some(self.close_container())),
            Some(b) => Err(ParseError::new(
                self.current_span_pos(),
                DeserializeErrorKind::UnexpectedChar {
                    ch: b as char,
                    expected: "',', ';', or '}'",
                },
            )),
            None => Err(self.error(DeserializeErrorKind::UnexpectedEof {
                expected: "',', ';', or '}'",
            })),
        }
    }

    fn parse_field_key(&mut self) -> Result<ParseEvent<'de>, ParseError> {
        self.skip_whitespace();
        let span_start = self.pos;

        let key: Cow<'de, str> = if self.peek_byte() == Some(b'[') {
            self.pos += 1;
            let s = self.parse_string()?;
            self.skip_whitespace();
            self.expect_byte(b']', "']'")?;
            s
        } else {
            let ident = self.parse_identifier()?;
            Cow::Borrowed(ident)
        };

        let span = self.span_at(span_start, self.pos - span_start);

        self.skip_whitespace();
        self.expect_byte(b'=', "'='")?;

        // Transition to expecting the field value
        if let Some(ContextState::Struct(state)) = self.state.stack.last_mut() {
            *state = StructState::Value;
        }

        Ok(ParseEvent::new(
            ParseEventKind::FieldKey(FieldKey::new(key, FieldLocationHint::KeyValue)),
            span,
        ))
    }

    fn produce_event(&mut self) -> Result<Option<ParseEvent<'de>>, ParseError> {
        loop {
            self.skip_whitespace();

            match self.state.stack.last().cloned() {
                None if self.state.root_complete => return Ok(None),
                None => return self.parse_value().map(Some),
                Some(ContextState::Struct(struct_state)) => {
                    if let Some(event) = self.produce_struct_event(struct_state)? {
                        return Ok(Some(event));
                    }
                }
                Some(ContextState::Seq(seq_state)) => {
                    if let Some(event) = self.produce_seq_event(seq_state)? {
                        return Ok(Some(event));
                    }
                }
            }
        }
    }

    /// Returns `Ok(None)` when a separator was consumed (caller should loop again).
    fn produce_struct_event(
        &mut self,
        struct_state: StructState,
    ) -> Result<Option<ParseEvent<'de>>, ParseError> {
        match struct_state {
            StructState::KeyOrEnd => {
                self.skip_whitespace();
                match self.peek_byte() {
                    Some(b'}') => Ok(Some(self.close_container())),
                    Some(_) => self.parse_field_key().map(Some),
                    None => Err(self.error(DeserializeErrorKind::UnexpectedEof {
                        expected: "field name or '}'",
                    })),
                }
            }
            StructState::Value => self.parse_value().map(Some),
            StructState::CommaOrEnd => self.consume_separator_or_close(),
        }
    }

    /// Returns `Ok(None)` when a separator was consumed (caller should loop again).
    fn produce_seq_event(
        &mut self,
        seq_state: SeqState,
    ) -> Result<Option<ParseEvent<'de>>, ParseError> {
        match seq_state {
            SeqState::ValueOrEnd => {
                self.skip_whitespace();
                match self.peek_byte() {
                    Some(b'}') => Ok(Some(self.close_container())),
                    Some(_) => self.parse_value().map(Some),
                    None => Err(self.error(DeserializeErrorKind::UnexpectedEof {
                        expected: "value or '}'",
                    })),
                }
            }
            SeqState::CommaOrEnd => self.consume_separator_or_close(),
        }
    }

    fn skip_value_raw(&mut self) -> Result<(), ParseError> {
        self.skip_whitespace();
        match self.classify_value()? {
            ValueStart::String => {
                self.parse_string()?;
                Ok(())
            }
            ValueStart::Table => {
                self.pos += 1;
                self.skip_table_contents()
            }
            ValueStart::Negative => {
                self.pos += 1;
                self.skip_whitespace();
                if self.starts_with(consts::MATH_HUGE) {
                    self.pos += consts::MATH_HUGE.len();
                } else {
                    self.parse_number(true)?;
                }
                Ok(())
            }
            ValueStart::NaN => {
                self.pos += 3;
                Ok(())
            }
            ValueStart::Number => {
                self.parse_number(false)?;
                Ok(())
            }
            ValueStart::Identifier => {
                let ident = self.parse_identifier()?;
                match ident {
                    "math" => {
                        self.expect_byte(b'.', "'.'")?;
                        self.parse_identifier()?;
                    }
                    "function" => {
                        self.skip_function_body()?;
                    }
                    _ => {}
                }
                Ok(())
            }
        }
    }

    /// Skip a function body: everything from after `function` to the matching `end`.
    /// Handles nested blocks (`function`, `if`, `for`, `while` ... `end`)
    /// and `repeat` ... `until`.
    fn skip_function_body(&mut self) -> Result<(), ParseError> {
        let start = self.pos;
        let mut depth: usize = 1; // we're already inside one `function`

        while depth > 0 {
            self.skip_whitespace();

            if self.at_string_start() {
                self.parse_string()?;
                continue;
            }

            match self.peek_byte() {
                Some(b) if b.is_ascii_alphabetic() || b == b'_' => {
                    let ident = self.parse_identifier()?;
                    if BLOCK_OPENERS.contains(&ident) {
                        depth += 1;
                    } else if ident == "repeat" {
                        // repeat...until doesn't use `end`, but can nest blocks
                        depth += 1;
                    } else if ident == "until" {
                        // closes a `repeat` block
                        depth -= 1;
                    } else if ident == "end" {
                        depth -= 1;
                    }
                }
                Some(b'{') => {
                    self.pos += 1;
                    self.skip_table_contents()?;
                }
                Some(_) => {
                    self.pos += 1;
                }
                None => {
                    return Err(ParseError::new(
                        self.span_at(start, self.pos - start),
                        DeserializeErrorKind::UnexpectedEof {
                            expected: "'end' to close function",
                        },
                    ));
                }
            }
        }
        Ok(())
    }

    /// Skip table contents — assumes `{` already consumed.
    fn skip_table_contents(&mut self) -> Result<(), ParseError> {
        let mut depth: usize = 1;
        while depth > 0 {
            self.skip_whitespace();
            if self.at_string_start() {
                self.parse_string()?;
                continue;
            }
            match self.peek_byte() {
                Some(b'{') => {
                    self.pos += 1;
                    depth += 1;
                }
                Some(b'}') => {
                    self.pos += 1;
                    depth -= 1;
                }
                Some(_) => {
                    self.pos += 1;
                }
                None => {
                    return Err(self.error(DeserializeErrorKind::UnexpectedEof { expected: "'}'" }));
                }
            }
        }
        Ok(())
    }
}

impl<'de> FormatParser<'de> for LuaParser<'de> {
    fn next_event(&mut self) -> Result<Option<ParseEvent<'de>>, ParseError> {
        if let Some(event) = self.state.event_peek.take() {
            return Ok(Some(event));
        }
        self.produce_event()
    }

    fn next_events(
        &mut self,
        buf: &mut VecDeque<ParseEvent<'de>>,
        limit: usize,
    ) -> Result<usize, ParseError> {
        if limit == 0 {
            return Ok(0);
        }
        let mut count = 0;
        if let Some(event) = self.state.event_peek.take() {
            buf.push_back(event);
            count += 1;
        }
        while count < limit {
            match self.produce_event()? {
                Some(event) => {
                    buf.push_back(event);
                    count += 1;
                }
                None => break,
            }
        }
        Ok(count)
    }

    fn peek_event(&mut self) -> Result<Option<ParseEvent<'de>>, ParseError> {
        if let Some(event) = self.state.event_peek.clone() {
            return Ok(Some(event));
        }
        let event = self.produce_event()?;
        if let Some(ref e) = event {
            self.state.event_peek = Some(e.clone());
        }
        Ok(event)
    }

    fn save(&mut self) -> SavePoint {
        self.save_counter += 1;
        self.saved_states
            .push((self.save_counter, self.state.clone(), self.pos));
        SavePoint(self.save_counter)
    }

    fn restore(&mut self, save_point: SavePoint) {
        if let Some(idx) = self
            .saved_states
            .iter()
            .position(|(id, _, _)| *id == save_point.0)
        {
            let (_, saved_state, saved_pos) = self.saved_states.remove(idx);
            self.state = saved_state;
            self.pos = saved_pos;
        }
    }

    fn skip_value(&mut self) -> Result<(), ParseError> {
        debug_assert!(
            self.state.event_peek.is_none(),
            "skip_value called while an event is buffered"
        );
        self.skip_value_raw()?;
        self.finish_value_in_parent();
        Ok(())
    }

    fn format_namespace(&self) -> Option<&'static str> {
        Some("lua")
    }

    fn input(&self) -> Option<&'de [u8]> {
        Some(self.input)
    }

    fn current_span(&self) -> Option<Span> {
        let offset = self.state.last_token_start;
        let len = self.pos.saturating_sub(offset);
        Some(Span::new(offset, len))
    }
}
