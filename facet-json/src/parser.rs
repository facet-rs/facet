extern crate alloc;

use alloc::{borrow::Cow, collections::VecDeque, format, vec::Vec};
use core::str;

use facet_core::Facet as _;
use facet_format::{
    ContainerKind, DeserializeErrorKind, FieldKey, FieldLocationHint, FormatParser, ParseError,
    ParseEvent, ParseEventKind, SavePoint, ScalarValue,
};
use facet_reflect::Span;

use crate::scanner::{self, ParsedNumber, ScanError, ScanErrorKind, Scanner, Token as ScanToken};

/// Convert a ScanError to a ParseError.
fn scan_error_to_parse_error(err: ScanError) -> ParseError {
    let kind = match err.kind {
        ScanErrorKind::UnexpectedChar(ch) => DeserializeErrorKind::UnexpectedChar {
            ch,
            expected: "valid JSON token",
        },
        ScanErrorKind::UnexpectedEof(expected) => DeserializeErrorKind::UnexpectedEof { expected },
        ScanErrorKind::InvalidUtf8 => DeserializeErrorKind::InvalidUtf8 {
            context: [0u8; 16],
            context_len: 0,
        },
    };
    ParseError::new(err.span, kind)
}

fn invalid_utf8_parse_error(span: Span) -> ParseError {
    ParseError::new(
        span,
        DeserializeErrorKind::InvalidUtf8 {
            context: [0u8; 16],
            context_len: 0,
        },
    )
}

#[derive(Debug, Clone)]
struct MaterializedToken<'de> {
    kind: TokenKind<'de>,
    span: Span,
}

#[derive(Debug, Clone)]
enum TokenKind<'de> {
    ObjectStart,
    ObjectEnd,
    ArrayStart,
    ArrayEnd,
    Colon,
    Comma,
    Null,
    True,
    False,
    String(Cow<'de, str>),
    U64(u64),
    I64(i64),
    U128(u128),
    I128(i128),
    F64(f64),
    Eof,
}

#[derive(Debug, Clone)]
enum JsonValueStart<'de> {
    Object(Span),
    Array(Span),
    Scalar(ScalarValue<'de>, Span),
}

impl JsonValueStart<'_> {
    fn span(&self) -> Span {
        match self {
            Self::Object(span) | Self::Array(span) | Self::Scalar(_, span) => *span,
        }
    }
}

pub(crate) enum JsonObjectKeyStep<'de> {
    Field {
        key: JsonFieldKeyInput<'de>,
        span: Span,
    },
    End,
}

pub(crate) enum JsonObjectOrderedScalarStep<'de> {
    Matched {
        span: Span,
        value: JsonScalarInput<'de>,
    },
    Field {
        key: JsonFieldKeyInput<'de>,
        span: Span,
    },
    End,
}

pub(crate) enum JsonObjectOrderedI32Step<'de> {
    Matched {
        span: Span,
        value: i32,
    },
    MatchedInput {
        span: Span,
        value: JsonScalarInput<'de>,
    },
    Field {
        key: JsonFieldKeyInput<'de>,
        span: Span,
    },
    End,
}

impl<'de> From<JsonObjectOrderedScalarStep<'de>> for JsonObjectOrderedI32Step<'de> {
    fn from(step: JsonObjectOrderedScalarStep<'de>) -> Self {
        match step {
            JsonObjectOrderedScalarStep::Matched { span, value } => {
                Self::MatchedInput { span, value }
            }
            JsonObjectOrderedScalarStep::Field { key, span } => Self::Field { key, span },
            JsonObjectOrderedScalarStep::End => Self::End,
        }
    }
}

pub(crate) enum JsonSequenceScalarStep<'de> {
    Value { value: JsonScalarInput<'de> },
    End,
}

pub(crate) enum JsonScalarInput<'de> {
    Raw(scanner::SpannedToken),
    Materialized(JsonScalarToken<'de>, Span),
}

impl JsonScalarInput<'_> {
    pub(crate) fn is_null(&self) -> bool {
        match self {
            Self::Raw(token) => matches!(token.token, ScanToken::Null),
            Self::Materialized(value, _) => matches!(value, JsonScalarToken::Null),
        }
    }
}

pub(crate) enum JsonFieldKey<'de> {
    Borrowed(&'de str),
    Decoded(Cow<'de, str>),
}

impl JsonFieldKey<'_> {
    pub(crate) fn as_str(&self) -> &str {
        match self {
            Self::Borrowed(value) => value,
            Self::Decoded(value) => value.as_ref(),
        }
    }
}

pub(crate) enum JsonFieldKeyInput<'de> {
    Raw {
        start: usize,
        end: usize,
        has_escapes: bool,
        span: Span,
    },
    Materialized(JsonFieldKey<'de>),
}

pub(crate) enum JsonScalarToken<'de> {
    Null,
    Bool(bool),
    Str(Cow<'de, str>),
    U64(u64),
    I64(i64),
    U128(u128),
    I128(i128),
    F64(f64),
    Other,
}

impl<'de> JsonScalarToken<'de> {
    pub(crate) fn kind_name(&self) -> &'static str {
        match self {
            Self::Null => "null",
            Self::Bool(_) => "bool",
            Self::Str(_) => "string",
            Self::U64(_) => "u64",
            Self::I64(_) => "i64",
            Self::U128(_) => "u128",
            Self::I128(_) => "i128",
            Self::F64(_) => "f64",
            Self::Other => "scalar",
        }
    }

    fn from_scalar_value(value: ScalarValue<'de>) -> Self {
        match value {
            ScalarValue::Unit | ScalarValue::Null => Self::Null,
            ScalarValue::Bool(value) => Self::Bool(value),
            ScalarValue::Char(value) => Self::Str(Cow::Owned(value.into())),
            ScalarValue::I64(value) => Self::I64(value),
            ScalarValue::U64(value) => Self::U64(value),
            ScalarValue::I128(value) => Self::I128(value),
            ScalarValue::U128(value) => Self::U128(value),
            ScalarValue::F64(value) => Self::F64(value),
            ScalarValue::Str(value) => Self::Str(value),
            ScalarValue::Bytes(_) => Self::Other,
            _ => Self::Other,
        }
    }
}

/// Mutable parser state that can be saved and restored.
#[derive(Clone)]
struct ParserState<'de> {
    /// Stack tracking nested containers.
    stack: Vec<ContextState>,
    /// Cached event for `peek_event`.
    event_peek: Option<ParseEvent<'de>>,
    /// Start offset of the peeked event's first token (for capture_raw).
    peek_start_offset: Option<usize>,
    /// Whether the root value has started.
    root_started: bool,
    /// Whether the root value has fully completed.
    root_complete: bool,
    /// Offset of the last token's start (span.offset).
    last_token_start: usize,
    /// Scanner position (for save/restore).
    scanner_pos: usize,
}

/// JSON parser using Scanner directly (no adapter layer).
///
/// The const generic `TRUSTED_UTF8` controls UTF-8 validation:
/// - `TRUSTED_UTF8=true`: skip UTF-8 validation (input came from `&str`)
/// - `TRUSTED_UTF8=false`: validate UTF-8 (input came from `&[u8]`)
pub struct JsonParser<'de, const TRUSTED_UTF8: bool = false> {
    input: &'de [u8],
    scanner: Scanner,
    state: ParserState<'de>,
    /// Counter for save points.
    save_counter: u64,
    /// Saved states for restore functionality.
    saved_states: Vec<(u64, ParserState<'de>)>,
}

#[derive(Debug, Clone)]
enum ContextState {
    Object(ObjectState),
    Array(ArrayState),
}

pub(crate) struct OrderedObjectProbeSave {
    scanner: Scanner,
    stack_len: usize,
    stack_top: Option<ContextState>,
    root_started: bool,
    root_complete: bool,
    last_token_start: usize,
    scanner_pos: usize,
}

#[derive(Debug, Clone, Copy)]
enum ObjectState {
    KeyOrEnd,
    Value,
    CommaOrEnd,
}

#[derive(Debug, Clone, Copy)]
enum ArrayState {
    ValueOrEnd,
    CommaOrEnd,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DelimKind {
    Object,
    Array,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NextAction {
    ObjectKey,
    ObjectValue,
    ObjectComma,
    ArrayValue,
    ArrayComma,
    RootValue,
    RootFinished,
}

impl<'de, const TRUSTED_UTF8: bool> JsonParser<'de, TRUSTED_UTF8> {
    pub fn new(input: &'de [u8]) -> Self {
        Self {
            input,
            scanner: Scanner::new(),
            state: ParserState {
                stack: Vec::new(),
                event_peek: None,
                peek_start_offset: None,
                root_started: false,
                root_complete: false,
                last_token_start: 0,
                scanner_pos: 0,
            },
            save_counter: 0,
            saved_states: Vec::new(),
        }
    }

    /// Create a JSONC parser that accepts `//` and `/* */` comments.
    pub fn new_jsonc(input: &'de [u8]) -> Self {
        Self {
            input,
            scanner: Scanner::new_with_comments(),
            state: ParserState {
                stack: Vec::new(),
                event_peek: None,
                peek_start_offset: None,
                root_started: false,
                root_complete: false,
                last_token_start: 0,
                scanner_pos: 0,
            },
            save_counter: 0,
            saved_states: Vec::new(),
        }
    }

    /// Scan and materialize the next token directly.
    #[inline]
    fn consume_token(&mut self) -> Result<MaterializedToken<'de>, ParseError> {
        let spanned = self.consume_spanned_token()?;
        let span = spanned.span;
        let kind = self.materialize_token_kind(spanned.token, span)?;

        Ok(MaterializedToken { kind, span })
    }

    #[inline]
    fn consume_spanned_token(&mut self) -> Result<scanner::SpannedToken, ParseError> {
        let spanned = self
            .scanner
            .next_token(self.input)
            .map_err(scan_error_to_parse_error)?;

        self.state.last_token_start = spanned.span.offset as usize;
        self.state.scanner_pos = self.scanner.pos();

        Ok(spanned)
    }

    #[inline]
    fn consume_punctuation_token(&mut self, byte: u8) -> Result<Option<Span>, ParseError> {
        let span = self
            .scanner
            .consume_punctuation(self.input, byte)
            .map_err(scan_error_to_parse_error)?;

        if let Some(span) = span {
            self.state.last_token_start = span.offset as usize;
            self.state.scanner_pos = self.scanner.pos();
        }

        Ok(span)
    }

    #[inline]
    fn try_consume_exact_string_token(
        &mut self,
        expected: &str,
    ) -> Result<Option<Span>, ParseError> {
        let span = self
            .scanner
            .try_consume_exact_string(self.input, expected.as_bytes())
            .map_err(scan_error_to_parse_error)?;

        if let Some(span) = span {
            self.state.last_token_start = span.offset as usize;
            self.state.scanner_pos = self.scanner.pos();
        }

        Ok(span)
    }

    #[inline]
    fn try_consume_i32_number(&mut self) -> Result<Option<(Span, i32)>, ParseError> {
        let value = self
            .scanner
            .try_consume_i32_number(self.input)
            .map_err(scan_error_to_parse_error)?;

        if let Some((span, _)) = value {
            self.state.last_token_start = span.offset as usize;
            self.state.scanner_pos = self.scanner.pos();
            self.state.root_started = true;
            self.finish_value_in_parent();
        }

        Ok(value)
    }

    fn materialize_token_kind(
        &self,
        token: ScanToken,
        span: Span,
    ) -> Result<TokenKind<'de>, ParseError> {
        let kind = match token {
            ScanToken::ObjectStart => TokenKind::ObjectStart,
            ScanToken::ObjectEnd => TokenKind::ObjectEnd,
            ScanToken::ArrayStart => TokenKind::ArrayStart,
            ScanToken::ArrayEnd => TokenKind::ArrayEnd,
            ScanToken::Colon => TokenKind::Colon,
            ScanToken::Comma => TokenKind::Comma,
            ScanToken::Null => TokenKind::Null,
            ScanToken::True => TokenKind::True,
            ScanToken::False => TokenKind::False,
            ScanToken::String {
                start,
                end,
                has_escapes,
            } => TokenKind::String(self.decode_string(start, end, has_escapes, span)?),
            ScanToken::Number { start, end, hint } => {
                let parsed = self.parse_number(start, end, hint)?;
                match parsed {
                    ParsedNumber::U64(n) => TokenKind::U64(n),
                    ParsedNumber::I64(n) => TokenKind::I64(n),
                    ParsedNumber::U128(n) => TokenKind::U128(n),
                    ParsedNumber::I128(n) => TokenKind::I128(n),
                    ParsedNumber::F64(n) => TokenKind::F64(n),
                }
            }
            ScanToken::Eof => TokenKind::Eof,
        };
        Ok(kind)
    }

    pub(crate) fn parse_number(
        &self,
        start: usize,
        end: usize,
        hint: scanner::NumberHint,
    ) -> Result<ParsedNumber, ParseError> {
        if TRUSTED_UTF8 {
            // SAFETY: Input came from &str, so it's valid UTF-8
            unsafe { scanner::parse_number_unchecked(self.input, start, end, hint) }
        } else {
            scanner::parse_number(self.input, start, end, hint)
        }
        .map_err(scan_error_to_parse_error)
    }

    #[inline]
    pub(crate) fn number_text(
        &self,
        start: usize,
        end: usize,
        span: Span,
    ) -> Result<&'de str, ParseError> {
        let slice = &self.input[start..end];
        if TRUSTED_UTF8 {
            // SAFETY: Input came from &str, so it is valid UTF-8.
            Ok(unsafe { str::from_utf8_unchecked(slice) })
        } else {
            str::from_utf8(slice).map_err(|_| invalid_utf8_parse_error(span))
        }
    }

    #[inline]
    pub(crate) fn decode_string(
        &self,
        start: usize,
        end: usize,
        has_escapes: bool,
        span: Span,
    ) -> Result<Cow<'de, str>, ParseError> {
        if !has_escapes {
            self.borrow_string_no_escapes(start, end, span)
                .map(Cow::Borrowed)
        } else if TRUSTED_UTF8 {
            // SAFETY: Caller guarantees input is valid UTF-8
            Ok(Cow::Owned(
                unsafe { scanner::decode_string_owned_unchecked(self.input, start, end) }
                    .map_err(scan_error_to_parse_error)?,
            ))
        } else {
            Ok(Cow::Owned(
                scanner::decode_string_owned(self.input, start, end)
                    .map_err(scan_error_to_parse_error)?,
            ))
        }
    }

    #[inline]
    fn borrow_string_no_escapes(
        &self,
        start: usize,
        end: usize,
        span: Span,
    ) -> Result<&'de str, ParseError> {
        let slice = &self.input[start..end];
        if TRUSTED_UTF8 {
            // SAFETY: The input came from &str, and the scanner already reported
            // this token as a string without escapes.
            Ok(unsafe { core::str::from_utf8_unchecked(slice) })
        } else {
            core::str::from_utf8(slice).map_err(|_| invalid_utf8_parse_error(span))
        }
    }

    #[inline]
    fn decode_field_key(
        &self,
        start: usize,
        end: usize,
        has_escapes: bool,
        span: Span,
    ) -> Result<JsonFieldKey<'de>, ParseError> {
        if !has_escapes {
            self.borrow_string_no_escapes(start, end, span)
                .map(JsonFieldKey::Borrowed)
        } else {
            self.decode_string(start, end, has_escapes, span)
                .map(JsonFieldKey::Decoded)
        }
    }

    #[inline]
    pub(crate) fn field_key_unescaped_bytes(&self, key: &JsonFieldKeyInput<'de>) -> Option<&[u8]> {
        match key {
            JsonFieldKeyInput::Raw {
                start,
                end,
                has_escapes: false,
                ..
            } => Some(&self.input[*start..*end]),
            JsonFieldKeyInput::Raw {
                has_escapes: true, ..
            }
            | JsonFieldKeyInput::Materialized(_) => None,
        }
    }

    #[inline]
    pub(crate) fn field_key_matches(
        &self,
        key: &JsonFieldKeyInput<'de>,
        expected: &str,
    ) -> Result<bool, ParseError> {
        match key {
            JsonFieldKeyInput::Raw {
                start,
                end,
                has_escapes,
                span,
            } => {
                if !has_escapes {
                    Ok(&self.input[*start..*end] == expected.as_bytes())
                } else {
                    self.decode_string(*start, *end, *has_escapes, *span)
                        .map(|decoded| decoded.as_ref() == expected)
                }
            }
            JsonFieldKeyInput::Materialized(key) => Ok(key.as_str() == expected),
        }
    }

    #[inline]
    pub(crate) fn materialize_field_key(
        &self,
        key: JsonFieldKeyInput<'de>,
    ) -> Result<JsonFieldKey<'de>, ParseError> {
        match key {
            JsonFieldKeyInput::Raw {
                start,
                end,
                has_escapes,
                span,
            } => self.decode_field_key(start, end, has_escapes, span),
            JsonFieldKeyInput::Materialized(key) => Ok(key),
        }
    }

    fn unexpected_scan_token(
        &self,
        token: &scanner::SpannedToken,
        expected: &'static str,
    ) -> ParseError {
        ParseError::new(
            token.span,
            DeserializeErrorKind::UnexpectedToken {
                got: format!("{:?}", token.token).into(),
                expected,
            },
        )
    }

    fn expect_colon_token(&mut self) -> Result<(), ParseError> {
        if self.consume_punctuation_token(b':')?.is_some() {
            return Ok(());
        }

        let token = self.consume_spanned_token()?;
        if !matches!(token.token, ScanToken::Colon) {
            return Err(self.unexpected_scan_token(&token, "':'"));
        }
        Ok(())
    }

    fn parse_value_start_with_token(
        &mut self,
        first: Option<MaterializedToken<'de>>,
    ) -> Result<ParseEvent<'de>, ParseError> {
        let value = self.parse_direct_value_start_with_token(first)?;
        let span = value.span();
        let kind = match value {
            JsonValueStart::Object(_) => ParseEventKind::StructStart(ContainerKind::Object),
            JsonValueStart::Array(_) => ParseEventKind::SequenceStart(ContainerKind::Array),
            JsonValueStart::Scalar(scalar, _) => ParseEventKind::Scalar(scalar),
        };
        Ok(ParseEvent::new(kind, span))
    }

    fn parse_direct_value_start_with_token(
        &mut self,
        first: Option<MaterializedToken<'de>>,
    ) -> Result<JsonValueStart<'de>, ParseError> {
        let token = match first {
            Some(tok) => tok,
            None => self.consume_token()?,
        };

        self.state.root_started = true;

        let span = token.span;
        match token.kind {
            TokenKind::ObjectStart => {
                self.state
                    .stack
                    .push(ContextState::Object(ObjectState::KeyOrEnd));
                Ok(JsonValueStart::Object(span))
            }
            TokenKind::ArrayStart => {
                self.state
                    .stack
                    .push(ContextState::Array(ArrayState::ValueOrEnd));
                Ok(JsonValueStart::Array(span))
            }
            TokenKind::String(s) => {
                self.finish_value_in_parent();
                Ok(JsonValueStart::Scalar(ScalarValue::Str(s), span))
            }
            TokenKind::True => {
                self.finish_value_in_parent();
                Ok(JsonValueStart::Scalar(ScalarValue::Bool(true), span))
            }
            TokenKind::False => {
                self.finish_value_in_parent();
                Ok(JsonValueStart::Scalar(ScalarValue::Bool(false), span))
            }
            TokenKind::Null => {
                self.finish_value_in_parent();
                Ok(JsonValueStart::Scalar(ScalarValue::Null, span))
            }
            TokenKind::U64(n) => {
                self.finish_value_in_parent();
                Ok(JsonValueStart::Scalar(ScalarValue::U64(n), span))
            }
            TokenKind::I64(n) => {
                self.finish_value_in_parent();
                Ok(JsonValueStart::Scalar(ScalarValue::I64(n), span))
            }
            TokenKind::U128(n) => {
                self.finish_value_in_parent();
                Ok(JsonValueStart::Scalar(
                    ScalarValue::Str(Cow::Owned(n.to_string())),
                    span,
                ))
            }
            TokenKind::I128(n) => {
                self.finish_value_in_parent();
                Ok(JsonValueStart::Scalar(
                    ScalarValue::Str(Cow::Owned(n.to_string())),
                    span,
                ))
            }
            TokenKind::F64(n) => {
                self.finish_value_in_parent();
                Ok(JsonValueStart::Scalar(ScalarValue::F64(n), span))
            }
            TokenKind::ObjectEnd | TokenKind::ArrayEnd => Err(self.unexpected(&token, "value")),
            TokenKind::Comma | TokenKind::Colon => Err(self.unexpected(&token, "value")),
            TokenKind::Eof => Err(ParseError::new(
                span,
                DeserializeErrorKind::UnexpectedEof { expected: "value" },
            )),
        }
    }

    fn finish_value_in_parent(&mut self) {
        if let Some(context) = self.state.stack.last_mut() {
            match context {
                ContextState::Object(state) => *state = ObjectState::CommaOrEnd,
                ContextState::Array(state) => *state = ArrayState::CommaOrEnd,
            }
        } else if self.state.root_started {
            self.state.root_complete = true;
        }
    }

    fn unexpected(&self, token: &MaterializedToken<'de>, expected: &'static str) -> ParseError {
        ParseError::new(
            token.span,
            DeserializeErrorKind::UnexpectedToken {
                got: format!("{:?}", token.kind).into(),
                expected,
            },
        )
    }

    /// Skip a JSON value by scanning tokens without full materialization.
    fn skip_value_tokens(&mut self) -> Result<Span, ParseError> {
        let first = self
            .scanner
            .next_token(self.input)
            .map_err(scan_error_to_parse_error)?;
        let start = first.span.offset as usize;
        self.state.scanner_pos = self.scanner.pos();

        match first.token {
            ScanToken::ObjectStart => self.skip_container(DelimKind::Object)?,
            ScanToken::ArrayStart => self.skip_container(DelimKind::Array)?,
            ScanToken::String { .. }
            | ScanToken::Number { .. }
            | ScanToken::True
            | ScanToken::False
            | ScanToken::Null => {}
            ScanToken::ObjectEnd | ScanToken::ArrayEnd | ScanToken::Comma | ScanToken::Colon => {
                return Err(ParseError::new(
                    first.span,
                    DeserializeErrorKind::UnexpectedToken {
                        got: format!("{:?}", first.token).into(),
                        expected: "value",
                    },
                ));
            }
            ScanToken::Eof => {
                return Err(ParseError::new(
                    first.span,
                    DeserializeErrorKind::UnexpectedEof { expected: "value" },
                ));
            }
        }

        let end = self.scanner.pos();
        Ok(Span::new(start, end - start))
    }

    fn skip_container(&mut self, start_kind: DelimKind) -> Result<(), ParseError> {
        let mut stack = alloc::vec![start_kind];
        while let Some(current) = stack.last().copied() {
            let spanned = self
                .scanner
                .next_token(self.input)
                .map_err(scan_error_to_parse_error)?;
            self.state.scanner_pos = self.scanner.pos();

            match spanned.token {
                ScanToken::ObjectStart => stack.push(DelimKind::Object),
                ScanToken::ArrayStart => stack.push(DelimKind::Array),
                ScanToken::ObjectEnd => {
                    if current != DelimKind::Object {
                        return Err(ParseError::new(
                            spanned.span,
                            DeserializeErrorKind::UnexpectedToken {
                                got: "'}'".into(),
                                expected: "']'",
                            },
                        ));
                    }
                    stack.pop();
                }
                ScanToken::ArrayEnd => {
                    if current != DelimKind::Array {
                        return Err(ParseError::new(
                            spanned.span,
                            DeserializeErrorKind::UnexpectedToken {
                                got: "']'".into(),
                                expected: "'}'",
                            },
                        ));
                    }
                    stack.pop();
                }
                ScanToken::Eof => {
                    return Err(ParseError::new(
                        spanned.span,
                        DeserializeErrorKind::UnexpectedEof { expected: "value" },
                    ));
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn determine_action(&self) -> NextAction {
        if let Some(context) = self.state.stack.last() {
            match context {
                ContextState::Object(state) => match state {
                    ObjectState::KeyOrEnd => NextAction::ObjectKey,
                    ObjectState::Value => NextAction::ObjectValue,
                    ObjectState::CommaOrEnd => NextAction::ObjectComma,
                },
                ContextState::Array(state) => match state {
                    ArrayState::ValueOrEnd => NextAction::ArrayValue,
                    ArrayState::CommaOrEnd => NextAction::ArrayComma,
                },
            }
        } else if self.state.root_complete {
            NextAction::RootFinished
        } else {
            NextAction::RootValue
        }
    }

    fn produce_event(&mut self) -> Result<Option<ParseEvent<'de>>, ParseError> {
        loop {
            match self.determine_action() {
                NextAction::ObjectKey => {
                    let token = self.consume_token()?;
                    let span = token.span;
                    match token.kind {
                        TokenKind::ObjectEnd => {
                            self.state.stack.pop();
                            self.finish_value_in_parent();
                            return Ok(Some(ParseEvent::new(ParseEventKind::StructEnd, span)));
                        }
                        TokenKind::String(name) => {
                            self.expect_colon_token()?;
                            if let Some(ContextState::Object(state)) = self.state.stack.last_mut() {
                                *state = ObjectState::Value;
                            }
                            return Ok(Some(ParseEvent::new(
                                ParseEventKind::FieldKey(FieldKey::new(
                                    name,
                                    FieldLocationHint::KeyValue,
                                )),
                                span,
                            )));
                        }
                        TokenKind::Eof => {
                            return Err(ParseError::new(
                                span,
                                DeserializeErrorKind::UnexpectedEof {
                                    expected: "field name or '}'",
                                },
                            ));
                        }
                        _ => return Err(self.unexpected(&token, "field name or '}'")),
                    }
                }
                NextAction::ObjectValue => {
                    return self.parse_value_start_with_token(None).map(Some);
                }
                NextAction::ObjectComma => {
                    let token = self.consume_token()?;
                    let span = token.span;
                    match token.kind {
                        TokenKind::Comma => {
                            if let Some(ContextState::Object(state)) = self.state.stack.last_mut() {
                                *state = ObjectState::KeyOrEnd;
                            }
                            continue;
                        }
                        TokenKind::ObjectEnd => {
                            self.state.stack.pop();
                            self.finish_value_in_parent();
                            return Ok(Some(ParseEvent::new(ParseEventKind::StructEnd, span)));
                        }
                        TokenKind::Eof => {
                            return Err(ParseError::new(
                                span,
                                DeserializeErrorKind::UnexpectedEof {
                                    expected: "',' or '}'",
                                },
                            ));
                        }
                        _ => return Err(self.unexpected(&token, "',' or '}'")),
                    }
                }
                NextAction::ArrayValue => {
                    let token = self.consume_token()?;
                    let span = token.span;
                    match token.kind {
                        TokenKind::ArrayEnd => {
                            self.state.stack.pop();
                            self.finish_value_in_parent();
                            return Ok(Some(ParseEvent::new(ParseEventKind::SequenceEnd, span)));
                        }
                        TokenKind::Eof => {
                            return Err(ParseError::new(
                                span,
                                DeserializeErrorKind::UnexpectedEof {
                                    expected: "value or ']'",
                                },
                            ));
                        }
                        TokenKind::Comma | TokenKind::Colon => {
                            return Err(self.unexpected(&token, "value or ']'"));
                        }
                        _ => {
                            return self.parse_value_start_with_token(Some(token)).map(Some);
                        }
                    }
                }
                NextAction::ArrayComma => {
                    let token = self.consume_token()?;
                    let span = token.span;
                    match token.kind {
                        TokenKind::Comma => {
                            if let Some(ContextState::Array(state)) = self.state.stack.last_mut() {
                                *state = ArrayState::ValueOrEnd;
                            }
                            continue;
                        }
                        TokenKind::ArrayEnd => {
                            self.state.stack.pop();
                            self.finish_value_in_parent();
                            return Ok(Some(ParseEvent::new(ParseEventKind::SequenceEnd, span)));
                        }
                        TokenKind::Eof => {
                            return Err(ParseError::new(
                                span,
                                DeserializeErrorKind::UnexpectedEof {
                                    expected: "',' or ']'",
                                },
                            ));
                        }
                        _ => return Err(self.unexpected(&token, "',' or ']'")),
                    }
                }
                NextAction::RootValue => {
                    return self.parse_value_start_with_token(None).map(Some);
                }
                NextAction::RootFinished => {
                    return Ok(None);
                }
            }
        }
    }

    /// Get current position in input.
    fn current_offset(&self) -> usize {
        self.state.scanner_pos
    }

    pub(crate) fn read_scalar_token(&mut self) -> Result<(JsonScalarToken<'de>, Span), ParseError> {
        if let Some(event) = self.state.event_peek.take() {
            self.state.peek_start_offset = None;
            return match event.kind {
                ParseEventKind::Scalar(value) => {
                    Ok((JsonScalarToken::from_scalar_value(value), event.span))
                }
                other => Err(ParseError::new(
                    event.span,
                    DeserializeErrorKind::UnexpectedToken {
                        got: other.kind_name().into(),
                        expected: "scalar",
                    },
                )),
            };
        }

        match self.determine_action() {
            NextAction::ObjectValue | NextAction::ArrayValue | NextAction::RootValue => {
                self.consume_scalar_token()
            }
            NextAction::ArrayComma => {
                self.consume_comma_token()?;
                if let Some(ContextState::Array(state)) = self.state.stack.last_mut() {
                    *state = ArrayState::ValueOrEnd;
                }
                self.consume_scalar_token()
            }
            NextAction::RootFinished => Err(ParseError::new(
                Span::new(self.current_offset(), 0),
                DeserializeErrorKind::UnexpectedEof { expected: "scalar" },
            )),
            _ => {
                let token = self.consume_spanned_token()?;
                Err(self.unexpected_scan_token(&token, "scalar"))
            }
        }
    }

    pub(crate) fn read_scalar_input(&mut self) -> Result<JsonScalarInput<'de>, ParseError> {
        if let Some(event) = self.state.event_peek.take() {
            self.state.peek_start_offset = None;
            return match event.kind {
                ParseEventKind::Scalar(value) => Ok(JsonScalarInput::Materialized(
                    JsonScalarToken::from_scalar_value(value),
                    event.span,
                )),
                other => Err(ParseError::new(
                    event.span,
                    DeserializeErrorKind::UnexpectedToken {
                        got: other.kind_name().into(),
                        expected: "scalar",
                    },
                )),
            };
        }

        match self.determine_action() {
            NextAction::ObjectValue | NextAction::ArrayValue | NextAction::RootValue => {
                self.consume_scalar_input()
            }
            NextAction::ArrayComma => {
                self.consume_comma_token()?;
                if let Some(ContextState::Array(state)) = self.state.stack.last_mut() {
                    *state = ArrayState::ValueOrEnd;
                }
                self.consume_scalar_input()
            }
            NextAction::RootFinished => Err(ParseError::new(
                Span::new(self.current_offset(), 0),
                DeserializeErrorKind::UnexpectedEof { expected: "scalar" },
            )),
            _ => {
                let token = self.consume_spanned_token()?;
                Err(self.unexpected_scan_token(&token, "scalar"))
            }
        }
    }

    pub(crate) fn read_current_scalar_input(&mut self) -> Result<JsonScalarInput<'de>, ParseError> {
        self.consume_scalar_input()
    }

    pub(crate) fn consume_object_start(&mut self) -> Result<Span, ParseError> {
        match self.consume_value_start("object")? {
            JsonValueStart::Object(span) => Ok(span),
            value => Err(unexpected_value_start(value, "object")),
        }
    }

    pub(crate) fn consume_array_start(&mut self) -> Result<Span, ParseError> {
        match self.consume_value_start("array")? {
            JsonValueStart::Array(span) => Ok(span),
            value => Err(unexpected_value_start(value, "array")),
        }
    }

    pub(crate) fn consume_object_start_fast(&mut self) -> Result<Span, ParseError> {
        if self.state.event_peek.is_some() {
            return self.consume_object_start();
        }

        match self.determine_action() {
            NextAction::ObjectValue | NextAction::ArrayValue | NextAction::RootValue => {
                self.consume_direct_object_start()
            }
            NextAction::ArrayComma => {
                if self.consume_punctuation_token(b',')?.is_some() {
                    if let Some(ContextState::Array(state)) = self.state.stack.last_mut() {
                        *state = ArrayState::ValueOrEnd;
                    }
                    self.consume_direct_object_start()
                } else {
                    self.consume_object_start()
                }
            }
            _ => self.consume_object_start(),
        }
    }

    pub(crate) fn consume_array_start_fast(&mut self) -> Result<Span, ParseError> {
        if self.state.event_peek.is_some() {
            return self.consume_array_start();
        }

        match self.determine_action() {
            NextAction::ObjectValue | NextAction::ArrayValue | NextAction::RootValue => {
                self.consume_direct_array_start()
            }
            NextAction::ArrayComma => {
                if self.consume_punctuation_token(b',')?.is_some() {
                    if let Some(ContextState::Array(state)) = self.state.stack.last_mut() {
                        *state = ArrayState::ValueOrEnd;
                    }
                    self.consume_direct_array_start()
                } else {
                    self.consume_array_start()
                }
            }
            _ => self.consume_array_start(),
        }
    }

    fn consume_direct_object_start(&mut self) -> Result<Span, ParseError> {
        if let Some(span) = self.consume_punctuation_token(b'{')? {
            self.state.root_started = true;
            self.state
                .stack
                .push(ContextState::Object(ObjectState::KeyOrEnd));
            Ok(span)
        } else {
            self.consume_object_start()
        }
    }

    fn consume_direct_array_start(&mut self) -> Result<Span, ParseError> {
        if let Some(span) = self.consume_punctuation_token(b'[')? {
            self.state.root_started = true;
            self.state
                .stack
                .push(ContextState::Array(ArrayState::ValueOrEnd));
            Ok(span)
        } else {
            self.consume_array_start()
        }
    }

    pub(crate) fn next_object_key_or_end(&mut self) -> Result<JsonObjectKeyStep<'de>, ParseError> {
        if let Some(event) = self.state.event_peek.take() {
            self.state.peek_start_offset = None;
            return match event.kind {
                ParseEventKind::StructEnd => Ok(JsonObjectKeyStep::End),
                ParseEventKind::FieldKey(key) => {
                    let Some(name) = key.name() else {
                        return Err(ParseError::new(
                            event.span,
                            DeserializeErrorKind::InvalidValue {
                                message: "JSON object field is missing a name".into(),
                            },
                        ));
                    };
                    Ok(JsonObjectKeyStep::Field {
                        key: JsonFieldKeyInput::Materialized(JsonFieldKey::Decoded(name.clone())),
                        span: event.span,
                    })
                }
                other => Err(ParseError::new(
                    event.span,
                    DeserializeErrorKind::UnexpectedToken {
                        got: other.kind_name().into(),
                        expected: "field key or object end",
                    },
                )),
            };
        }

        loop {
            match self.determine_action() {
                NextAction::ObjectKey => {
                    let token = self.consume_spanned_token()?;
                    let span = token.span;
                    match token.token {
                        ScanToken::ObjectEnd => {
                            self.state.stack.pop();
                            self.finish_value_in_parent();
                            return Ok(JsonObjectKeyStep::End);
                        }
                        ScanToken::String {
                            start,
                            end,
                            has_escapes,
                        } => {
                            self.expect_colon_token()?;
                            if let Some(ContextState::Object(state)) = self.state.stack.last_mut() {
                                *state = ObjectState::Value;
                            }
                            return Ok(JsonObjectKeyStep::Field {
                                key: JsonFieldKeyInput::Raw {
                                    start,
                                    end,
                                    has_escapes,
                                    span,
                                },
                                span,
                            });
                        }
                        ScanToken::Eof => {
                            return Err(ParseError::new(
                                span,
                                DeserializeErrorKind::UnexpectedEof {
                                    expected: "field name or '}'",
                                },
                            ));
                        }
                        _ => return Err(self.unexpected_scan_token(&token, "field name or '}'")),
                    }
                }
                NextAction::ObjectComma => {
                    if self.consume_punctuation_token(b',')?.is_some() {
                        if let Some(ContextState::Object(state)) = self.state.stack.last_mut() {
                            *state = ObjectState::KeyOrEnd;
                        }
                        continue;
                    }

                    if self.consume_punctuation_token(b'}')?.is_some() {
                        self.state.stack.pop();
                        self.finish_value_in_parent();
                        return Ok(JsonObjectKeyStep::End);
                    }

                    let token = self.consume_spanned_token()?;
                    let span = token.span;
                    match token.token {
                        ScanToken::Comma => {
                            if let Some(ContextState::Object(state)) = self.state.stack.last_mut() {
                                *state = ObjectState::KeyOrEnd;
                            }
                        }
                        ScanToken::ObjectEnd => {
                            self.state.stack.pop();
                            self.finish_value_in_parent();
                            return Ok(JsonObjectKeyStep::End);
                        }
                        ScanToken::Eof => {
                            return Err(ParseError::new(
                                span,
                                DeserializeErrorKind::UnexpectedEof {
                                    expected: "',' or '}'",
                                },
                            ));
                        }
                        _ => return Err(self.unexpected_scan_token(&token, "',' or '}'")),
                    }
                }
                NextAction::RootFinished => {
                    return Err(ParseError::new(
                        Span::new(self.current_offset(), 0),
                        DeserializeErrorKind::UnexpectedEof {
                            expected: "field key or object end",
                        },
                    ));
                }
                _ => {
                    let token = self.consume_spanned_token()?;
                    return Err(self.unexpected_scan_token(&token, "field key or object end"));
                }
            }
        }
    }

    pub(crate) fn next_ordered_object_i32_or_key(
        &mut self,
        expected: &str,
    ) -> Result<JsonObjectOrderedI32Step<'de>, ParseError> {
        if self.state.event_peek.is_some() {
            return self
                .next_ordered_object_scalar_or_key(expected)
                .map(Into::into);
        }

        loop {
            match self.determine_action() {
                NextAction::ObjectKey => {
                    if let Some(span) = self.try_consume_exact_string_token(expected)? {
                        self.expect_colon_token()?;
                        if let Some(ContextState::Object(state)) = self.state.stack.last_mut() {
                            *state = ObjectState::Value;
                        }
                        if let Some((_, value)) = self.try_consume_i32_number()? {
                            return Ok(JsonObjectOrderedI32Step::Matched { span, value });
                        }

                        let value = self.consume_scalar_input()?;
                        return Ok(JsonObjectOrderedI32Step::MatchedInput { span, value });
                    }

                    return self
                        .next_ordered_object_scalar_or_key(expected)
                        .map(Into::into);
                }
                NextAction::ObjectComma => {
                    if self.consume_punctuation_token(b',')?.is_some() {
                        if let Some(ContextState::Object(state)) = self.state.stack.last_mut() {
                            *state = ObjectState::KeyOrEnd;
                        }
                        continue;
                    }

                    if self.consume_punctuation_token(b'}')?.is_some() {
                        self.state.stack.pop();
                        self.finish_value_in_parent();
                        return Ok(JsonObjectOrderedI32Step::End);
                    }

                    let token = self.consume_spanned_token()?;
                    let span = token.span;
                    match token.token {
                        ScanToken::Comma => {
                            if let Some(ContextState::Object(state)) = self.state.stack.last_mut() {
                                *state = ObjectState::KeyOrEnd;
                            }
                        }
                        ScanToken::ObjectEnd => {
                            self.state.stack.pop();
                            self.finish_value_in_parent();
                            return Ok(JsonObjectOrderedI32Step::End);
                        }
                        ScanToken::Eof => {
                            return Err(ParseError::new(
                                span,
                                DeserializeErrorKind::UnexpectedEof {
                                    expected: "',' or '}'",
                                },
                            ));
                        }
                        _ => return Err(self.unexpected_scan_token(&token, "',' or '}'")),
                    }
                }
                NextAction::RootFinished => {
                    return Err(ParseError::new(
                        Span::new(self.current_offset(), 0),
                        DeserializeErrorKind::UnexpectedEof {
                            expected: "field key or object end",
                        },
                    ));
                }
                _ => {
                    let token = self.consume_spanned_token()?;
                    return Err(self.unexpected_scan_token(&token, "field key or object end"));
                }
            }
        }
    }

    pub(crate) fn next_ordered_object_scalar_or_key(
        &mut self,
        expected: &str,
    ) -> Result<JsonObjectOrderedScalarStep<'de>, ParseError> {
        if self.state.event_peek.is_some() {
            return self.next_object_key_or_end().map(|step| match step {
                JsonObjectKeyStep::Field { key, span } => {
                    JsonObjectOrderedScalarStep::Field { key, span }
                }
                JsonObjectKeyStep::End => JsonObjectOrderedScalarStep::End,
            });
        }

        loop {
            match self.determine_action() {
                NextAction::ObjectKey => {
                    let token = self.consume_spanned_token()?;
                    let span = token.span;
                    match token.token {
                        ScanToken::ObjectEnd => {
                            self.state.stack.pop();
                            self.finish_value_in_parent();
                            return Ok(JsonObjectOrderedScalarStep::End);
                        }
                        ScanToken::String {
                            start,
                            end,
                            has_escapes,
                        } => {
                            self.expect_colon_token()?;
                            if let Some(ContextState::Object(state)) = self.state.stack.last_mut() {
                                *state = ObjectState::Value;
                            }
                            if !has_escapes && &self.input[start..end] == expected.as_bytes() {
                                let value = self.consume_scalar_input()?;
                                return Ok(JsonObjectOrderedScalarStep::Matched { span, value });
                            }
                            return Ok(JsonObjectOrderedScalarStep::Field {
                                key: JsonFieldKeyInput::Raw {
                                    start,
                                    end,
                                    has_escapes,
                                    span,
                                },
                                span,
                            });
                        }
                        ScanToken::Eof => {
                            return Err(ParseError::new(
                                span,
                                DeserializeErrorKind::UnexpectedEof {
                                    expected: "field name or '}'",
                                },
                            ));
                        }
                        _ => return Err(self.unexpected_scan_token(&token, "field name or '}'")),
                    }
                }
                NextAction::ObjectComma => {
                    if self.consume_punctuation_token(b',')?.is_some() {
                        if let Some(ContextState::Object(state)) = self.state.stack.last_mut() {
                            *state = ObjectState::KeyOrEnd;
                        }
                        continue;
                    }

                    if self.consume_punctuation_token(b'}')?.is_some() {
                        self.state.stack.pop();
                        self.finish_value_in_parent();
                        return Ok(JsonObjectOrderedScalarStep::End);
                    }

                    let token = self.consume_spanned_token()?;
                    let span = token.span;
                    match token.token {
                        ScanToken::Comma => {
                            if let Some(ContextState::Object(state)) = self.state.stack.last_mut() {
                                *state = ObjectState::KeyOrEnd;
                            }
                        }
                        ScanToken::ObjectEnd => {
                            self.state.stack.pop();
                            self.finish_value_in_parent();
                            return Ok(JsonObjectOrderedScalarStep::End);
                        }
                        ScanToken::Eof => {
                            return Err(ParseError::new(
                                span,
                                DeserializeErrorKind::UnexpectedEof {
                                    expected: "',' or '}'",
                                },
                            ));
                        }
                        _ => return Err(self.unexpected_scan_token(&token, "',' or '}'")),
                    }
                }
                NextAction::RootFinished => {
                    return Err(ParseError::new(
                        Span::new(self.current_offset(), 0),
                        DeserializeErrorKind::UnexpectedEof {
                            expected: "field key or object end",
                        },
                    ));
                }
                _ => {
                    let token = self.consume_spanned_token()?;
                    return Err(self.unexpected_scan_token(&token, "field key or object end"));
                }
            }
        }
    }

    #[inline(never)]
    pub(crate) fn try_consume_ordered_i32_object_fields(
        &mut self,
        expected: &[&str],
        spans: &mut [Span],
        values: &mut [i32],
    ) -> Result<bool, ParseError> {
        debug_assert_eq!(expected.len(), spans.len());
        debug_assert_eq!(expected.len(), values.len());

        if self.state.event_peek.is_some() {
            return Ok(false);
        }

        let save = self.save_ordered_i32_object_probe();
        let matched = self.try_consume_ordered_i32_object_fields_inner(expected, spans, values)?;
        if !matched {
            self.restore_ordered_i32_object_probe(save);
        }
        Ok(matched)
    }

    #[inline(never)]
    #[cfg(all(feature = "jit", target_os = "macos", target_arch = "aarch64"))]
    pub(crate) fn try_consume_ordered_i32_object(
        &mut self,
        expected: &[&str],
        spans: &mut [Span],
        values: &mut [i32],
    ) -> Result<bool, ParseError> {
        debug_assert_eq!(expected.len(), spans.len());
        debug_assert_eq!(expected.len(), values.len());

        if expected.is_empty() || self.state.event_peek.is_some() {
            return Ok(false);
        }

        let save = self.save_ordered_object_probe();
        self.consume_object_start_fast()?;
        let matched = self.try_consume_ordered_i32_object_fields_inner(expected, spans, values)?;
        if !matched {
            self.restore_ordered_object_probe(save);
        }
        Ok(matched)
    }

    #[cfg(all(feature = "jit", target_os = "macos", target_arch = "aarch64"))]
    pub(crate) fn try_consume_ordered_scalar_object_with<E, W>(
        &mut self,
        expected: &[&str],
        mut consume: W,
    ) -> Result<bool, E>
    where
        E: From<ParseError>,
        W: FnMut(
            &JsonParser<'de, TRUSTED_UTF8>,
            usize,
            Span,
            scanner::SpannedToken,
        ) -> Result<(), E>,
    {
        if expected.is_empty() || self.state.event_peek.is_some() {
            return Ok(false);
        }

        let save = self.save_ordered_object_probe();
        self.consume_object_start_fast().map_err(E::from)?;
        let matched = self.try_consume_ordered_scalar_object_inner(expected, &mut consume)?;
        if !matched {
            self.restore_ordered_object_probe(save);
        }
        Ok(matched)
    }

    #[cfg(all(feature = "jit", target_os = "macos", target_arch = "aarch64"))]
    pub(crate) fn save_native_probe(&self) -> OrderedObjectProbeSave {
        self.save_ordered_object_probe()
    }

    #[cfg(all(feature = "jit", target_os = "macos", target_arch = "aarch64"))]
    pub(crate) fn restore_native_probe(&mut self, save: OrderedObjectProbeSave) {
        self.restore_ordered_object_probe(save);
    }

    #[inline(never)]
    fn save_ordered_i32_object_probe(&self) -> OrderedObjectProbeSave {
        self.save_ordered_object_probe()
    }

    #[inline(never)]
    fn save_ordered_object_probe(&self) -> OrderedObjectProbeSave {
        OrderedObjectProbeSave {
            scanner: self.scanner.clone(),
            stack_len: self.state.stack.len(),
            stack_top: self.state.stack.last().cloned(),
            root_started: self.state.root_started,
            root_complete: self.state.root_complete,
            last_token_start: self.state.last_token_start,
            scanner_pos: self.state.scanner_pos,
        }
    }

    #[cold]
    fn restore_ordered_i32_object_probe(&mut self, save: OrderedObjectProbeSave) {
        self.restore_ordered_object_probe(save);
    }

    #[cold]
    fn restore_ordered_object_probe(&mut self, save: OrderedObjectProbeSave) {
        self.scanner = save.scanner;
        self.state.stack.truncate(save.stack_len);
        if let Some(top) = save.stack_top {
            if let Some(slot) = self.state.stack.last_mut() {
                *slot = top;
            } else {
                self.state.stack.push(top);
            }
        }
        self.state.root_started = save.root_started;
        self.state.root_complete = save.root_complete;
        self.state.last_token_start = save.last_token_start;
        self.state.scanner_pos = save.scanner_pos;
    }

    #[inline(never)]
    fn try_consume_ordered_i32_object_fields_inner(
        &mut self,
        expected: &[&str],
        spans: &mut [Span],
        values: &mut [i32],
    ) -> Result<bool, ParseError> {
        for (index, expected) in expected.iter().copied().enumerate() {
            if index > 0 {
                if self.determine_action() != NextAction::ObjectComma {
                    return Ok(false);
                }
                if self.consume_punctuation_token(b',')?.is_none() {
                    return Ok(false);
                }
                if let Some(ContextState::Object(state)) = self.state.stack.last_mut() {
                    *state = ObjectState::KeyOrEnd;
                } else {
                    return Ok(false);
                }
            }

            if self.determine_action() != NextAction::ObjectKey {
                return Ok(false);
            }

            let Some(span) = self.try_consume_exact_string_token(expected)? else {
                return Ok(false);
            };
            self.expect_colon_token()?;
            if let Some(ContextState::Object(state)) = self.state.stack.last_mut() {
                *state = ObjectState::Value;
            } else {
                return Ok(false);
            }

            let Some((_, value)) = self.try_consume_i32_number()? else {
                return Ok(false);
            };
            spans[index] = span;
            values[index] = value;
        }

        if self.determine_action() != NextAction::ObjectComma {
            return Ok(false);
        }
        if self.consume_punctuation_token(b'}')?.is_none() {
            return Ok(false);
        }
        self.state.stack.pop();
        self.finish_value_in_parent();
        Ok(true)
    }

    #[inline(never)]
    #[cfg(all(feature = "jit", target_os = "macos", target_arch = "aarch64"))]
    fn try_consume_ordered_scalar_object_inner<E, W>(
        &mut self,
        expected: &[&str],
        consume: &mut W,
    ) -> Result<bool, E>
    where
        E: From<ParseError>,
        W: FnMut(
            &JsonParser<'de, TRUSTED_UTF8>,
            usize,
            Span,
            scanner::SpannedToken,
        ) -> Result<(), E>,
    {
        for (index, expected) in expected.iter().copied().enumerate() {
            if index > 0 {
                if self.determine_action() != NextAction::ObjectComma {
                    return Ok(false);
                }
                if self
                    .consume_punctuation_token(b',')
                    .map_err(E::from)?
                    .is_none()
                {
                    return Ok(false);
                }
                if let Some(ContextState::Object(state)) = self.state.stack.last_mut() {
                    *state = ObjectState::KeyOrEnd;
                } else {
                    return Ok(false);
                }
            }

            if self.determine_action() != NextAction::ObjectKey {
                return Ok(false);
            }

            let Some(span) = self
                .try_consume_exact_string_token(expected)
                .map_err(E::from)?
            else {
                return Ok(false);
            };
            self.expect_colon_token().map_err(E::from)?;
            if let Some(ContextState::Object(state)) = self.state.stack.last_mut() {
                *state = ObjectState::Value;
            } else {
                return Ok(false);
            }

            let token = self.consume_spanned_token().map_err(E::from)?;
            self.validate_scalar_token(&token, "scalar")
                .map_err(E::from)?;
            self.state.root_started = true;
            self.finish_value_in_parent();
            consume(self, index, span, token)?;
        }

        if self.determine_action() != NextAction::ObjectComma {
            return Ok(false);
        }
        if self
            .consume_punctuation_token(b'}')
            .map_err(E::from)?
            .is_none()
        {
            return Ok(false);
        }
        self.state.stack.pop();
        self.finish_value_in_parent();
        Ok(true)
    }

    pub(crate) fn consume_null_if_next(&mut self) -> Result<bool, ParseError> {
        if let Some(event) = self.state.event_peek.as_ref() {
            if matches!(event.kind, ParseEventKind::Scalar(ScalarValue::Null)) {
                self.state.event_peek = None;
                self.state.peek_start_offset = None;
                return Ok(true);
            }
            return Ok(false);
        }

        match self.determine_action() {
            NextAction::ObjectValue | NextAction::ArrayValue | NextAction::RootValue => {
                if !self.next_significant_is(b'n')? {
                    return Ok(false);
                }
                self.consume_null_token()
            }
            NextAction::ArrayComma => {
                let Some((comma_pos, b',')) = self.peek_significant_byte()? else {
                    return Ok(false);
                };
                if !self.significant_after_is(comma_pos + 1, b'n')? {
                    return Ok(false);
                }

                self.consume_comma_token()?;
                if let Some(ContextState::Array(state)) = self.state.stack.last_mut() {
                    *state = ArrayState::ValueOrEnd;
                }
                self.consume_null_token()
            }
            _ => Ok(false),
        }
    }

    pub(crate) fn consume_sequence_end_if_next(&mut self) -> Result<bool, ParseError> {
        if let Some(event) = self.state.event_peek.as_ref() {
            if matches!(event.kind, ParseEventKind::SequenceEnd) {
                self.state.event_peek = None;
                self.state.peek_start_offset = None;
                return Ok(true);
            }
            return Ok(false);
        }

        match self.determine_action() {
            NextAction::ArrayValue => {
                if !self.next_significant_is(b']')? {
                    return Ok(false);
                }
                self.consume_array_end_token()
            }
            NextAction::ArrayComma => match self.peek_significant_byte()? {
                Some((_, b']')) => self.consume_array_end_token(),
                Some((comma_pos, b',')) if self.significant_after_is(comma_pos + 1, b']')? => {
                    self.consume_comma_token()?;
                    if let Some(ContextState::Array(state)) = self.state.stack.last_mut() {
                        *state = ArrayState::ValueOrEnd;
                    }
                    self.consume_array_end_token()
                }
                _ => Ok(false),
            },
            _ => Ok(false),
        }
    }

    pub(crate) fn consume_object_end_if_next(&mut self) -> Result<bool, ParseError> {
        if let Some(event) = self.state.event_peek.as_ref() {
            if matches!(event.kind, ParseEventKind::StructEnd) {
                self.state.event_peek = None;
                self.state.peek_start_offset = None;
                return Ok(true);
            }
            return Ok(false);
        }

        match self.determine_action() {
            NextAction::ObjectKey | NextAction::ObjectComma => {
                if !self.next_significant_is(b'}')? {
                    return Ok(false);
                }
                let Some(_) = self.consume_punctuation_token(b'}')? else {
                    return Ok(false);
                };
                self.state.stack.pop();
                self.finish_value_in_parent();
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    pub(crate) fn next_sequence_scalar_or_end(
        &mut self,
    ) -> Result<JsonSequenceScalarStep<'de>, ParseError> {
        if self.state.event_peek.is_some() {
            if self.consume_sequence_end_if_next()? {
                return Ok(JsonSequenceScalarStep::End);
            }
            let value = self.read_scalar_input()?;
            return Ok(JsonSequenceScalarStep::Value { value });
        }

        match self.determine_action() {
            NextAction::ArrayValue => {
                let token = self.consume_spanned_token()?;
                let span = token.span;
                match token.token {
                    ScanToken::ArrayEnd => {
                        self.state.stack.pop();
                        self.finish_value_in_parent();
                        Ok(JsonSequenceScalarStep::End)
                    }
                    ScanToken::Eof => Err(ParseError::new(
                        span,
                        DeserializeErrorKind::UnexpectedEof {
                            expected: "scalar or ']'",
                        },
                    )),
                    _ => {
                        let value = self.finish_scalar_input_token(token, "scalar or ']'")?;
                        Ok(JsonSequenceScalarStep::Value { value })
                    }
                }
            }
            NextAction::ArrayComma => {
                let token = self.consume_spanned_token()?;
                let span = token.span;
                match token.token {
                    ScanToken::Comma => {
                        if let Some(ContextState::Array(state)) = self.state.stack.last_mut() {
                            *state = ArrayState::ValueOrEnd;
                        }
                        let token = self.consume_spanned_token()?;
                        let span = token.span;
                        match token.token {
                            ScanToken::ArrayEnd => {
                                self.state.stack.pop();
                                self.finish_value_in_parent();
                                Ok(JsonSequenceScalarStep::End)
                            }
                            ScanToken::Eof => Err(ParseError::new(
                                span,
                                DeserializeErrorKind::UnexpectedEof {
                                    expected: "scalar or ']'",
                                },
                            )),
                            _ => {
                                let value =
                                    self.finish_scalar_input_token(token, "scalar or ']'")?;
                                Ok(JsonSequenceScalarStep::Value { value })
                            }
                        }
                    }
                    ScanToken::ArrayEnd => {
                        self.state.stack.pop();
                        self.finish_value_in_parent();
                        Ok(JsonSequenceScalarStep::End)
                    }
                    ScanToken::Eof => Err(ParseError::new(
                        span,
                        DeserializeErrorKind::UnexpectedEof {
                            expected: "',' or ']'",
                        },
                    )),
                    _ => Err(self.unexpected_scan_token(&token, "',' or ']'")),
                }
            }
            NextAction::RootFinished => Err(ParseError::new(
                Span::new(self.current_offset(), 0),
                DeserializeErrorKind::UnexpectedEof {
                    expected: "scalar or ']'",
                },
            )),
            _ => {
                let token = self.consume_spanned_token()?;
                Err(self.unexpected_scan_token(&token, "scalar or ']'"))
            }
        }
    }

    fn next_significant_is(&self, expected: u8) -> Result<bool, ParseError> {
        Ok(matches!(
            self.peek_significant_byte()?,
            Some((_, byte)) if byte == expected
        ))
    }

    fn significant_after_is(&self, pos: usize, expected: u8) -> Result<bool, ParseError> {
        Ok(matches!(
            self.scanner
                .peek_significant_byte_from(self.input, pos)
                .map_err(scan_error_to_parse_error)?,
            Some((_, byte)) if byte == expected
        ))
    }

    fn peek_significant_byte(&self) -> Result<Option<(usize, u8)>, ParseError> {
        self.scanner
            .peek_significant_byte(self.input)
            .map_err(scan_error_to_parse_error)
    }

    fn consume_comma_token(&mut self) -> Result<(), ParseError> {
        let token = self.consume_token()?;
        if matches!(token.kind, TokenKind::Comma) {
            return Ok(());
        }
        Err(self.unexpected(&token, "','"))
    }

    fn validate_scalar_token(
        &self,
        token: &scanner::SpannedToken,
        expected: &'static str,
    ) -> Result<(), ParseError> {
        match token.token {
            ScanToken::Null
            | ScanToken::True
            | ScanToken::False
            | ScanToken::String { .. }
            | ScanToken::Number { .. } => Ok(()),
            ScanToken::ObjectStart
            | ScanToken::ObjectEnd
            | ScanToken::ArrayStart
            | ScanToken::ArrayEnd
            | ScanToken::Colon
            | ScanToken::Comma => Err(self.unexpected_scan_token(token, expected)),
            ScanToken::Eof => Err(ParseError::new(
                token.span,
                DeserializeErrorKind::UnexpectedEof { expected },
            )),
        }
    }

    fn scalar_from_token(
        &self,
        token: scanner::SpannedToken,
        expected: &'static str,
    ) -> Result<(JsonScalarToken<'de>, Span), ParseError> {
        let span = token.span;
        let value = match token.token {
            ScanToken::Null => JsonScalarToken::Null,
            ScanToken::True => JsonScalarToken::Bool(true),
            ScanToken::False => JsonScalarToken::Bool(false),
            ScanToken::String {
                start,
                end,
                has_escapes,
            } => JsonScalarToken::Str(self.decode_string(start, end, has_escapes, span)?),
            ScanToken::Number { start, end, hint } => match self.parse_number(start, end, hint)? {
                ParsedNumber::U64(value) => JsonScalarToken::U64(value),
                ParsedNumber::I64(value) => JsonScalarToken::I64(value),
                ParsedNumber::U128(value) => JsonScalarToken::U128(value),
                ParsedNumber::I128(value) => JsonScalarToken::I128(value),
                ParsedNumber::F64(value) => JsonScalarToken::F64(value),
            },
            ScanToken::ObjectStart
            | ScanToken::ObjectEnd
            | ScanToken::ArrayStart
            | ScanToken::ArrayEnd
            | ScanToken::Colon
            | ScanToken::Comma => return Err(self.unexpected_scan_token(&token, expected)),
            ScanToken::Eof => {
                return Err(ParseError::new(
                    span,
                    DeserializeErrorKind::UnexpectedEof { expected },
                ));
            }
        };
        Ok((value, span))
    }

    fn finish_scalar_token(
        &mut self,
        token: scanner::SpannedToken,
        expected: &'static str,
    ) -> Result<(JsonScalarToken<'de>, Span), ParseError> {
        let scalar = self.scalar_from_token(token, expected)?;

        self.state.root_started = true;
        self.finish_value_in_parent();
        Ok(scalar)
    }

    fn finish_scalar_input_token(
        &mut self,
        token: scanner::SpannedToken,
        expected: &'static str,
    ) -> Result<JsonScalarInput<'de>, ParseError> {
        self.validate_scalar_token(&token, expected)?;

        self.state.root_started = true;
        self.finish_value_in_parent();
        Ok(JsonScalarInput::Raw(token))
    }

    fn consume_scalar_input(&mut self) -> Result<JsonScalarInput<'de>, ParseError> {
        let token = self.consume_spanned_token()?;
        self.finish_scalar_input_token(token, "scalar")
    }

    fn consume_scalar_token(&mut self) -> Result<(JsonScalarToken<'de>, Span), ParseError> {
        let token = self.consume_spanned_token()?;
        self.finish_scalar_token(token, "scalar")
    }

    fn consume_null_token(&mut self) -> Result<bool, ParseError> {
        let token = self.consume_token()?;
        match token.kind {
            TokenKind::Null => {
                self.state.root_started = true;
                self.finish_value_in_parent();
                Ok(true)
            }
            _ => Err(self.unexpected(&token, "null")),
        }
    }

    fn consume_array_end_token(&mut self) -> Result<bool, ParseError> {
        let token = self.consume_token()?;
        match token.kind {
            TokenKind::ArrayEnd => {
                self.state.stack.pop();
                self.finish_value_in_parent();
                Ok(true)
            }
            _ => Err(self.unexpected(&token, "']'")),
        }
    }

    fn consume_value_start(
        &mut self,
        expected: &'static str,
    ) -> Result<JsonValueStart<'de>, ParseError> {
        if let Some(event) = self.state.event_peek.take() {
            self.state.peek_start_offset = None;
            return match event.kind {
                ParseEventKind::StructStart(ContainerKind::Object) => {
                    Ok(JsonValueStart::Object(event.span))
                }
                ParseEventKind::SequenceStart(ContainerKind::Array) => {
                    Ok(JsonValueStart::Array(event.span))
                }
                ParseEventKind::Scalar(value) => Ok(JsonValueStart::Scalar(value, event.span)),
                other => Err(ParseError::new(
                    event.span,
                    DeserializeErrorKind::UnexpectedToken {
                        got: other.kind_name().into(),
                        expected,
                    },
                )),
            };
        }

        match self.determine_action() {
            NextAction::ObjectValue | NextAction::ArrayValue | NextAction::RootValue => {
                self.parse_direct_value_start_with_token(None)
            }
            NextAction::ArrayComma => {
                self.consume_comma_token()?;
                if let Some(ContextState::Array(state)) = self.state.stack.last_mut() {
                    *state = ArrayState::ValueOrEnd;
                }
                self.parse_direct_value_start_with_token(None)
            }
            NextAction::RootFinished => Err(ParseError::new(
                Span::new(self.current_offset(), 0),
                DeserializeErrorKind::UnexpectedEof { expected },
            )),
            _ => {
                let token = self.consume_token()?;
                Err(self.unexpected(&token, expected))
            }
        }
    }
}

fn unexpected_value_start(value: JsonValueStart<'_>, expected: &'static str) -> ParseError {
    let (got, span) = match value {
        JsonValueStart::Object(span) => ("object", span),
        JsonValueStart::Array(span) => ("array", span),
        JsonValueStart::Scalar(value, span) => (value.kind_name(), span),
    };
    ParseError::new(
        span,
        DeserializeErrorKind::UnexpectedToken {
            got: got.into(),
            expected,
        },
    )
}

impl<'de, const TRUSTED_UTF8: bool> FormatParser<'de> for JsonParser<'de, TRUSTED_UTF8> {
    fn raw_capture_shape(&self) -> Option<&'static facet_core::Shape> {
        Some(crate::RawJson::SHAPE)
    }

    fn input(&self) -> Option<&'de [u8]> {
        Some(self.input)
    }

    fn next_event(&mut self) -> Result<Option<ParseEvent<'de>>, ParseError> {
        if let Some(event) = self.state.event_peek.take() {
            self.state.peek_start_offset = None;
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

        // First, drain any peeked event
        if let Some(event) = self.state.event_peek.take() {
            self.state.peek_start_offset = None;
            buf.push_back(event);
            count += 1;
        }

        // Simple implementation: just call produce_event in a loop
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

    fn save(&mut self) -> SavePoint {
        self.save_counter += 1;
        self.saved_states
            .push((self.save_counter, self.state.clone()));
        SavePoint(self.save_counter)
    }

    fn restore(&mut self, save_point: SavePoint) {
        // Find and remove the saved state
        if let Some(pos) = self
            .saved_states
            .iter()
            .position(|(id, _)| *id == save_point.0)
        {
            let (_, saved) = self.saved_states.remove(pos);
            self.state = saved;
            // Reset the scanner to the saved position
            self.scanner = Scanner::at_position(self.state.scanner_pos);
        }
    }

    fn peek_event(&mut self) -> Result<Option<ParseEvent<'de>>, ParseError> {
        if let Some(event) = self.state.event_peek.clone() {
            return Ok(Some(event));
        }
        let event = self.produce_event()?;
        if let Some(ref e) = event {
            self.state.event_peek = Some(e.clone());
            // Use the offset of the last token consumed (which is the value's first token)
            // For values, produce_event ultimately calls parse_value_start_with_token
            // which consumes the first token and sets last_token_start.
            self.state.peek_start_offset = Some(self.state.last_token_start);
        }
        Ok(event)
    }

    fn skip_value(&mut self) -> Result<(), ParseError> {
        // Handle the case where peek_event was called before skip_value
        if let Some(event) = self.state.event_peek.take() {
            self.state.peek_start_offset = None;

            // Based on the peeked event, we may need to skip the rest of a container.
            // Note: When peeking a StructStart/SequenceStart, the parser already pushed
            // to self.state.stack. We need to pop it after skipping the container.
            match event.kind {
                ParseEventKind::StructStart(_) => {
                    let res = self.skip_container(DelimKind::Object);
                    // Pop the stack entry that was pushed during peek, even if skip_container errored
                    self.state.stack.pop();
                    res?;
                    // Update the parent's state after skipping the container
                    self.finish_value_in_parent();
                }
                ParseEventKind::SequenceStart(_) => {
                    let res = self.skip_container(DelimKind::Array);
                    // Pop the stack entry that was pushed during peek, even if skip_container errored
                    self.state.stack.pop();
                    res?;
                    // Update the parent's state after skipping the container
                    self.finish_value_in_parent();
                }
                _ => {
                    // Scalar or end event - already consumed during peek.
                    // parse_value_start_with_token already called finish_value_in_parent
                    // for scalars, so we don't call it again here.
                }
            }
        } else {
            self.skip_value_tokens()?;
            self.finish_value_in_parent();
        }
        Ok(())
    }

    fn capture_raw(&mut self) -> Result<Option<&'de str>, ParseError> {
        // Handle the case where peek_event was called before capture_raw.
        // This happens when deserialize_option peeks to check for null.
        let start_offset = if let Some(event) = self.state.event_peek.take() {
            let start = self
                .state
                .peek_start_offset
                .take()
                .expect("peek_start_offset should be set when event_peek is set");

            // Based on the peeked event, we may need to skip the rest of a container.
            // Note: When peeking a StructStart/SequenceStart, the parser already pushed
            // to self.state.stack. We need to pop it after skipping the container.
            match event.kind {
                ParseEventKind::StructStart(_) => {
                    let res = self.skip_container(DelimKind::Object);
                    // Pop the stack entry that was pushed during peek, even if skip_container errored
                    self.state.stack.pop();
                    res?;
                }
                ParseEventKind::SequenceStart(_) => {
                    let res = self.skip_container(DelimKind::Array);
                    // Pop the stack entry that was pushed during peek, even if skip_container errored
                    self.state.stack.pop();
                    res?;
                }
                ParseEventKind::StructEnd | ParseEventKind::SequenceEnd => {
                    // This shouldn't happen in valid usage, but handle gracefully
                    return Err(ParseError::new(
                        Span::new(start, 0),
                        DeserializeErrorKind::InvalidValue {
                            message: "unexpected end event in capture_raw".into(),
                        },
                    ));
                }
                _ => {
                    // Scalar value - already fully consumed during peek
                }
            }

            start
        } else {
            // Normal path: no peek, consume the first token
            let first = self
                .scanner
                .next_token(self.input)
                .map_err(scan_error_to_parse_error)?;
            let start = first.span.offset as usize;
            self.state.scanner_pos = self.scanner.pos();

            // Skip the rest of the value if it's a container
            match first.token {
                ScanToken::ObjectStart => self.skip_container(DelimKind::Object)?,
                ScanToken::ArrayStart => self.skip_container(DelimKind::Array)?,
                ScanToken::ObjectEnd
                | ScanToken::ArrayEnd
                | ScanToken::Comma
                | ScanToken::Colon => {
                    return Err(ParseError::new(
                        first.span,
                        DeserializeErrorKind::UnexpectedToken {
                            got: format!("{:?}", first.token).into(),
                            expected: "value",
                        },
                    ));
                }
                ScanToken::Eof => {
                    return Err(ParseError::new(
                        first.span,
                        DeserializeErrorKind::UnexpectedEof { expected: "value" },
                    ));
                }
                _ => {
                    // Simple value - already consumed
                }
            }

            start
        };

        // Get end position
        let end_offset = self.current_offset();

        // Extract the raw slice and convert to str
        let raw_bytes = &self.input[start_offset..end_offset];
        let raw_str = core::str::from_utf8(raw_bytes).map_err(|e| {
            ParseError::new(
                Span::new(start_offset, end_offset - start_offset),
                DeserializeErrorKind::InvalidValue {
                    message: format!("invalid UTF-8 in raw JSON: {}", e).into(),
                },
            )
        })?;

        self.finish_value_in_parent();
        Ok(Some(raw_str))
    }

    fn format_namespace(&self) -> Option<&'static str> {
        Some("json")
    }

    fn current_span(&self) -> Option<Span> {
        // Return the span of the most recently consumed token
        // This is used by metadata containers to track source locations
        let offset = self.state.last_token_start;
        let len = self.current_offset().saturating_sub(offset);
        Some(Span::new(offset, len))
    }
}
