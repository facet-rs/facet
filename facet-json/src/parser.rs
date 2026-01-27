extern crate alloc;

use alloc::{borrow::Cow, vec::Vec};

use facet_core::Facet as _;
use facet_format::{
    ContainerKind, DeserializeErrorKind, FieldKey, FieldLocationHint, FormatParser, ParseError,
    ParseEvent, ParseEventKind, SavePoint, ScalarValue,
};

use crate::adapter::{
    AdapterError, AdapterErrorKind, SliceAdapter, SpannedAdapterToken, Token as AdapterToken,
};
use crate::scanner::ScanErrorKind;

/// Convert an AdapterError to a ParseError.
fn adapter_error_to_parse_error(err: AdapterError) -> ParseError {
    let kind = match err.kind {
        AdapterErrorKind::Scan(scan_err) => match scan_err {
            ScanErrorKind::UnexpectedChar(ch) => DeserializeErrorKind::UnexpectedChar {
                ch,
                expected: "valid JSON token",
            },
            ScanErrorKind::UnexpectedEof(expected) => {
                DeserializeErrorKind::UnexpectedEof { expected }
            }
            ScanErrorKind::InvalidUtf8 => DeserializeErrorKind::InvalidUtf8 {
                context: [0u8; 16],
                context_len: 0,
            },
        },
        AdapterErrorKind::NeedMore => DeserializeErrorKind::UnexpectedEof {
            expected: "more data",
        },
    };
    ParseError::new(err.span, kind)
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
    /// Absolute offset (in bytes) of the next unread token.
    current_offset: usize,
    /// Offset of the last token's start (span.offset).
    last_token_start: usize,
}

/// Streaming JSON parser backed by `facet-json`'s `SliceAdapter`.
///
/// The const generic `TRUSTED_UTF8` controls UTF-8 validation:
/// - `TRUSTED_UTF8=true`: skip UTF-8 validation (input came from `&str`)
/// - `TRUSTED_UTF8=false`: validate UTF-8 (input came from `&[u8]`)
pub struct JsonParser<'de, const TRUSTED_UTF8: bool = false> {
    input: &'de [u8],
    adapter: SliceAdapter<'de, true, TRUSTED_UTF8>,
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
            adapter: SliceAdapter::new(input),
            state: ParserState {
                stack: Vec::new(),
                event_peek: None,
                peek_start_offset: None,
                root_started: false,
                root_complete: false,
                current_offset: 0,
                last_token_start: 0,
            },
            save_counter: 0,
            saved_states: Vec::new(),
        }
    }

    fn consume_token(&mut self) -> Result<SpannedAdapterToken<'de>, ParseError> {
        let token = self
            .adapter
            .next_token()
            .map_err(adapter_error_to_parse_error)?;
        self.state.last_token_start = token.span.offset;
        self.state.current_offset = token.span.offset + token.span.len;
        Ok(token)
    }

    fn expect_colon(&mut self) -> Result<(), ParseError> {
        let token = self.consume_token()?;
        if !matches!(token.token, AdapterToken::Colon) {
            return Err(self.unexpected(&token, "':'"));
        }
        Ok(())
    }

    fn parse_value_start_with_token(
        &mut self,
        first: Option<SpannedAdapterToken<'de>>,
    ) -> Result<ParseEvent<'de>, ParseError> {
        let token = match first {
            Some(tok) => tok,
            None => self.consume_token()?,
        };

        self.state.root_started = true;

        let span = token.span;
        match token.token {
            AdapterToken::ObjectStart => {
                self.state
                    .stack
                    .push(ContextState::Object(ObjectState::KeyOrEnd));
                Ok(ParseEvent::new(
                    ParseEventKind::StructStart(ContainerKind::Object),
                    span,
                ))
            }
            AdapterToken::ArrayStart => {
                self.state
                    .stack
                    .push(ContextState::Array(ArrayState::ValueOrEnd));
                Ok(ParseEvent::new(
                    ParseEventKind::SequenceStart(ContainerKind::Array),
                    span,
                ))
            }
            AdapterToken::String(s) => {
                let event = ParseEvent::new(ParseEventKind::Scalar(ScalarValue::Str(s)), span);
                self.finish_value_in_parent();
                Ok(event)
            }
            AdapterToken::True => {
                self.finish_value_in_parent();
                Ok(ParseEvent::new(
                    ParseEventKind::Scalar(ScalarValue::Bool(true)),
                    span,
                ))
            }
            AdapterToken::False => {
                self.finish_value_in_parent();
                Ok(ParseEvent::new(
                    ParseEventKind::Scalar(ScalarValue::Bool(false)),
                    span,
                ))
            }
            AdapterToken::Null => {
                self.finish_value_in_parent();
                Ok(ParseEvent::new(
                    ParseEventKind::Scalar(ScalarValue::Null),
                    span,
                ))
            }
            AdapterToken::U64(n) => {
                self.finish_value_in_parent();
                Ok(ParseEvent::new(
                    ParseEventKind::Scalar(ScalarValue::U64(n)),
                    span,
                ))
            }
            AdapterToken::I64(n) => {
                self.finish_value_in_parent();
                Ok(ParseEvent::new(
                    ParseEventKind::Scalar(ScalarValue::I64(n)),
                    span,
                ))
            }
            AdapterToken::U128(n) => {
                self.finish_value_in_parent();
                Ok(ParseEvent::new(
                    ParseEventKind::Scalar(ScalarValue::Str(Cow::Owned(n.to_string()))),
                    span,
                ))
            }
            AdapterToken::I128(n) => {
                self.finish_value_in_parent();
                Ok(ParseEvent::new(
                    ParseEventKind::Scalar(ScalarValue::Str(Cow::Owned(n.to_string()))),
                    span,
                ))
            }
            AdapterToken::F64(n) => {
                self.finish_value_in_parent();
                Ok(ParseEvent::new(
                    ParseEventKind::Scalar(ScalarValue::F64(n)),
                    span,
                ))
            }
            AdapterToken::ObjectEnd | AdapterToken::ArrayEnd => {
                Err(self.unexpected(&token, "value"))
            }
            AdapterToken::Comma | AdapterToken::Colon => Err(self.unexpected(&token, "value")),
            AdapterToken::Eof => Err(ParseError::new(
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

    fn unexpected(&self, token: &SpannedAdapterToken<'de>, expected: &'static str) -> ParseError {
        ParseError::new(
            token.span,
            DeserializeErrorKind::UnexpectedToken {
                got: format!("{:?}", token.token).into(),
                expected,
            },
        )
    }

    fn consume_value_tokens(&mut self) -> Result<(), ParseError> {
        let span = self.adapter.skip().map_err(adapter_error_to_parse_error)?;
        self.state.current_offset = span.offset + span.len;
        Ok(())
    }

    fn skip_container(&mut self, start_kind: DelimKind) -> Result<(), ParseError> {
        let mut stack = vec![start_kind];
        while let Some(current) = stack.last().copied() {
            let token = self.consume_token()?;
            match token.token {
                AdapterToken::ObjectStart => stack.push(DelimKind::Object),
                AdapterToken::ArrayStart => stack.push(DelimKind::Array),
                AdapterToken::ObjectEnd => {
                    if current != DelimKind::Object {
                        return Err(self.unexpected(&token, "'}'"));
                    }
                    stack.pop();
                    if stack.is_empty() {
                        break;
                    }
                }
                AdapterToken::ArrayEnd => {
                    if current != DelimKind::Array {
                        return Err(self.unexpected(&token, "']'"));
                    }
                    stack.pop();
                    if stack.is_empty() {
                        break;
                    }
                }
                AdapterToken::Eof => {
                    return Err(ParseError::new(
                        token.span,
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
                    match token.token {
                        AdapterToken::ObjectEnd => {
                            self.state.stack.pop();
                            self.finish_value_in_parent();
                            return Ok(Some(ParseEvent::new(ParseEventKind::StructEnd, span)));
                        }
                        AdapterToken::String(name) => {
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
                        AdapterToken::Eof => {
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
                    match token.token {
                        AdapterToken::Comma => {
                            if let Some(ContextState::Object(state)) = self.state.stack.last_mut() {
                                *state = ObjectState::KeyOrEnd;
                            }
                            continue;
                        }
                        AdapterToken::ObjectEnd => {
                            self.state.stack.pop();
                            self.finish_value_in_parent();
                            return Ok(Some(ParseEvent::new(ParseEventKind::StructEnd, span)));
                        }
                        AdapterToken::Eof => {
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
                    match token.token {
                        AdapterToken::ArrayEnd => {
                            self.state.stack.pop();
                            self.finish_value_in_parent();
                            return Ok(Some(ParseEvent::new(ParseEventKind::SequenceEnd, span)));
                        }
                        AdapterToken::Eof => {
                            return Err(ParseError::new(
                                span,
                                DeserializeErrorKind::UnexpectedEof {
                                    expected: "value or ']'",
                                },
                            ));
                        }
                        AdapterToken::Comma | AdapterToken::Colon => {
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
                    match token.token {
                        AdapterToken::Comma => {
                            if let Some(ContextState::Array(state)) = self.state.stack.last_mut() {
                                *state = ArrayState::ValueOrEnd;
                            }
                            continue;
                        }
                        AdapterToken::ArrayEnd => {
                            self.state.stack.pop();
                            self.finish_value_in_parent();
                            return Ok(Some(ParseEvent::new(ParseEventKind::SequenceEnd, span)));
                        }
                        AdapterToken::Eof => {
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

    fn next_events(&mut self, buf: &mut [ParseEvent<'de>]) -> Result<usize, ParseError> {
        if buf.is_empty() {
            return Ok(0);
        }

        let mut count = 0;

        // First, drain any peeked event
        if let Some(event) = self.state.event_peek.take() {
            self.state.peek_start_offset = None;
            buf[count] = event;
            count += 1;
        }

        // Simple implementation: just call produce_event in a loop
        while count < buf.len() {
            match self.produce_event()? {
                Some(event) => {
                    buf[count] = event;
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
            // Reset the adapter to the saved position
            self.adapter = SliceAdapter::new_with_offset(self.input, self.state.current_offset);
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
            self.consume_value_tokens()?;
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
                        facet_reflect::Span::new(start, 0),
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
            let first = self.consume_token()?;
            let start = first.span.offset;

            // Skip the rest of the value if it's a container
            match first.token {
                AdapterToken::ObjectStart => self.skip_container(DelimKind::Object)?,
                AdapterToken::ArrayStart => self.skip_container(DelimKind::Array)?,
                AdapterToken::ObjectEnd
                | AdapterToken::ArrayEnd
                | AdapterToken::Comma
                | AdapterToken::Colon => return Err(self.unexpected(&first, "value")),
                AdapterToken::Eof => {
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
        let end_offset = self.state.current_offset;

        // Extract the raw slice and convert to str
        let raw_bytes = &self.input[start_offset..end_offset];
        let raw_str = core::str::from_utf8(raw_bytes).map_err(|e| {
            ParseError::new(
                facet_reflect::Span::new(start_offset, end_offset - start_offset),
                DeserializeErrorKind::InvalidValue {
                    message: alloc::format!("invalid UTF-8 in raw JSON: {}", e).into(),
                },
            )
        })?;

        self.finish_value_in_parent();
        Ok(Some(raw_str))
    }

    fn format_namespace(&self) -> Option<&'static str> {
        Some("json")
    }

    fn current_span(&self) -> Option<facet_reflect::Span> {
        // Return the span of the most recently consumed token
        // This is used by metadata containers to track source locations
        let offset = self.state.last_token_start;
        let len = self.state.current_offset.saturating_sub(offset);
        Some(facet_reflect::Span::new(offset, len))
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
        Some(self.state.current_offset)
    }

    fn jit_set_pos(&mut self, pos: usize) {
        // Update the offset
        self.state.current_offset = pos;

        // Reset the adapter to start from the new position
        // We need to create a new adapter pointing to the remaining input
        // but preserving absolute offset semantics
        self.adapter = SliceAdapter::new_with_offset(self.input, pos);

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
        use facet_reflect::Span;

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
                message: alloc::format!("Tier-2 JIT error code: {}", error_code).into(),
            },
        };

        ParseError::new(Span::new(error_pos, 1), kind)
    }
}
