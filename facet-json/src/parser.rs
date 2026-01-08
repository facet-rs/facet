extern crate alloc;

use alloc::{borrow::Cow, vec::Vec};

use facet_core::Facet as _;
use facet_format::{
    ContainerKind, FieldEvidence, FieldKey, FieldLocationHint, FormatParser, ParseEvent,
    ProbeStream, ScalarValue,
};

use crate::adapter::{SliceAdapter, SpannedAdapterToken, Token as AdapterToken};
pub use crate::error::JsonError;
use crate::error::JsonErrorKind;

/// Streaming JSON parser backed by `facet-json`'s `SliceAdapter`.
pub struct JsonParser<'de> {
    input: &'de [u8],
    adapter: SliceAdapter<'de, true>,
    stack: Vec<ContextState>,
    /// Cached event for `peek_event`.
    event_peek: Option<ParseEvent<'de>>,
    /// Start offset of the peeked event's first token (for capture_raw).
    /// This is the span.offset of the first token consumed during peek.
    peek_start_offset: Option<usize>,
    /// Whether the root value has started.
    root_started: bool,
    /// Whether the root value has fully completed.
    root_complete: bool,
    /// Absolute offset (in bytes) of the next unread token.
    current_offset: usize,
    /// Offset of the last token's start (span.offset).
    /// Used to track the start of a value during peek.
    last_token_start: usize,
}

#[derive(Debug)]
enum ContextState {
    Object(ObjectState),
    Array(ArrayState),
}

#[derive(Debug)]
enum ObjectState {
    KeyOrEnd,
    Value,
    CommaOrEnd,
}

#[derive(Debug)]
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

impl<'de> JsonParser<'de> {
    pub fn new(input: &'de [u8]) -> Self {
        Self {
            input,
            adapter: SliceAdapter::new(input),
            stack: Vec::new(),
            event_peek: None,
            peek_start_offset: None,
            root_started: false,
            root_complete: false,
            current_offset: 0,
            last_token_start: 0,
        }
    }

    fn consume_token(&mut self) -> Result<SpannedAdapterToken<'de>, JsonError> {
        let token = self.adapter.next_token().map_err(JsonError::from)?;
        self.last_token_start = token.span.offset;
        self.current_offset = token.span.offset + token.span.len;
        Ok(token)
    }

    fn expect_colon(&mut self) -> Result<(), JsonError> {
        let token = self.consume_token()?;
        if !matches!(token.token, AdapterToken::Colon) {
            return Err(self.unexpected(&token, "':'"));
        }
        Ok(())
    }

    fn parse_value_start_with_token(
        &mut self,
        first: Option<SpannedAdapterToken<'de>>,
    ) -> Result<ParseEvent<'de>, JsonError> {
        let token = match first {
            Some(tok) => tok,
            None => self.consume_token()?,
        };

        self.root_started = true;

        match token.token {
            AdapterToken::ObjectStart => {
                self.stack.push(ContextState::Object(ObjectState::KeyOrEnd));
                Ok(ParseEvent::StructStart(ContainerKind::Object))
            }
            AdapterToken::ArrayStart => {
                self.stack.push(ContextState::Array(ArrayState::ValueOrEnd));
                Ok(ParseEvent::SequenceStart(ContainerKind::Array))
            }
            AdapterToken::String(s) => {
                let event = ParseEvent::Scalar(ScalarValue::Str(s));
                self.finish_value_in_parent();
                Ok(event)
            }
            AdapterToken::True => {
                self.finish_value_in_parent();
                Ok(ParseEvent::Scalar(ScalarValue::Bool(true)))
            }
            AdapterToken::False => {
                self.finish_value_in_parent();
                Ok(ParseEvent::Scalar(ScalarValue::Bool(false)))
            }
            AdapterToken::Null => {
                self.finish_value_in_parent();
                Ok(ParseEvent::Scalar(ScalarValue::Null))
            }
            AdapterToken::U64(n) => {
                self.finish_value_in_parent();
                Ok(ParseEvent::Scalar(ScalarValue::U64(n)))
            }
            AdapterToken::I64(n) => {
                self.finish_value_in_parent();
                Ok(ParseEvent::Scalar(ScalarValue::I64(n)))
            }
            AdapterToken::U128(n) => {
                self.finish_value_in_parent();
                Ok(ParseEvent::Scalar(ScalarValue::Str(Cow::Owned(
                    n.to_string(),
                ))))
            }
            AdapterToken::I128(n) => {
                self.finish_value_in_parent();
                Ok(ParseEvent::Scalar(ScalarValue::Str(Cow::Owned(
                    n.to_string(),
                ))))
            }
            AdapterToken::F64(n) => {
                self.finish_value_in_parent();
                Ok(ParseEvent::Scalar(ScalarValue::F64(n)))
            }
            AdapterToken::ObjectEnd | AdapterToken::ArrayEnd => {
                Err(self.unexpected(&token, "value"))
            }
            AdapterToken::Comma | AdapterToken::Colon => Err(self.unexpected(&token, "value")),
            AdapterToken::Eof => Err(JsonError::new(
                JsonErrorKind::UnexpectedEof { expected: "value" },
                token.span,
            )),
        }
    }

    fn finish_value_in_parent(&mut self) {
        if let Some(context) = self.stack.last_mut() {
            match context {
                ContextState::Object(state) => *state = ObjectState::CommaOrEnd,
                ContextState::Array(state) => *state = ArrayState::CommaOrEnd,
            }
        } else if self.root_started {
            self.root_complete = true;
        }
    }

    fn unexpected(&self, token: &SpannedAdapterToken<'de>, expected: &'static str) -> JsonError {
        JsonError::new(
            JsonErrorKind::UnexpectedToken {
                got: format!("{:?}", token.token),
                expected,
            },
            token.span,
        )
    }

    fn consume_value_tokens(&mut self) -> Result<(), JsonError> {
        let span = self.adapter.skip().map_err(JsonError::from)?;
        self.current_offset = span.offset + span.len;
        Ok(())
    }

    fn skip_container(&mut self, start_kind: DelimKind) -> Result<(), JsonError> {
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
                    return Err(JsonError::new(
                        JsonErrorKind::UnexpectedEof { expected: "value" },
                        token.span,
                    ));
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// Skip a container in a separate adapter (used during probing).
    fn skip_container_in_adapter(
        &self,
        adapter: &mut SliceAdapter<'de, true>,
        start_kind: DelimKind,
    ) -> Result<(), JsonError> {
        let mut stack = vec![start_kind];
        while let Some(current) = stack.last().copied() {
            let token = adapter.next_token().map_err(JsonError::from)?;
            match token.token {
                AdapterToken::ObjectStart => stack.push(DelimKind::Object),
                AdapterToken::ArrayStart => stack.push(DelimKind::Array),
                AdapterToken::ObjectEnd => {
                    if current != DelimKind::Object {
                        return Err(JsonError::new(
                            JsonErrorKind::UnexpectedToken {
                                got: format!("{:?}", token.token),
                                expected: "'}'",
                            },
                            token.span,
                        ));
                    }
                    stack.pop();
                    if stack.is_empty() {
                        break;
                    }
                }
                AdapterToken::ArrayEnd => {
                    if current != DelimKind::Array {
                        return Err(JsonError::new(
                            JsonErrorKind::UnexpectedToken {
                                got: format!("{:?}", token.token),
                                expected: "']'",
                            },
                            token.span,
                        ));
                    }
                    stack.pop();
                    if stack.is_empty() {
                        break;
                    }
                }
                AdapterToken::Eof => {
                    return Err(JsonError::new(
                        JsonErrorKind::UnexpectedEof { expected: "value" },
                        token.span,
                    ));
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn determine_action(&self) -> NextAction {
        if let Some(context) = self.stack.last() {
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
        } else if self.root_complete {
            NextAction::RootFinished
        } else {
            NextAction::RootValue
        }
    }

    fn produce_event(&mut self) -> Result<Option<ParseEvent<'de>>, JsonError> {
        loop {
            match self.determine_action() {
                NextAction::ObjectKey => {
                    let token = self.consume_token()?;
                    match token.token {
                        AdapterToken::ObjectEnd => {
                            self.stack.pop();
                            self.finish_value_in_parent();
                            return Ok(Some(ParseEvent::StructEnd));
                        }
                        AdapterToken::String(name) => {
                            self.expect_colon()?;
                            if let Some(ContextState::Object(state)) = self.stack.last_mut() {
                                *state = ObjectState::Value;
                            }
                            return Ok(Some(ParseEvent::FieldKey(FieldKey::new(
                                name,
                                FieldLocationHint::KeyValue,
                            ))));
                        }
                        AdapterToken::Eof => {
                            return Err(JsonError::new(
                                JsonErrorKind::UnexpectedEof {
                                    expected: "field name or '}'",
                                },
                                token.span,
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
                    match token.token {
                        AdapterToken::Comma => {
                            if let Some(ContextState::Object(state)) = self.stack.last_mut() {
                                *state = ObjectState::KeyOrEnd;
                            }
                            continue;
                        }
                        AdapterToken::ObjectEnd => {
                            self.stack.pop();
                            self.finish_value_in_parent();
                            return Ok(Some(ParseEvent::StructEnd));
                        }
                        AdapterToken::Eof => {
                            return Err(JsonError::new(
                                JsonErrorKind::UnexpectedEof {
                                    expected: "',' or '}'",
                                },
                                token.span,
                            ));
                        }
                        _ => return Err(self.unexpected(&token, "',' or '}'")),
                    }
                }
                NextAction::ArrayValue => {
                    let token = self.consume_token()?;
                    match token.token {
                        AdapterToken::ArrayEnd => {
                            self.stack.pop();
                            self.finish_value_in_parent();
                            return Ok(Some(ParseEvent::SequenceEnd));
                        }
                        AdapterToken::Eof => {
                            return Err(JsonError::new(
                                JsonErrorKind::UnexpectedEof {
                                    expected: "value or ']'",
                                },
                                token.span,
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
                    match token.token {
                        AdapterToken::Comma => {
                            if let Some(ContextState::Array(state)) = self.stack.last_mut() {
                                *state = ArrayState::ValueOrEnd;
                            }
                            continue;
                        }
                        AdapterToken::ArrayEnd => {
                            self.stack.pop();
                            self.finish_value_in_parent();
                            return Ok(Some(ParseEvent::SequenceEnd));
                        }
                        AdapterToken::Eof => {
                            return Err(JsonError::new(
                                JsonErrorKind::UnexpectedEof {
                                    expected: "',' or ']'",
                                },
                                token.span,
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

    fn build_probe(&self) -> Result<Vec<FieldEvidence<'de>>, JsonError> {
        let remaining = self.input.get(self.current_offset..).unwrap_or_default();
        if remaining.is_empty() {
            return Ok(Vec::new());
        }

        let mut adapter = SliceAdapter::<true>::new(remaining);

        // If we've peeked a StructStart, we've already consumed the '{' so skip the check.
        // Otherwise, expect ObjectStart as the first token.
        let already_inside_object = matches!(self.event_peek, Some(ParseEvent::StructStart(_)));

        if !already_inside_object {
            let first = adapter.next_token().map_err(JsonError::from)?;
            if !matches!(first.token, AdapterToken::ObjectStart) {
                return Ok(Vec::new());
            }
        }

        let mut evidence = Vec::new();
        loop {
            let token = adapter.next_token().map_err(JsonError::from)?;
            match token.token {
                AdapterToken::ObjectEnd => break,
                AdapterToken::String(name) => {
                    let colon = adapter.next_token().map_err(JsonError::from)?;
                    if !matches!(colon.token, AdapterToken::Colon) {
                        return Err(JsonError::new(
                            JsonErrorKind::UnexpectedToken {
                                got: format!("{:?}", colon.token),
                                expected: "':'",
                            },
                            colon.span,
                        ));
                    }

                    // Capture scalar values, skip complex types (objects/arrays)
                    let value_token = adapter.next_token().map_err(JsonError::from)?;
                    let scalar_value = match value_token.token {
                        AdapterToken::String(s) => Some(ScalarValue::Str(s)),
                        AdapterToken::True => Some(ScalarValue::Bool(true)),
                        AdapterToken::False => Some(ScalarValue::Bool(false)),
                        AdapterToken::Null => Some(ScalarValue::Null),
                        AdapterToken::I64(n) => Some(ScalarValue::I64(n)),
                        AdapterToken::U64(n) => Some(ScalarValue::U64(n)),
                        AdapterToken::I128(n) => Some(ScalarValue::Str(Cow::Owned(n.to_string()))),
                        AdapterToken::U128(n) => Some(ScalarValue::Str(Cow::Owned(n.to_string()))),
                        AdapterToken::F64(n) => Some(ScalarValue::F64(n)),
                        AdapterToken::ObjectStart => {
                            // Skip the complex object
                            self.skip_container_in_adapter(&mut adapter, DelimKind::Object)?;
                            None
                        }
                        AdapterToken::ArrayStart => {
                            // Skip the complex array
                            self.skip_container_in_adapter(&mut adapter, DelimKind::Array)?;
                            None
                        }
                        _ => None,
                    };

                    if let Some(sv) = scalar_value {
                        evidence.push(FieldEvidence::with_scalar_value(
                            name,
                            FieldLocationHint::KeyValue,
                            None,
                            sv,
                            None, // No namespace for JSON
                        ));
                    } else {
                        evidence.push(FieldEvidence::new(
                            name,
                            FieldLocationHint::KeyValue,
                            None,
                            None, // No namespace for JSON
                        ));
                    }

                    let sep = adapter.next_token().map_err(JsonError::from)?;
                    match sep.token {
                        AdapterToken::Comma => continue,
                        AdapterToken::ObjectEnd => break,
                        AdapterToken::Eof => {
                            return Err(JsonError::new(
                                JsonErrorKind::UnexpectedEof {
                                    expected: "',' or '}'",
                                },
                                sep.span,
                            ));
                        }
                        _ => {
                            return Err(JsonError::new(
                                JsonErrorKind::UnexpectedToken {
                                    got: format!("{:?}", sep.token),
                                    expected: "',' or '}'",
                                },
                                sep.span,
                            ));
                        }
                    }
                }
                AdapterToken::Eof => {
                    return Err(JsonError::new(
                        JsonErrorKind::UnexpectedEof {
                            expected: "field name or '}'",
                        },
                        token.span,
                    ));
                }
                _ => {
                    return Err(JsonError::new(
                        JsonErrorKind::UnexpectedToken {
                            got: format!("{:?}", token.token),
                            expected: "field name or '}'",
                        },
                        token.span,
                    ));
                }
            }
        }

        Ok(evidence)
    }
}

impl<'de> FormatParser<'de> for JsonParser<'de> {
    type Error = JsonError;
    type Probe<'a>
        = JsonProbe<'de>
    where
        Self: 'a;

    fn raw_capture_shape(&self) -> Option<&'static facet_core::Shape> {
        Some(crate::RawJson::SHAPE)
    }

    fn next_event(&mut self) -> Result<Option<ParseEvent<'de>>, Self::Error> {
        if let Some(event) = self.event_peek.take() {
            self.peek_start_offset = None;
            return Ok(Some(event));
        }
        self.produce_event()
    }

    fn peek_event(&mut self) -> Result<Option<ParseEvent<'de>>, Self::Error> {
        if let Some(event) = self.event_peek.clone() {
            return Ok(Some(event));
        }
        let event = self.produce_event()?;
        if let Some(ref e) = event {
            self.event_peek = Some(e.clone());
            // Use the offset of the last token consumed (which is the value's first token)
            // For values, produce_event ultimately calls parse_value_start_with_token
            // which consumes the first token and sets last_token_start.
            self.peek_start_offset = Some(self.last_token_start);
        }
        Ok(event)
    }

    fn skip_value(&mut self) -> Result<(), Self::Error> {
        // Handle the case where peek_event was called before skip_value
        if let Some(event) = self.event_peek.take() {
            self.peek_start_offset = None;

            // Based on the peeked event, we may need to skip the rest of a container.
            // Note: When peeking a StructStart/SequenceStart, the parser already pushed
            // to self.stack. We need to pop it after skipping the container.
            match event {
                ParseEvent::StructStart(_) => {
                    let res = self.skip_container(DelimKind::Object);
                    // Pop the stack entry that was pushed during peek, even if skip_container errored
                    self.stack.pop();
                    res?;
                    // Update the parent's state after skipping the container
                    self.finish_value_in_parent();
                }
                ParseEvent::SequenceStart(_) => {
                    let res = self.skip_container(DelimKind::Array);
                    // Pop the stack entry that was pushed during peek, even if skip_container errored
                    self.stack.pop();
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

    fn begin_probe(&mut self) -> Result<Self::Probe<'_>, Self::Error> {
        let evidence = self.build_probe()?;
        Ok(JsonProbe { evidence, idx: 0 })
    }

    fn capture_raw(&mut self) -> Result<Option<&'de str>, Self::Error> {
        // Handle the case where peek_event was called before capture_raw.
        // This happens when deserialize_option peeks to check for null.
        let start_offset = if let Some(event) = self.event_peek.take() {
            let start = self
                .peek_start_offset
                .take()
                .expect("peek_start_offset should be set when event_peek is set");

            // Based on the peeked event, we may need to skip the rest of a container.
            // Note: When peeking a StructStart/SequenceStart, the parser already pushed
            // to self.stack. We need to pop it after skipping the container.
            match event {
                ParseEvent::StructStart(_) => {
                    let res = self.skip_container(DelimKind::Object);
                    // Pop the stack entry that was pushed during peek, even if skip_container errored
                    self.stack.pop();
                    res?;
                }
                ParseEvent::SequenceStart(_) => {
                    let res = self.skip_container(DelimKind::Array);
                    // Pop the stack entry that was pushed during peek, even if skip_container errored
                    self.stack.pop();
                    res?;
                }
                ParseEvent::StructEnd | ParseEvent::SequenceEnd => {
                    // This shouldn't happen in valid usage, but handle gracefully
                    return Err(JsonError::without_span(JsonErrorKind::InvalidValue {
                        message: "unexpected end event in capture_raw".to_string(),
                    }));
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
                    return Err(JsonError::new(
                        JsonErrorKind::UnexpectedEof { expected: "value" },
                        first.span,
                    ));
                }
                _ => {
                    // Simple value - already consumed
                }
            }

            start
        };

        // Get end position
        let end_offset = self.current_offset;

        // Extract the raw slice and convert to str
        let raw_bytes = &self.input[start_offset..end_offset];
        let raw_str = core::str::from_utf8(raw_bytes).map_err(|e| {
            JsonError::without_span(JsonErrorKind::InvalidValue {
                message: alloc::format!("invalid UTF-8 in raw JSON: {}", e),
            })
        })?;

        self.finish_value_in_parent();
        Ok(Some(raw_str))
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
        if self.event_peek.is_some() {
            return None;
        }
        if !self.stack.is_empty() {
            return None;
        }
        if self.root_started && !self.root_complete {
            // We've started parsing root but haven't finished - not safe
            return None;
        }
        Some(self.current_offset)
    }

    fn jit_set_pos(&mut self, pos: usize) {
        // Update the offset
        self.current_offset = pos;

        // Reset the adapter to start from the new position
        // We need to create a new adapter pointing to the remaining input
        // but preserving absolute offset semantics
        self.adapter = SliceAdapter::new_with_offset(self.input, pos);

        // Clear any peeked event and its offset
        self.event_peek = None;
        self.peek_start_offset = None;

        // Tier-2 JIT parsed a complete root value, so update parser state.
        // jit_pos() already enforces root-only usage, so we know:
        // - We started at root level with empty stack
        // - Tier-2 successfully parsed a complete value
        // - We're now at the position after that value
        self.root_started = true;
        self.root_complete = true;
        // Stack should already be empty (jit_pos enforces this)
        debug_assert!(self.stack.is_empty());
    }

    fn jit_format(&self) -> Self::FormatJit {
        crate::jit::JsonJitFormat
    }

    fn jit_error(&self, _input: &'de [u8], error_pos: usize, error_code: i32) -> Self::Error {
        use crate::error::JsonErrorKind;
        use facet_reflect::Span;

        let kind = match error_code {
            -100 => JsonErrorKind::UnexpectedEof { expected: "value" },
            -101 => JsonErrorKind::UnexpectedToken {
                got: "non-'['".into(),
                expected: "'['",
            },
            -102 => JsonErrorKind::UnexpectedToken {
                got: "non-boolean".into(),
                expected: "'true' or 'false'",
            },
            -103 => JsonErrorKind::UnexpectedToken {
                got: "unexpected token".into(),
                expected: "',' or ']'",
            },
            _ => JsonErrorKind::InvalidValue {
                message: alloc::format!("Tier-2 JIT error code: {}", error_code),
            },
        };

        JsonError::new(
            kind,
            Span {
                offset: error_pos,
                len: 1,
            },
        )
    }
}

pub struct JsonProbe<'de> {
    evidence: Vec<FieldEvidence<'de>>,
    idx: usize,
}

impl<'de> ProbeStream<'de> for JsonProbe<'de> {
    type Error = JsonError;

    fn next(&mut self) -> Result<Option<FieldEvidence<'de>>, Self::Error> {
        if self.idx >= self.evidence.len() {
            Ok(None)
        } else {
            let ev = self.evidence[self.idx].clone();
            self.idx += 1;
            Ok(Some(ev))
        }
    }
}
