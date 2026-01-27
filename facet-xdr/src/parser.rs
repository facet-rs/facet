//! XDR parser implementing FormatParser.
//!
//! XDR (External Data Representation) is defined in RFC 4506.
//! Key characteristics:
//! - Big-endian byte order
//! - All values are padded to 4-byte boundaries
//! - Fixed-size integers (4 bytes for i32/u32, 8 bytes for i64/u64)
//! - No support for i128/u128
//! - Strings and variable-length data are length-prefixed with 4-byte padding
//!
//! XDR is NOT a self-describing format - fields are positional.
//! This parser uses the `hint_*` methods from `FormatParser` to know what types to expect.

extern crate alloc;

use alloc::{borrow::Cow, string::String, vec::Vec};

use crate::error::codes;
use facet_format::{
    ContainerKind, DeserializeErrorKind, EnumVariantHint, FormatParser, ParseError, ParseEvent,
    ParseEventKind, SavePoint, ScalarTypeHint, ScalarValue,
};
use facet_reflect::Span;

/// Create a ParseError from an error code and position.
fn error_from_code(code: i32, pos: usize) -> ParseError {
    let message = match code {
        codes::UNEXPECTED_EOF => "unexpected end of input",
        codes::INVALID_BOOL => "invalid boolean value (must be 0 or 1)",
        codes::INVALID_OPTIONAL => "invalid optional discriminant (must be 0 or 1)",
        codes::INVALID_VARIANT => "invalid enum discriminant",
        codes::INVALID_UTF8 => "invalid UTF-8 in string",
        codes::UNSUPPORTED_TYPE => "unsupported type for XDR",
        codes::ALIGNMENT_ERROR => "position not aligned to 4 bytes",
        _ => "unknown error",
    };
    ParseError::new(
        Span::new(pos, 1),
        DeserializeErrorKind::InvalidValue {
            message: message.into(),
        },
    )
}

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
    /// Inside a sequence (variable-length array), tracking remaining elements.
    InSequence { remaining_elements: u32 },
    /// Inside a fixed-size array, tracking remaining elements.
    InArray { remaining_elements: usize },
    /// Inside an enum variant, tracking parsing progress.
    InEnum {
        variant_name: String,
        variant_kind: facet_core::StructKind,
        variant_field_count: usize,
        field_key_emitted: bool,
        wrapper_start_emitted: bool,
        wrapper_end_emitted: bool,
    },
}

/// XDR parser for deserialization.
///
/// XDR is a positional binary format - fields don't have names in the wire format.
/// This parser relies on `hint_*` methods to know what types to expect.
pub struct XdrParser<'de> {
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
    /// Pending fixed-size array length from `hint_array`.
    pending_array: Option<usize>,
    /// Pending option flag from `hint_option`.
    pending_option: bool,
    /// Pending enum variant metadata from `hint_enum`.
    pending_enum: Option<Vec<VariantMeta>>,
}

impl<'de> XdrParser<'de> {
    /// Create a new XDR parser from input bytes.
    pub const fn new(input: &'de [u8]) -> Self {
        Self {
            input,
            pos: 0,
            state_stack: Vec::new(),
            peeked: None,
            pending_struct_fields: None,
            pending_scalar_type: None,
            pending_sequence: false,
            pending_array: None,
            pending_option: false,
            pending_enum: None,
        }
    }

    /// Read a u32 in big-endian (XDR standard).
    fn read_u32(&mut self) -> Result<u32, ParseError> {
        if self.pos + 4 > self.input.len() {
            return Err(error_from_code(codes::UNEXPECTED_EOF, self.pos));
        }
        let bytes = &self.input[self.pos..self.pos + 4];
        self.pos += 4;
        Ok(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    /// Read a u64 in big-endian.
    fn read_u64(&mut self) -> Result<u64, ParseError> {
        if self.pos + 8 > self.input.len() {
            return Err(error_from_code(codes::UNEXPECTED_EOF, self.pos));
        }
        let bytes = &self.input[self.pos..self.pos + 8];
        self.pos += 8;
        Ok(u64::from_be_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]))
    }

    /// Read an i32 in big-endian.
    fn read_i32(&mut self) -> Result<i32, ParseError> {
        Ok(self.read_u32()? as i32)
    }

    /// Read an i64 in big-endian.
    fn read_i64(&mut self) -> Result<i64, ParseError> {
        Ok(self.read_u64()? as i64)
    }

    /// Read an f32 in big-endian.
    fn read_f32(&mut self) -> Result<f32, ParseError> {
        let bits = self.read_u32()?;
        Ok(f32::from_bits(bits))
    }

    /// Read an f64 in big-endian.
    fn read_f64(&mut self) -> Result<f64, ParseError> {
        let bits = self.read_u64()?;
        Ok(f64::from_bits(bits))
    }

    /// Read variable-length opaque data (with length prefix and padding).
    fn read_opaque_var(&mut self) -> Result<&'de [u8], ParseError> {
        let len = self.read_u32()? as usize;
        if self.pos + len > self.input.len() {
            return Err(error_from_code(codes::UNEXPECTED_EOF, self.pos));
        }
        let data = &self.input[self.pos..self.pos + len];
        self.pos += len;
        // Skip padding to align to 4 bytes
        let pad = (4 - (len % 4)) % 4;
        if self.pos + pad > self.input.len() {
            return Err(error_from_code(codes::UNEXPECTED_EOF, self.pos));
        }
        self.pos += pad;
        Ok(data)
    }

    /// Read a string (variable-length opaque interpreted as UTF-8).
    fn read_string(&mut self) -> Result<Cow<'de, str>, ParseError> {
        let start_pos = self.pos;
        let bytes = self.read_opaque_var()?;
        core::str::from_utf8(bytes).map(Cow::Borrowed).map_err(|_| {
            let mut context = [0u8; 16];
            let context_len = bytes.len().min(16);
            context[..context_len].copy_from_slice(&bytes[..context_len]);
            ParseError::new(
                Span::new(start_pos, bytes.len()),
                DeserializeErrorKind::InvalidUtf8 {
                    context,
                    context_len: context_len as u8,
                },
            )
        })
    }

    /// Read a boolean (XDR bool is 4 bytes: 0=false, 1=true).
    fn read_bool(&mut self) -> Result<bool, ParseError> {
        let val = self.read_u32()?;
        match val {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(error_from_code(codes::INVALID_BOOL, self.pos - 4)),
        }
    }

    /// Get the current parser state (top of stack or Ready).
    fn current_state(&self) -> &ParserState {
        self.state_stack.last().unwrap_or(&ParserState::Ready)
    }

    /// Generate the next event based on current state and hints.
    fn generate_next_event(&mut self) -> Result<ParseEvent<'de>, ParseError> {
        // Check if we have a pending option hint
        if self.pending_option {
            self.pending_option = false;
            let discriminant = self.read_u32()?;
            match discriminant {
                0 => {
                    return Ok(self.event(ParseEventKind::Scalar(ScalarValue::Null)));
                }
                1 => {
                    // Some(value) - return placeholder, deserializer will call hint for inner
                    return Ok(self.event(ParseEventKind::OrderedField));
                }
                _ => {
                    return Err(error_from_code(codes::INVALID_OPTIONAL, self.pos - 4));
                }
            }
        }

        // Check if we have a pending enum hint
        if let Some(variants) = self.pending_enum.take() {
            let discriminant = self.read_u32()? as usize;

            if discriminant >= variants.len() {
                return Err(error_from_code(codes::INVALID_VARIANT, self.pos - 4));
            }
            let variant = &variants[discriminant];

            self.state_stack.push(ParserState::InEnum {
                variant_name: variant.name.clone(),
                variant_kind: variant.kind,
                variant_field_count: variant.field_count,
                field_key_emitted: false,
                wrapper_start_emitted: false,
                wrapper_end_emitted: false,
            });
            return Ok(self.event(ParseEventKind::StructStart(ContainerKind::Object)));
        }

        // Check if we have a pending scalar type hint
        if let Some(hint) = self.pending_scalar_type.take() {
            return self.parse_scalar_with_hint(hint);
        }

        // Check if we have a pending sequence hint (variable-length)
        if self.pending_sequence {
            self.pending_sequence = false;
            let count = self.read_u32()?;
            self.state_stack.push(ParserState::InSequence {
                remaining_elements: count,
            });
            return Ok(self.event(ParseEventKind::SequenceStart(ContainerKind::Array)));
        }

        // Check if we have a pending fixed-size array hint
        if let Some(len) = self.pending_array.take() {
            self.state_stack.push(ParserState::InArray {
                remaining_elements: len,
            });
            return Ok(self.event(ParseEventKind::SequenceStart(ContainerKind::Array)));
        }

        // Check if we have a pending struct hint
        if let Some(num_fields) = self.pending_struct_fields.take() {
            self.state_stack.push(ParserState::InStruct {
                remaining_fields: num_fields,
            });
            return Ok(self.event(ParseEventKind::StructStart(ContainerKind::Object)));
        }

        // Check current state
        match self.current_state().clone() {
            ParserState::Ready => {
                // At top level without a hint - error
                Err(ParseError::new(
                    Span::new(self.pos, 1),
                    DeserializeErrorKind::InvalidValue {
                        message: "XDR parser needs type hints (use hint_scalar_type, hint_struct_fields, or hint_sequence)".into(),
                    },
                ))
            }
            ParserState::InStruct { remaining_fields } => {
                if remaining_fields == 0 {
                    self.state_stack.pop();
                    Ok(self.event(ParseEventKind::StructEnd))
                } else {
                    if let Some(ParserState::InStruct { remaining_fields }) =
                        self.state_stack.last_mut()
                    {
                        *remaining_fields -= 1;
                    }
                    Ok(self.event(ParseEventKind::OrderedField))
                }
            }
            ParserState::InSequence { remaining_elements } => {
                if remaining_elements == 0 {
                    self.state_stack.pop();
                    Ok(self.event(ParseEventKind::SequenceEnd))
                } else {
                    if let Some(ParserState::InSequence { remaining_elements }) =
                        self.state_stack.last_mut()
                    {
                        *remaining_elements -= 1;
                    }
                    Ok(self.event(ParseEventKind::OrderedField))
                }
            }
            ParserState::InArray { remaining_elements } => {
                if remaining_elements == 0 {
                    self.state_stack.pop();
                    Ok(self.event(ParseEventKind::SequenceEnd))
                } else {
                    if let Some(ParserState::InArray { remaining_elements }) =
                        self.state_stack.last_mut()
                    {
                        *remaining_elements -= 1;
                    }
                    Ok(self.event(ParseEventKind::OrderedField))
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
                    if let Some(ParserState::InEnum {
                        field_key_emitted, ..
                    }) = self.state_stack.last_mut()
                    {
                        *field_key_emitted = true;
                    }
                    Ok(
                        self.event(ParseEventKind::FieldKey(facet_format::FieldKey::new(
                            Cow::Owned(variant_name),
                            facet_format::FieldLocationHint::KeyValue,
                        ))),
                    )
                } else if !wrapper_start_emitted {
                    match variant_kind {
                        StructKind::Unit => {
                            self.state_stack.pop();
                            Ok(self.event(ParseEventKind::StructEnd))
                        }
                        StructKind::Tuple | StructKind::TupleStruct => {
                            if variant_field_count == 1 {
                                // Newtype variant
                                if let Some(ParserState::InEnum {
                                    wrapper_start_emitted,
                                    wrapper_end_emitted,
                                    ..
                                }) = self.state_stack.last_mut()
                                {
                                    *wrapper_start_emitted = true;
                                    *wrapper_end_emitted = true;
                                }
                                self.generate_next_event()
                            } else {
                                if let Some(ParserState::InEnum {
                                    wrapper_start_emitted,
                                    ..
                                }) = self.state_stack.last_mut()
                                {
                                    *wrapper_start_emitted = true;
                                }
                                Ok(self.event(ParseEventKind::SequenceStart(ContainerKind::Array)))
                            }
                        }
                        StructKind::Struct => {
                            if let Some(ParserState::InEnum {
                                wrapper_start_emitted,
                                ..
                            }) = self.state_stack.last_mut()
                            {
                                *wrapper_start_emitted = true;
                            }
                            self.state_stack.push(ParserState::InStruct {
                                remaining_fields: variant_field_count,
                            });
                            Ok(self.event(ParseEventKind::StructStart(ContainerKind::Object)))
                        }
                    }
                } else if !wrapper_end_emitted {
                    match variant_kind {
                        StructKind::Unit => unreachable!(),
                        StructKind::Tuple | StructKind::TupleStruct => {
                            if variant_field_count > 1 {
                                if let Some(ParserState::InEnum {
                                    wrapper_end_emitted,
                                    ..
                                }) = self.state_stack.last_mut()
                                {
                                    *wrapper_end_emitted = true;
                                }
                                Ok(self.event(ParseEventKind::SequenceEnd))
                            } else {
                                self.state_stack.pop();
                                Ok(self.event(ParseEventKind::StructEnd))
                            }
                        }
                        StructKind::Struct => {
                            self.state_stack.pop();
                            Ok(self.event(ParseEventKind::StructEnd))
                        }
                    }
                } else {
                    self.state_stack.pop();
                    Ok(self.event(ParseEventKind::StructEnd))
                }
            }
        }
    }

    /// Parse a scalar value with the given type hint.
    fn parse_scalar_with_hint(
        &mut self,
        hint: ScalarTypeHint,
    ) -> Result<ParseEvent<'de>, ParseError> {
        let scalar = match hint {
            ScalarTypeHint::Bool => {
                let val = self.read_bool()?;
                ScalarValue::Bool(val)
            }
            // XDR encodes smaller integers as 4 bytes
            ScalarTypeHint::U8 => {
                let val = self.read_u32()? as u8;
                ScalarValue::U64(val as u64)
            }
            ScalarTypeHint::U16 => {
                let val = self.read_u32()? as u16;
                ScalarValue::U64(val as u64)
            }
            ScalarTypeHint::U32 => {
                let val = self.read_u32()?;
                ScalarValue::U64(val as u64)
            }
            ScalarTypeHint::U64 => {
                let val = self.read_u64()?;
                ScalarValue::U64(val)
            }
            ScalarTypeHint::U128 => {
                // XDR doesn't support u128
                return Err(error_from_code(codes::UNSUPPORTED_TYPE, self.pos));
            }
            ScalarTypeHint::Usize => {
                // Encode usize as u64
                let val = self.read_u64()?;
                ScalarValue::U64(val)
            }
            ScalarTypeHint::I8 => {
                let val = self.read_i32()? as i8;
                ScalarValue::I64(val as i64)
            }
            ScalarTypeHint::I16 => {
                let val = self.read_i32()? as i16;
                ScalarValue::I64(val as i64)
            }
            ScalarTypeHint::I32 => {
                let val = self.read_i32()?;
                ScalarValue::I64(val as i64)
            }
            ScalarTypeHint::I64 => {
                let val = self.read_i64()?;
                ScalarValue::I64(val)
            }
            ScalarTypeHint::I128 => {
                // XDR doesn't support i128
                return Err(error_from_code(codes::UNSUPPORTED_TYPE, self.pos));
            }
            ScalarTypeHint::Isize => {
                // Encode isize as i64
                let val = self.read_i64()?;
                ScalarValue::I64(val)
            }
            ScalarTypeHint::F32 => {
                let val = self.read_f32()?;
                ScalarValue::F64(val as f64)
            }
            ScalarTypeHint::F64 => {
                let val = self.read_f64()?;
                ScalarValue::F64(val)
            }
            ScalarTypeHint::String => {
                let val = self.read_string()?;
                ScalarValue::Str(val)
            }
            ScalarTypeHint::Bytes => {
                let val = self.read_opaque_var()?;
                ScalarValue::Bytes(Cow::Borrowed(val))
            }
            ScalarTypeHint::Char => {
                // XDR encodes char as u32
                let val = self.read_u32()?;
                let c = char::from_u32(val).ok_or_else(|| {
                    ParseError::new(
                        Span::new(self.pos - 4, 4),
                        DeserializeErrorKind::InvalidValue {
                            message: "invalid char codepoint".into(),
                        },
                    )
                })?;
                ScalarValue::Str(Cow::Owned(c.to_string()))
            }
        };
        Ok(self.event(ParseEventKind::Scalar(scalar)))
    }
}

impl<'de> FormatParser<'de> for XdrParser<'de> {
    fn next_event(&mut self) -> Result<Option<ParseEvent<'de>>, ParseError> {
        if let Some(event) = self.peeked.take() {
            return Ok(Some(event));
        }
        Ok(Some(self.generate_next_event()?))
    }

    fn peek_event(&mut self) -> Result<Option<ParseEvent<'de>>, ParseError> {
        if self.peeked.is_none() {
            self.peeked = Some(self.generate_next_event()?);
        }
        Ok(self.peeked.clone())
    }

    fn skip_value(&mut self) -> Result<(), ParseError> {
        // XDR is not self-describing, so we can't skip arbitrary values
        Err(ParseError::new(
            Span::new(self.pos, 1),
            DeserializeErrorKind::InvalidValue {
                message: "skip_value not supported for XDR (non-self-describing format)".into(),
            },
        ))
    }

    fn current_span(&self) -> Option<Span> {
        Some(Span::new(self.pos, 1))
    }

    fn save(&mut self) -> SavePoint {
        // XDR is positional - save/restore not meaningful
        unimplemented!("save/restore not supported for XDR (positional format)")
    }

    fn restore(&mut self, _save_point: SavePoint) {
        unimplemented!("save/restore not supported for XDR (positional format)")
    }

    fn is_self_describing(&self) -> bool {
        false
    }

    fn hint_struct_fields(&mut self, num_fields: usize) {
        self.pending_struct_fields = Some(num_fields);
        if self
            .peeked
            .as_ref()
            .is_some_and(|e| matches!(e.kind, ParseEventKind::OrderedField))
        {
            self.peeked = None;
        }
    }

    fn hint_scalar_type(&mut self, hint: ScalarTypeHint) {
        self.pending_scalar_type = Some(hint);
        if self
            .peeked
            .as_ref()
            .is_some_and(|e| matches!(e.kind, ParseEventKind::OrderedField))
        {
            self.peeked = None;
        }
    }

    fn hint_sequence(&mut self) {
        self.pending_sequence = true;
        if self
            .peeked
            .as_ref()
            .is_some_and(|e| matches!(e.kind, ParseEventKind::OrderedField))
        {
            self.peeked = None;
        }
    }

    fn hint_array(&mut self, len: usize) {
        self.pending_array = Some(len);
        if self
            .peeked
            .as_ref()
            .is_some_and(|e| matches!(e.kind, ParseEventKind::OrderedField))
        {
            self.peeked = None;
        }
    }

    fn hint_option(&mut self) {
        self.pending_option = true;
        if self
            .peeked
            .as_ref()
            .is_some_and(|e| matches!(e.kind, ParseEventKind::OrderedField))
        {
            self.peeked = None;
        }
    }

    fn hint_enum(&mut self, variants: &[EnumVariantHint]) {
        let metas: Vec<VariantMeta> = variants
            .iter()
            .map(|v| VariantMeta {
                name: v.name.to_string(),
                kind: v.kind,
                field_count: v.field_count,
            })
            .collect();
        self.pending_enum = Some(metas);
        if self
            .peeked
            .as_ref()
            .is_some_and(|e| matches!(e.kind, ParseEventKind::OrderedField))
        {
            self.peeked = None;
        }
    }
}

impl<'de> XdrParser<'de> {
    /// Create an event with the current span.
    #[inline]
    fn event(&self, kind: ParseEventKind<'de>) -> ParseEvent<'de> {
        ParseEvent::new(kind, Span::new(self.pos, 1))
    }
}
