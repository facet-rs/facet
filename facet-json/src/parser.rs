extern crate alloc;

use alloc::{borrow::Cow, collections::VecDeque, format, vec::Vec};

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

/// Materialized token ready for the parser.
#[derive(Debug, Clone)]
pub struct MaterializedToken<'de> {
    pub kind: TokenKind<'de>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum TokenKind<'de> {
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

    /// Scan and materialize the next token directly.
    #[inline]
    fn consume_token(&mut self) -> Result<MaterializedToken<'de>, ParseError> {
        let mut spanned = self
            .scanner
            .next_token(self.input)
            .map_err(scan_error_to_parse_error)?;

        // Handle NeedMore by finalizing - we have full input so this is true EOF
        if matches!(spanned.token, ScanToken::NeedMore { .. }) {
            spanned = self
                .scanner
                .finalize_at_eof(self.input)
                .map_err(scan_error_to_parse_error)?;
        }

        self.state.last_token_start = spanned.span.offset as usize;
        self.state.scanner_pos = self.scanner.pos();

        let kind = match spanned.token {
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
            } => {
                let s = if !has_escapes {
                    if TRUSTED_UTF8 {
                        // SAFETY: Caller guarantees input is valid UTF-8
                        unsafe { scanner::decode_string_borrowed_unchecked(self.input, start, end) }
                            .map(Cow::Borrowed)
                            .ok_or_else(|| {
                                ParseError::new(
                                    spanned.span,
                                    DeserializeErrorKind::InvalidUtf8 {
                                        context: [0u8; 16],
                                        context_len: 0,
                                    },
                                )
                            })?
                    } else {
                        scanner::decode_string_borrowed(self.input, start, end)
                            .map(Cow::Borrowed)
                            .ok_or_else(|| {
                                ParseError::new(
                                    spanned.span,
                                    DeserializeErrorKind::InvalidUtf8 {
                                        context: [0u8; 16],
                                        context_len: 0,
                                    },
                                )
                            })?
                    }
                } else if TRUSTED_UTF8 {
                    // SAFETY: Caller guarantees input is valid UTF-8
                    Cow::Owned(
                        unsafe { scanner::decode_string_owned_unchecked(self.input, start, end) }
                            .map_err(scan_error_to_parse_error)?,
                    )
                } else {
                    Cow::Owned(
                        scanner::decode_string_owned(self.input, start, end)
                            .map_err(scan_error_to_parse_error)?,
                    )
                };
                TokenKind::String(s)
            }
            ScanToken::Number { start, end, hint } => {
                let parsed = if TRUSTED_UTF8 {
                    // SAFETY: Input came from &str, so it's valid UTF-8
                    unsafe { scanner::parse_number_unchecked(self.input, start, end, hint) }
                } else {
                    scanner::parse_number(self.input, start, end, hint)
                }
                .map_err(scan_error_to_parse_error)?;
                match parsed {
                    ParsedNumber::U64(n) => TokenKind::U64(n),
                    ParsedNumber::I64(n) => TokenKind::I64(n),
                    ParsedNumber::U128(n) => TokenKind::U128(n),
                    ParsedNumber::I128(n) => TokenKind::I128(n),
                    ParsedNumber::F64(n) => TokenKind::F64(n),
                }
            }
            ScanToken::Eof => TokenKind::Eof,
            ScanToken::NeedMore { .. } => unreachable!("handled above"),
        };

        Ok(MaterializedToken {
            kind,
            span: spanned.span,
        })
    }

    fn expect_colon(&mut self) -> Result<(), ParseError> {
        let token = self.consume_token()?;
        if !matches!(token.kind, TokenKind::Colon) {
            return Err(self.unexpected(&token, "':'"));
        }
        Ok(())
    }

    fn parse_value_start_with_token(
        &mut self,
        first: Option<MaterializedToken<'de>>,
    ) -> Result<ParseEvent<'de>, ParseError> {
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
                Ok(ParseEvent::new(
                    ParseEventKind::StructStart(ContainerKind::Object),
                    span,
                ))
            }
            TokenKind::ArrayStart => {
                self.state
                    .stack
                    .push(ContextState::Array(ArrayState::ValueOrEnd));
                Ok(ParseEvent::new(
                    ParseEventKind::SequenceStart(ContainerKind::Array),
                    span,
                ))
            }
            TokenKind::String(s) => {
                let event = ParseEvent::new(ParseEventKind::Scalar(ScalarValue::Str(s)), span);
                self.finish_value_in_parent();
                Ok(event)
            }
            TokenKind::True => {
                self.finish_value_in_parent();
                Ok(ParseEvent::new(
                    ParseEventKind::Scalar(ScalarValue::Bool(true)),
                    span,
                ))
            }
            TokenKind::False => {
                self.finish_value_in_parent();
                Ok(ParseEvent::new(
                    ParseEventKind::Scalar(ScalarValue::Bool(false)),
                    span,
                ))
            }
            TokenKind::Null => {
                self.finish_value_in_parent();
                Ok(ParseEvent::new(
                    ParseEventKind::Scalar(ScalarValue::Null),
                    span,
                ))
            }
            TokenKind::U64(n) => {
                self.finish_value_in_parent();
                Ok(ParseEvent::new(
                    ParseEventKind::Scalar(ScalarValue::U64(n)),
                    span,
                ))
            }
            TokenKind::I64(n) => {
                self.finish_value_in_parent();
                Ok(ParseEvent::new(
                    ParseEventKind::Scalar(ScalarValue::I64(n)),
                    span,
                ))
            }
            TokenKind::U128(n) => {
                self.finish_value_in_parent();
                Ok(ParseEvent::new(
                    ParseEventKind::Scalar(ScalarValue::Str(Cow::Owned(n.to_string()))),
                    span,
                ))
            }
            TokenKind::I128(n) => {
                self.finish_value_in_parent();
                Ok(ParseEvent::new(
                    ParseEventKind::Scalar(ScalarValue::Str(Cow::Owned(n.to_string()))),
                    span,
                ))
            }
            TokenKind::F64(n) => {
                self.finish_value_in_parent();
                Ok(ParseEvent::new(
                    ParseEventKind::Scalar(ScalarValue::F64(n)),
                    span,
                ))
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
            ScanToken::NeedMore { .. } => {
                return Err(ParseError::new(
                    first.span,
                    DeserializeErrorKind::UnexpectedEof {
                        expected: "more data",
                    },
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
                ScanToken::NeedMore { .. } => {
                    return Err(ParseError::new(
                        spanned.span,
                        DeserializeErrorKind::UnexpectedEof {
                            expected: "more data",
                        },
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
                            self.expect_colon()?;
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
                ScanToken::NeedMore { .. } => {
                    return Err(ParseError::new(
                        first.span,
                        DeserializeErrorKind::UnexpectedEof {
                            expected: "more data",
                        },
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

// =============================================================================
// FormatJitParser Implementation (Tier-2 JIT support)
// =============================================================================

#[cfg(feature = "jit")]
impl<'de> facet_format::FormatJitParser<'de> for JsonParser<'de> {
    type FormatJit = crate::jit::JsonJitFormat;

    fn jit_input(&self) -> &'de [u8] {
        self.input
    }

    fn jit_pos(&self) -> Option<usize> {
        // Tier-2 JIT is only safe at root boundary:
        // - No peeked event (position would be ambiguous)
        // - Empty stack (we're at root level, not inside an object/array)
        // - Root not yet started, OR root is complete
        //
        // This ensures jit_set_pos doesn't corrupt parser state machine.
        if self.state.event_peek.is_some() {
            return None;
        }
        if !self.state.stack.is_empty() {
            return None;
        }
        if self.state.root_started && !self.state.root_complete {
            // We've started parsing root but haven't finished - not safe
            return None;
        }
        Some(self.current_offset())
    }

    fn jit_set_pos(&mut self, pos: usize) {
        // Update the scanner position
        self.state.scanner_pos = pos;
        self.scanner = Scanner::at_position(pos);

        // Clear any peeked event and its offset
        self.state.event_peek = None;
        self.state.peek_start_offset = None;

        // Tier-2 JIT parsed a complete root value, so update parser state.
        // jit_pos() already enforces root-only usage, so we know:
        // - We started at root level with empty stack
        // - Tier-2 successfully parsed a complete value
        // - We're now at the position after that value
        self.state.root_started = true;
        self.state.root_complete = true;
        // Stack should already be empty (jit_pos enforces this)
        debug_assert!(self.state.stack.is_empty());
    }

    fn jit_format(&self) -> Self::FormatJit {
        crate::jit::JsonJitFormat
    }

    fn jit_error(&self, _input: &'de [u8], error_pos: usize, error_code: i32) -> ParseError {
        let kind = match error_code {
            -100 => DeserializeErrorKind::UnexpectedEof { expected: "value" },
            -101 => DeserializeErrorKind::UnexpectedToken {
                got: "non-'['".into(),
                expected: "'['",
            },
            -102 => DeserializeErrorKind::UnexpectedToken {
                got: "non-boolean".into(),
                expected: "'true' or 'false'",
            },
            -103 => DeserializeErrorKind::UnexpectedToken {
                got: "unexpected token".into(),
                expected: "',' or ']'",
            },
            _ => DeserializeErrorKind::InvalidValue {
                message: format!("Tier-2 JIT error code: {}", error_code).into(),
            },
        };

        ParseError::new(Span::new(error_pos, 1), kind)
    }
}
