//! Postcard parser implementing FormatParser and FormatJitParser.
//!
//! Postcard is NOT a self-describing format, but Tier-0 deserialization is supported
//! via the `hint_struct_fields` mechanism. The driver tells the parser how many fields
//! to expect, and the parser emits `OrderedField` events accordingly.

use alloc::borrow::Cow;
use alloc::vec::Vec;

use crate::error::{PostcardError, codes};
use facet_format::{
    ContainerKind, EnumVariantHint, FieldEvidence, FieldKey, FieldLocationHint, FormatParser,
    ParseEvent, ProbeStream, ScalarTypeHint, ScalarValue,
};

/// Stored variant metadata for enum parsing.
#[derive(Debug, Clone)]
struct VariantMeta {
    name: String,
    kind: facet_core::StructKind,
    field_count: usize,
}

/// Parser state for tracking nested structures.
#[derive(Debug, Clone)]
enum ParserState {
    /// At the top level or after completing a value.
    Ready,
    /// Inside a struct, tracking remaining fields.
    InStruct { remaining_fields: usize },
    /// Inside a sequence, tracking remaining elements.
    InSequence { remaining_elements: u64 },
    /// Inside an enum variant, tracking parsing progress.
    InEnum {
        variant_name: String,
        variant_kind: facet_core::StructKind,
        variant_field_count: usize,
        field_key_emitted: bool,
        /// For multi-field variants, whether we've emitted the inner wrapper start
        wrapper_start_emitted: bool,
        /// For multi-field variants, whether we've emitted the inner wrapper end
        wrapper_end_emitted: bool,
    },
}

/// Postcard parser for Tier-0 and Tier-2 deserialization.
///
/// For Tier-0, the parser relies on `hint_struct_fields` to know how many fields
/// to expect in structs. Sequences are length-prefixed in the wire format.
pub struct PostcardParser<'de> {
    input: &'de [u8],
    pos: usize,
    /// Stack of parser states for nested structures.
    state_stack: Vec<ParserState>,
    /// Peeked event (for `peek_event`).
    peeked: Option<ParseEvent<'de>>,
    /// Pending struct field count from `hint_struct_fields`.
    pending_struct_fields: Option<usize>,
    /// Pending scalar type hint from `hint_scalar_type`.
    pending_scalar_type: Option<ScalarTypeHint>,
    /// Pending sequence flag from `hint_sequence`.
    pending_sequence: bool,
    /// Pending option flag from `hint_option`.
    pending_option: bool,
    /// Pending enum variant metadata from `hint_enum`.
    pending_enum: Option<Vec<VariantMeta>>,
}

impl<'de> PostcardParser<'de> {
    /// Create a new postcard parser from input bytes.
    pub fn new(input: &'de [u8]) -> Self {
        Self {
            input,
            pos: 0,
            state_stack: Vec::new(),
            peeked: None,
            pending_struct_fields: None,
            pending_scalar_type: None,
            pending_sequence: false,
            pending_option: false,
            pending_enum: None,
        }
    }

    /// Read a single byte, advancing position.
    fn read_byte(&mut self) -> Result<u8, PostcardError> {
        if self.pos >= self.input.len() {
            return Err(PostcardError {
                code: codes::UNEXPECTED_EOF,
                pos: self.pos,
                message: "unexpected end of input".into(),
            });
        }
        let byte = self.input[self.pos];
        self.pos += 1;
        Ok(byte)
    }

    /// Read a varint (LEB128 encoded unsigned integer).
    fn read_varint(&mut self) -> Result<u64, PostcardError> {
        let mut result: u64 = 0;
        let mut shift: u32 = 0;

        loop {
            let byte = self.read_byte()?;
            let data = (byte & 0x7F) as u64;

            if shift >= 64 {
                return Err(PostcardError {
                    code: codes::VARINT_OVERFLOW,
                    pos: self.pos,
                    message: "varint overflow".into(),
                });
            }

            result |= data << shift;
            shift += 7;

            if (byte & 0x80) == 0 {
                return Ok(result);
            }
        }
    }

    /// Read a signed varint (ZigZag + LEB128).
    fn read_signed_varint(&mut self) -> Result<i64, PostcardError> {
        let unsigned = self.read_varint()?;
        // ZigZag decode: (n >> 1) ^ -(n & 1)
        let decoded = ((unsigned >> 1) as i64) ^ -((unsigned & 1) as i64);
        Ok(decoded)
    }

    /// Read N bytes as a slice.
    fn read_bytes(&mut self, len: usize) -> Result<&'de [u8], PostcardError> {
        if self.pos + len > self.input.len() {
            return Err(PostcardError {
                code: codes::UNEXPECTED_EOF,
                pos: self.pos,
                message: "unexpected end of input reading bytes".into(),
            });
        }
        let bytes = &self.input[self.pos..self.pos + len];
        self.pos += len;
        Ok(bytes)
    }

    /// Get the current parser state (top of stack or Ready).
    fn current_state(&self) -> &ParserState {
        self.state_stack.last().unwrap_or(&ParserState::Ready)
    }

    /// Generate the next event based on current state.
    fn generate_next_event(&mut self) -> Result<ParseEvent<'de>, PostcardError> {
        // Check if we have a pending option hint
        if self.pending_option {
            self.pending_option = false;
            let discriminant = self.read_byte()?;
            match discriminant {
                0x00 => return Ok(ParseEvent::Scalar(ScalarValue::Null)),
                0x01 => {
                    // Some(value) - consumed the discriminant. The deserializer will peek to check
                    // if it's None, see this is not Null, and then call deserialize_into for the value.
                    // Return a placeholder event (like OrderedField) to signal "not None".
                    // The deserializer will then call hint + expect for the inner value.
                    return Ok(ParseEvent::OrderedField);
                }
                _ => {
                    return Err(PostcardError {
                        code: codes::INVALID_OPTION_DISCRIMINANT,
                        pos: self.pos - 1,
                        message: format!("invalid Option discriminant: {}", discriminant),
                    });
                }
            }
        }

        // Check if we have a pending enum hint
        if let Some(variants) = self.pending_enum.take() {
            let variant_index = self.read_varint()? as usize;
            if variant_index >= variants.len() {
                return Err(PostcardError {
                    code: codes::INVALID_ENUM_DISCRIMINANT,
                    pos: self.pos,
                    message: format!(
                        "enum variant index {} out of range (max {})",
                        variant_index,
                        variants.len() - 1
                    ),
                });
            }
            let variant = &variants[variant_index];
            // Push InEnum state to emit StructStart, FieldKey, content, StructEnd sequence
            self.state_stack.push(ParserState::InEnum {
                variant_name: variant.name.clone(),
                variant_kind: variant.kind,
                variant_field_count: variant.field_count,
                field_key_emitted: false,
                wrapper_start_emitted: false,
                wrapper_end_emitted: false,
            });
            return Ok(ParseEvent::StructStart(ContainerKind::Object));
        }

        // Check if we have a pending scalar type hint
        if let Some(hint) = self.pending_scalar_type.take() {
            return self.parse_scalar_with_hint(hint);
        }

        // Check if we have a pending sequence hint
        if self.pending_sequence {
            self.pending_sequence = false;
            let count = self.read_varint()?;
            self.state_stack.push(ParserState::InSequence {
                remaining_elements: count,
            });
            return Ok(ParseEvent::SequenceStart(ContainerKind::Array));
        }

        // Check if we have a pending struct hint
        if let Some(num_fields) = self.pending_struct_fields.take() {
            self.state_stack.push(ParserState::InStruct {
                remaining_fields: num_fields,
            });
            return Ok(ParseEvent::StructStart(ContainerKind::Object));
        }

        // Check current state
        match self.current_state().clone() {
            ParserState::Ready => {
                // At top level without a hint - error
                Err(PostcardError {
                    code: codes::UNSUPPORTED,
                    pos: self.pos,
                    message: "postcard parser needs type hints (use hint_scalar_type, hint_struct_fields, or hint_sequence)".into(),
                })
            }
            ParserState::InStruct { remaining_fields } => {
                if remaining_fields == 0 {
                    // Struct complete
                    self.state_stack.pop();
                    Ok(ParseEvent::StructEnd)
                } else {
                    // More fields to go - emit OrderedField and decrement
                    if let Some(ParserState::InStruct { remaining_fields }) =
                        self.state_stack.last_mut()
                    {
                        *remaining_fields -= 1;
                    }
                    Ok(ParseEvent::OrderedField)
                }
            }
            ParserState::InSequence { remaining_elements } => {
                if remaining_elements == 0 {
                    // Sequence complete
                    self.state_stack.pop();
                    Ok(ParseEvent::SequenceEnd)
                } else {
                    // More elements remaining. Return OrderedField as a placeholder to indicate
                    // "not end yet". The deserializer will then call hint + expect for the next element.
                    // Decrement the counter after returning the placeholder.
                    if let Some(ParserState::InSequence { remaining_elements }) =
                        self.state_stack.last_mut()
                    {
                        *remaining_elements -= 1;
                    }
                    Ok(ParseEvent::OrderedField)
                }
            }
            ParserState::InEnum {
                variant_name,
                variant_kind,
                variant_field_count,
                field_key_emitted,
                wrapper_start_emitted,
                wrapper_end_emitted,
            } => {
                use facet_core::StructKind;

                if !field_key_emitted {
                    // Step 1: Emit the FieldKey with the variant name
                    if let Some(ParserState::InEnum {
                        field_key_emitted, ..
                    }) = self.state_stack.last_mut()
                    {
                        *field_key_emitted = true;
                    }
                    Ok(ParseEvent::FieldKey(FieldKey {
                        name: Cow::Owned(variant_name.clone()),
                        namespace: None,
                        location: FieldLocationHint::KeyValue,
                    }))
                } else if !wrapper_start_emitted {
                    // Step 2: After FieldKey, emit wrapper start (if needed)
                    match variant_kind {
                        StructKind::Unit => {
                            // Unit variant - no wrapper, skip directly to StructEnd
                            self.state_stack.pop();
                            Ok(ParseEvent::StructEnd)
                        }
                        StructKind::Tuple | StructKind::TupleStruct => {
                            // Check if it's a newtype (single-field) or multi-field tuple
                            if variant_field_count == 1 {
                                // Newtype variant - no wrapper, content consumed directly
                                // Mark wrapper as emitted so we skip directly to final StructEnd
                                if let Some(ParserState::InEnum {
                                    wrapper_start_emitted,
                                    wrapper_end_emitted,
                                    ..
                                }) = self.state_stack.last_mut()
                                {
                                    *wrapper_start_emitted = true;
                                    *wrapper_end_emitted = true; // Skip wrapper end emission
                                }
                                // Recursively call to get the next event (likely a scalar hint response)
                                self.generate_next_event()
                            } else {
                                // Multi-field tuple variant - emit SequenceStart and push InSequence state
                                // But unlike regular sequences, tuple enum variants don't use OrderedField placeholders
                                // The deserializer calls deserialize_into directly for each field
                                // So we DON'T push InSequence - we track manually via wrapper_end_emitted
                                if let Some(ParserState::InEnum {
                                    wrapper_start_emitted,
                                    ..
                                }) = self.state_stack.last_mut()
                                {
                                    *wrapper_start_emitted = true;
                                }
                                Ok(ParseEvent::SequenceStart(ContainerKind::Array))
                            }
                        }
                        StructKind::Struct => {
                            // Struct variant - mark wrapper start emitted and push InStruct state
                            // The InStruct state will emit OrderedField events for each field
                            // (postcard encodes struct fields in order without names)
                            if let Some(ParserState::InEnum {
                                wrapper_start_emitted,
                                ..
                            }) = self.state_stack.last_mut()
                            {
                                *wrapper_start_emitted = true;
                            }
                            // Get the field count from the variant
                            let field_count = if let ParserState::InEnum {
                                variant_field_count,
                                ..
                            } = self.current_state()
                            {
                                *variant_field_count
                            } else {
                                0
                            };
                            self.state_stack.push(ParserState::InStruct {
                                remaining_fields: field_count,
                            });
                            Ok(ParseEvent::StructStart(ContainerKind::Object))
                        }
                    }
                } else if !wrapper_end_emitted {
                    // Step 3: Emit wrapper end for multi-field variants
                    match variant_kind {
                        StructKind::Unit => {
                            // Already handled above
                            unreachable!()
                        }
                        StructKind::Tuple | StructKind::TupleStruct => {
                            // For multi-field tuples, emit SequenceEnd
                            if variant_field_count > 1 {
                                if let Some(ParserState::InEnum {
                                    wrapper_end_emitted,
                                    ..
                                }) = self.state_stack.last_mut()
                                {
                                    *wrapper_end_emitted = true;
                                }
                                Ok(ParseEvent::SequenceEnd)
                            } else {
                                // Newtype - already marked wrapper_end_emitted=true, skip to final StructEnd
                                self.state_stack.pop();
                                Ok(ParseEvent::StructEnd)
                            }
                        }
                        StructKind::Struct => {
                            // Struct variants use InStruct which already popped, so we're ready for final StructEnd
                            self.state_stack.pop();
                            Ok(ParseEvent::StructEnd)
                        }
                    }
                } else {
                    // Step 4: Emit final outer StructEnd
                    // This is reached after wrapper end has been emitted
                    self.state_stack.pop();
                    Ok(ParseEvent::StructEnd)
                }
            }
        }
    }

    /// Parse a scalar value with the given type hint.
    fn parse_scalar_with_hint(
        &mut self,
        hint: ScalarTypeHint,
    ) -> Result<ParseEvent<'de>, PostcardError> {
        let scalar = match hint {
            ScalarTypeHint::Bool => {
                let val = self.parse_bool()?;
                ScalarValue::Bool(val)
            }
            ScalarTypeHint::U8 => {
                let val = self.parse_u8()?;
                ScalarValue::U64(val as u64)
            }
            ScalarTypeHint::U16 => {
                let val = self.parse_u16()?;
                ScalarValue::U64(val as u64)
            }
            ScalarTypeHint::U32 => {
                let val = self.parse_u32()?;
                ScalarValue::U64(val as u64)
            }
            ScalarTypeHint::U64 => {
                let val = self.parse_u64()?;
                ScalarValue::U64(val)
            }
            ScalarTypeHint::I8 => {
                let val = self.parse_i8()?;
                ScalarValue::I64(val as i64)
            }
            ScalarTypeHint::I16 => {
                let val = self.parse_i16()?;
                ScalarValue::I64(val as i64)
            }
            ScalarTypeHint::I32 => {
                let val = self.parse_i32()?;
                ScalarValue::I64(val as i64)
            }
            ScalarTypeHint::I64 => {
                let val = self.parse_i64()?;
                ScalarValue::I64(val)
            }
            ScalarTypeHint::F32 => {
                let val = self.parse_f32()?;
                ScalarValue::F64(val as f64)
            }
            ScalarTypeHint::F64 => {
                let val = self.parse_f64()?;
                ScalarValue::F64(val)
            }
            ScalarTypeHint::String => {
                let val = self.parse_string()?;
                ScalarValue::Str(Cow::Borrowed(val))
            }
            ScalarTypeHint::Bytes => {
                let val = self.parse_bytes()?;
                ScalarValue::Bytes(Cow::Borrowed(val))
            }
            ScalarTypeHint::Char => {
                // Parse as UTF-8 character - read varint for codepoint
                let codepoint = self.read_varint()? as u32;
                let c = char::from_u32(codepoint).ok_or_else(|| PostcardError {
                    code: codes::INVALID_UTF8,
                    pos: self.pos,
                    message: "invalid unicode codepoint".into(),
                })?;
                // Represent as string since ScalarValue doesn't have Char
                ScalarValue::Str(Cow::Owned(c.to_string()))
            }
        };
        Ok(ParseEvent::Scalar(scalar))
    }

    /// Parse a boolean value.
    pub fn parse_bool(&mut self) -> Result<bool, PostcardError> {
        let byte = self.read_byte()?;
        match byte {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(PostcardError {
                code: codes::INVALID_BOOL,
                pos: self.pos - 1,
                message: "invalid boolean value".into(),
            }),
        }
    }

    /// Parse an unsigned 8-bit integer.
    pub fn parse_u8(&mut self) -> Result<u8, PostcardError> {
        self.read_byte()
    }

    /// Parse an unsigned 16-bit integer (varint).
    pub fn parse_u16(&mut self) -> Result<u16, PostcardError> {
        let val = self.read_varint()?;
        Ok(val as u16)
    }

    /// Parse an unsigned 32-bit integer (varint).
    pub fn parse_u32(&mut self) -> Result<u32, PostcardError> {
        let val = self.read_varint()?;
        Ok(val as u32)
    }

    /// Parse an unsigned 64-bit integer (varint).
    pub fn parse_u64(&mut self) -> Result<u64, PostcardError> {
        self.read_varint()
    }

    /// Parse a signed 8-bit integer (single byte, two's complement).
    pub fn parse_i8(&mut self) -> Result<i8, PostcardError> {
        // i8 is encoded as a single byte in two's complement form (not varint)
        let byte = self.read_byte()?;
        Ok(byte as i8)
    }

    /// Parse a signed 16-bit integer (zigzag varint).
    pub fn parse_i16(&mut self) -> Result<i16, PostcardError> {
        let val = self.read_signed_varint()?;
        Ok(val as i16)
    }

    /// Parse a signed 32-bit integer (zigzag varint).
    pub fn parse_i32(&mut self) -> Result<i32, PostcardError> {
        let val = self.read_signed_varint()?;
        Ok(val as i32)
    }

    /// Parse a signed 64-bit integer (zigzag varint).
    pub fn parse_i64(&mut self) -> Result<i64, PostcardError> {
        self.read_signed_varint()
    }

    /// Parse a 32-bit float (little-endian).
    pub fn parse_f32(&mut self) -> Result<f32, PostcardError> {
        let bytes = self.read_bytes(4)?;
        Ok(f32::from_le_bytes(bytes.try_into().unwrap()))
    }

    /// Parse a 64-bit float (little-endian).
    pub fn parse_f64(&mut self) -> Result<f64, PostcardError> {
        let bytes = self.read_bytes(8)?;
        Ok(f64::from_le_bytes(bytes.try_into().unwrap()))
    }

    /// Parse a string (varint length + UTF-8 bytes).
    pub fn parse_string(&mut self) -> Result<&'de str, PostcardError> {
        let len = self.read_varint()? as usize;
        let bytes = self.read_bytes(len)?;
        core::str::from_utf8(bytes).map_err(|_| PostcardError {
            code: codes::INVALID_UTF8,
            pos: self.pos - len,
            message: "invalid UTF-8 in string".into(),
        })
    }

    /// Parse bytes (varint length + raw bytes).
    pub fn parse_bytes(&mut self) -> Result<&'de [u8], PostcardError> {
        let len = self.read_varint()? as usize;
        self.read_bytes(len)
    }

    /// Begin parsing a sequence, returning the element count.
    pub fn begin_sequence(&mut self) -> Result<u64, PostcardError> {
        let count = self.read_varint()?;
        self.state_stack.push(ParserState::InSequence {
            remaining_elements: count,
        });
        Ok(count)
    }
}

/// Stub probe stream for PostcardParser.
///
/// Not used since postcard doesn't support probing (non-self-describing).
pub struct PostcardProbe;

impl<'de> ProbeStream<'de> for PostcardProbe {
    type Error = PostcardError;

    fn next(&mut self) -> Result<Option<FieldEvidence<'de>>, Self::Error> {
        // Postcard doesn't support probing
        Ok(None)
    }
}

impl<'de> FormatParser<'de> for PostcardParser<'de> {
    type Error = PostcardError;
    type Probe<'a>
        = PostcardProbe
    where
        Self: 'a;

    fn next_event(&mut self) -> Result<Option<ParseEvent<'de>>, Self::Error> {
        // Return peeked event if available
        if let Some(event) = self.peeked.take() {
            return Ok(Some(event));
        }
        Ok(Some(self.generate_next_event()?))
    }

    fn peek_event(&mut self) -> Result<Option<ParseEvent<'de>>, Self::Error> {
        if self.peeked.is_none() {
            self.peeked = Some(self.generate_next_event()?);
        }
        Ok(self.peeked.clone())
    }

    fn skip_value(&mut self) -> Result<(), Self::Error> {
        // For non-self-describing formats, skipping is complex because
        // we don't know the type/size of the value.
        Err(PostcardError {
            code: codes::UNSUPPORTED,
            pos: self.pos,
            message: "skip_value not supported for postcard (non-self-describing)".into(),
        })
    }

    fn begin_probe(&mut self) -> Result<Self::Probe<'_>, Self::Error> {
        // Postcard doesn't support probing
        Ok(PostcardProbe)
    }

    fn is_self_describing(&self) -> bool {
        false
    }

    fn hint_struct_fields(&mut self, num_fields: usize) {
        self.pending_struct_fields = Some(num_fields);
        // Clear any peeked OrderedField placeholder for sequences
        if matches!(self.peeked, Some(ParseEvent::OrderedField)) {
            self.peeked = None;
        }
    }

    fn hint_scalar_type(&mut self, hint: ScalarTypeHint) {
        self.pending_scalar_type = Some(hint);
        // Clear any peeked OrderedField placeholder for sequences
        if matches!(self.peeked, Some(ParseEvent::OrderedField)) {
            self.peeked = None;
        }
    }

    fn hint_sequence(&mut self) {
        self.pending_sequence = true;
        // Clear any peeked OrderedField placeholder
        if matches!(self.peeked, Some(ParseEvent::OrderedField)) {
            self.peeked = None;
        }
    }

    fn hint_option(&mut self) {
        self.pending_option = true;
        // Clear any peeked OrderedField placeholder
        if matches!(self.peeked, Some(ParseEvent::OrderedField)) {
            self.peeked = None;
        }
    }

    fn hint_enum(&mut self, variants: &[EnumVariantHint]) {
        // Store variant metadata, converting to owned strings to avoid lifetime issues.
        let metas: Vec<VariantMeta> = variants
            .iter()
            .map(|v| VariantMeta {
                name: v.name.to_string(),
                kind: v.kind,
                field_count: v.field_count,
            })
            .collect();
        self.pending_enum = Some(metas);
        // Clear any peeked OrderedField placeholder for sequences
        if matches!(self.peeked, Some(ParseEvent::OrderedField)) {
            self.peeked = None;
        }
    }
}

#[cfg(feature = "jit")]
impl<'de> facet_format::FormatJitParser<'de> for PostcardParser<'de> {
    type FormatJit = crate::jit::PostcardJitFormat;

    fn jit_input(&self) -> &'de [u8] {
        self.input
    }

    fn jit_pos(&self) -> Option<usize> {
        // Only return position if no peeked event (clean state)
        if self.peeked.is_some() {
            None
        } else {
            Some(self.pos)
        }
    }

    fn jit_set_pos(&mut self, pos: usize) {
        self.pos = pos;
        self.peeked = None;
        // Clear state when JIT takes over
        self.state_stack.clear();
        self.pending_struct_fields = None;
        self.pending_scalar_type = None;
        self.pending_sequence = false;
    }

    fn jit_format(&self) -> Self::FormatJit {
        crate::jit::PostcardJitFormat
    }

    fn jit_error(&self, _input: &'de [u8], error_pos: usize, error_code: i32) -> Self::Error {
        PostcardError::from_code(error_code, error_pos)
    }
}
