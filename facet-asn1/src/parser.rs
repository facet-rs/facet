//! ASN.1 DER/BER parser implementing FormatParser.
//!
//! ASN.1 DER (Distinguished Encoding Rules) is a TLV (Tag-Length-Value) format.
//! This parser translates DER structures into the ParseEvent stream.

extern crate alloc;

use alloc::{borrow::Cow, vec::Vec};

use crate::error::{Asn1Error, Asn1ErrorKind};
use facet_format::{
    ContainerKind, FieldEvidence, FormatParser, ParseEvent, ProbeStream, ScalarValue,
};

// ASN.1 Universal Tags
const TAG_BOOLEAN: u8 = 0x01;
const TAG_INTEGER: u8 = 0x02;
const TAG_OCTET_STRING: u8 = 0x04;

const TAG_REAL: u8 = 0x09;
const TAG_UTF8STRING: u8 = 0x0C;

// Tag class masks
const CLASS_MASK: u8 = 0xC0;
const CLASS_UNIVERSAL: u8 = 0x00;
const CLASS_CONTEXT: u8 = 0x80;
const CONSTRUCTED_BIT: u8 = 0x20;

// Real format special values
const REAL_INFINITY: u8 = 0b01000000;
const REAL_NEG_INFINITY: u8 = 0b01000001;
const REAL_NAN: u8 = 0b01000010;
const REAL_NEG_ZERO: u8 = 0b01000011;

const F64_MANTISSA_MASK: u64 = 0b1111111111111111111111111111111111111111111111111111;

/// ASN.1 DER parser.
pub struct Asn1Parser<'de> {
    input: &'de [u8],
    pos: usize,
    /// Stack of (end_position, is_struct, remaining_fields) for tracking nested containers
    stack: Vec<ContainerState>,
    /// Cached event for peek_event
    event_peek: Option<ParseEvent<'de>>,
    /// Current field index within a sequence (for positional mapping)
    field_indices: Vec<usize>,
    /// Hint: next SEQUENCE should be parsed as an array (not struct)
    pending_sequence: bool,
    /// Hint: number of fields expected in the next struct
    pending_struct_fields: Option<usize>,
    /// Pending scalar type hint
    pending_scalar_type: Option<facet_format::ScalarTypeHint>,
}

#[derive(Debug, Clone)]
struct ContainerState {
    /// End position of this container
    end: usize,
    /// Whether this is a struct (true) or array (false)
    is_sequence: bool,
    /// Remaining fields to emit OrderedField for (0 means auto-detect from content)
    remaining_fields: usize,
    /// Whether we just emitted OrderedField and are waiting for the value
    awaiting_value: bool,
}

impl<'de> Asn1Parser<'de> {
    /// Create a new ASN.1 DER parser from input bytes.
    pub const fn new(input: &'de [u8]) -> Self {
        Self {
            input,
            pos: 0,
            stack: Vec::new(),
            event_peek: None,
            field_indices: Vec::new(),
            pending_sequence: false,
            pending_struct_fields: None,
            pending_scalar_type: None,
        }
    }

    /// Peek at the next byte without consuming it.
    fn peek_byte(&self) -> Result<u8, Asn1Error> {
        self.input
            .get(self.pos)
            .copied()
            .ok_or_else(|| Asn1Error::unexpected_eof(self.pos))
    }

    /// Read a single byte.
    fn read_byte(&mut self) -> Result<u8, Asn1Error> {
        let byte = self.peek_byte()?;
        self.pos += 1;
        Ok(byte)
    }

    /// Read the length field of a TLV.
    fn read_length(&mut self) -> Result<usize, Asn1Error> {
        let first = self.read_byte()?;
        if first < 128 {
            Ok(first as usize)
        } else {
            let num_bytes = (first & 0x7f) as usize;
            if num_bytes == 0 {
                return Err(Asn1Error::new(
                    Asn1ErrorKind::Unsupported {
                        message: "indefinite length not supported".into(),
                    },
                    self.pos,
                ));
            }
            if num_bytes > 8 {
                return Err(Asn1Error::new(
                    Asn1ErrorKind::Unsupported {
                        message: "length too large".into(),
                    },
                    self.pos,
                ));
            }
            let mut len = 0usize;
            for _ in 0..num_bytes {
                len = len.checked_shl(8).ok_or_else(|| {
                    Asn1Error::new(
                        Asn1ErrorKind::Unsupported {
                            message: "length overflow".into(),
                        },
                        self.pos,
                    )
                })?;
                len |= self.read_byte()? as usize;
            }
            Ok(len)
        }
    }

    /// Read a TLV header (tag + length), return (tag, content_end_position).
    fn read_tl(&mut self) -> Result<(u8, usize), Asn1Error> {
        let tag = self.read_byte()?;
        let len = self.read_length()?;
        let end = self.pos.checked_add(len).ok_or_else(|| {
            Asn1Error::new(
                Asn1ErrorKind::Unsupported {
                    message: "content length overflow".into(),
                },
                self.pos,
            )
        })?;
        if end > self.input.len() {
            return Err(Asn1Error::unexpected_eof(self.pos));
        }
        Ok((tag, end))
    }

    /// Read a complete TLV, return (tag, value_bytes).
    fn read_tlv(&mut self) -> Result<(u8, &'de [u8]), Asn1Error> {
        let (tag, end) = self.read_tl()?;
        let start = self.pos;
        self.pos = end;
        Ok((tag, &self.input[start..end]))
    }

    /// Read a boolean value.
    fn read_bool(&mut self) -> Result<bool, Asn1Error> {
        let (tag, bytes) = self.read_tlv()?;
        if tag != TAG_BOOLEAN {
            return Err(Asn1Error::unknown_tag(tag, self.pos));
        }
        match bytes {
            [0x00] => Ok(false),
            [0xFF] => Ok(true),
            [_] => Err(Asn1Error::new(Asn1ErrorKind::InvalidBool, self.pos)),
            _ => Err(Asn1Error::new(
                Asn1ErrorKind::LengthMismatch {
                    expected: 1,
                    got: bytes.len(),
                },
                self.pos,
            )),
        }
    }

    /// Read an integer value as i64.
    fn read_integer(&mut self) -> Result<i64, Asn1Error> {
        let (tag, bytes) = self.read_tlv()?;
        if tag != TAG_INTEGER {
            return Err(Asn1Error::unknown_tag(tag, self.pos));
        }
        if bytes.is_empty() {
            return Ok(0);
        }
        // Sign-extend from first byte
        let mut value = bytes[0] as i8 as i64;
        for &byte in &bytes[1..] {
            value = (value << 8) | (byte as i64);
        }
        Ok(value)
    }

    /// Read a real (float) value as f64.
    fn read_real(&mut self) -> Result<f64, Asn1Error> {
        let (tag, bytes) = self.read_tlv()?;
        if tag != TAG_REAL {
            return Err(Asn1Error::unknown_tag(tag, self.pos));
        }
        if bytes.is_empty() {
            return Ok(0.0);
        }
        match bytes[0] {
            REAL_INFINITY => Ok(f64::INFINITY),
            REAL_NEG_INFINITY => Ok(f64::NEG_INFINITY),
            REAL_NAN => Ok(f64::NAN),
            REAL_NEG_ZERO => Ok(-0.0),
            struct_byte => {
                if struct_byte & 0b10111100 != 0b10000000 {
                    return Err(Asn1Error::new(Asn1ErrorKind::InvalidReal, self.pos));
                }
                let sign_negative = (struct_byte >> 6 & 0b1) > 0;
                let exponent_len = ((struct_byte & 0b11) + 1) as usize;
                if bytes.len() < exponent_len + 2 {
                    return Err(Asn1Error::new(
                        Asn1ErrorKind::LengthMismatch {
                            expected: exponent_len + 2,
                            got: bytes.len(),
                        },
                        self.pos,
                    ));
                }

                // Parse exponent
                let mut exponent = bytes[1] as i8 as i64;
                for &byte in &bytes[2..1 + exponent_len] {
                    exponent = (exponent << 8) | (byte as u64 as i64);
                }

                if exponent > 1023 {
                    return Ok(if sign_negative {
                        f64::NEG_INFINITY
                    } else {
                        f64::INFINITY
                    });
                }

                // Parse mantissa
                let mut mantissa = 0u64;
                for &byte in bytes[1 + exponent_len..].iter().take(7) {
                    mantissa = (mantissa << 8) | (byte as u64);
                }

                // Normalize mantissa
                let mut normalization_factor = 52i64;
                while mantissa & (0b1 << 52) == 0 && normalization_factor > 0 {
                    mantissa <<= 1;
                    normalization_factor -= 1;
                }
                exponent += normalization_factor + 1023;

                Ok(f64::from_bits(
                    (sign_negative as u64) << 63
                        | ((exponent as u64) & 0b11111111111) << 52
                        | (mantissa & F64_MANTISSA_MASK),
                ))
            }
        }
    }

    /// Read a UTF-8 string.
    fn read_string(&mut self) -> Result<&'de str, Asn1Error> {
        let (tag, bytes) = self.read_tlv()?;
        if tag != TAG_UTF8STRING {
            return Err(Asn1Error::unknown_tag(tag, self.pos));
        }
        core::str::from_utf8(bytes).map_err(|e| {
            Asn1Error::new(
                Asn1ErrorKind::InvalidString {
                    message: e.to_string(),
                },
                self.pos,
            )
        })
    }

    /// Read an octet string (binary data).
    fn read_octet_string(&mut self) -> Result<&'de [u8], Asn1Error> {
        let (tag, bytes) = self.read_tlv()?;
        if tag != TAG_OCTET_STRING {
            return Err(Asn1Error::unknown_tag(tag, self.pos));
        }
        Ok(bytes)
    }

    /// Finish processing a value.
    fn finish_value(&mut self) {
        // Update field index if in a sequence
        if let Some(idx) = self.field_indices.last_mut() {
            *idx += 1;
        }
    }

    /// Produce the next parse event.
    fn produce_event(&mut self) -> Result<Option<ParseEvent<'de>>, Asn1Error> {
        // Check if we need to emit container end events
        if let Some(state) = self.stack.last()
            && self.pos >= state.end
        {
            let state = self.stack.pop().unwrap();
            self.field_indices.pop();
            if state.is_sequence {
                return Ok(Some(ParseEvent::StructEnd));
            } else {
                return Ok(Some(ParseEvent::SequenceEnd));
            }
        }

        // Check if we're done
        if self.pos >= self.input.len() {
            return Ok(None);
        }

        // Check if we're in a struct with remaining fields - emit OrderedField
        // But only if we're not already waiting for a value
        if let Some(state) = self.stack.last()
            && state.is_sequence
            && state.remaining_fields > 0
            && !state.awaiting_value
        {
            // Decrement remaining fields and mark that we're awaiting a value
            if let Some(state) = self.stack.last_mut() {
                state.remaining_fields -= 1;
                state.awaiting_value = true;
            }
            return Ok(Some(ParseEvent::OrderedField));
        }

        // Clear the awaiting_value flag since we're about to produce a value
        if let Some(state) = self.stack.last_mut() {
            state.awaiting_value = false;
        }

        // Clear pending scalar type hint (we don't need it for ASN.1 - types are in TLV)
        self.pending_scalar_type = None;

        // Peek at the tag to determine what to parse
        let tag = self.peek_byte()?;
        let tag_class = tag & CLASS_MASK;
        let is_constructed = (tag & CONSTRUCTED_BIT) != 0;
        let tag_number = tag & 0x1F;

        match (tag_class, is_constructed, tag_number) {
            // Universal constructed SEQUENCE
            (CLASS_UNIVERSAL, true, 0x10) => {
                let (_, end) = self.read_tl()?;
                // Check if hint_sequence was called - if so, parse as array
                let as_array = self.pending_sequence;
                self.pending_sequence = false;

                // Get pending struct field count (for positional field handling)
                let remaining_fields = self.pending_struct_fields.take().unwrap_or(0);

                self.stack.push(ContainerState {
                    end,
                    is_sequence: !as_array, // is_sequence means "is struct" here
                    remaining_fields,
                    awaiting_value: false,
                });
                self.field_indices.push(0);

                if as_array {
                    Ok(Some(ParseEvent::SequenceStart(ContainerKind::Array)))
                } else {
                    Ok(Some(ParseEvent::StructStart(ContainerKind::Object)))
                }
            }

            // Universal primitive BOOLEAN
            (CLASS_UNIVERSAL, false, 0x01) => {
                let value = self.read_bool()?;
                self.finish_value();
                Ok(Some(ParseEvent::Scalar(ScalarValue::Bool(value))))
            }

            // Universal primitive INTEGER
            (CLASS_UNIVERSAL, false, 0x02) => {
                let value = self.read_integer()?;
                self.finish_value();
                Ok(Some(ParseEvent::Scalar(ScalarValue::I64(value))))
            }

            // Universal primitive OCTET STRING
            (CLASS_UNIVERSAL, false, 0x04) => {
                let bytes = self.read_octet_string()?;
                self.finish_value();
                Ok(Some(ParseEvent::Scalar(ScalarValue::Bytes(Cow::Borrowed(
                    bytes,
                )))))
            }

            // Universal primitive NULL
            (CLASS_UNIVERSAL, false, 0x05) => {
                let _ = self.read_tlv()?;
                self.finish_value();
                Ok(Some(ParseEvent::Scalar(ScalarValue::Null)))
            }

            // Universal primitive REAL
            (CLASS_UNIVERSAL, false, 0x09) => {
                let value = self.read_real()?;
                self.finish_value();
                Ok(Some(ParseEvent::Scalar(ScalarValue::F64(value))))
            }

            // Universal primitive UTF8String
            (CLASS_UNIVERSAL, false, 0x0C) => {
                let s = self.read_string()?;
                self.finish_value();
                Ok(Some(ParseEvent::Scalar(ScalarValue::Str(Cow::Borrowed(s)))))
            }

            // Context-specific tags (used for enum variants and optional fields)
            (CLASS_CONTEXT, _, _) => {
                let (_, end) = self.read_tl()?;
                if is_constructed {
                    self.stack.push(ContainerState {
                        end,
                        is_sequence: true,
                        remaining_fields: 0,
                        awaiting_value: false,
                    });
                    self.field_indices.push(0);
                    Ok(Some(ParseEvent::StructStart(ContainerKind::Object)))
                } else {
                    // For primitive context-specific, treat as variant discriminant
                    // The tag number is often used as the variant index
                    self.finish_value();
                    Ok(Some(ParseEvent::Scalar(ScalarValue::U64(
                        tag_number as u64,
                    ))))
                }
            }

            _ => {
                // Skip unsupported tags
                let (_, end) = self.read_tl()?;
                self.pos = end;
                self.produce_event()
            }
        }
    }

    /// Skip a complete TLV value.
    fn skip_value_internal(&mut self) -> Result<(), Asn1Error> {
        let (_, end) = self.read_tl()?;
        self.pos = end;
        Ok(())
    }
}

impl<'de> FormatParser<'de> for Asn1Parser<'de> {
    type Error = Asn1Error;
    type Probe<'a>
        = Asn1Probe<'de>
    where
        Self: 'a;

    fn next_event(&mut self) -> Result<Option<ParseEvent<'de>>, Self::Error> {
        if let Some(event) = self.event_peek.take() {
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
        }
        Ok(event)
    }

    fn skip_value(&mut self) -> Result<(), Self::Error> {
        debug_assert!(
            self.event_peek.is_none(),
            "skip_value called while an event is buffered"
        );
        self.skip_value_internal()?;
        self.finish_value();
        Ok(())
    }

    fn begin_probe(&mut self) -> Result<Self::Probe<'_>, Self::Error> {
        // ASN.1 DER doesn't have field names in the encoding (they're defined by the schema)
        // So probing returns an empty list - the deserializer will use positional matching
        Ok(Asn1Probe::new())
    }

    fn is_self_describing(&self) -> bool {
        // ASN.1 DER doesn't include field names - it uses positional encoding
        // So we use the hint-based approach like postcard
        false
    }

    fn hint_struct_fields(&mut self, num_fields: usize) {
        self.pending_struct_fields = Some(num_fields);
        // Clear any peeked event since the interpretation may change
        if matches!(self.event_peek, Some(ParseEvent::OrderedField)) {
            self.event_peek = None;
        }
    }

    fn hint_scalar_type(&mut self, hint: facet_format::ScalarTypeHint) {
        self.pending_scalar_type = Some(hint);
        // Clear any peeked OrderedField since we're about to read a value
        if matches!(self.event_peek, Some(ParseEvent::OrderedField)) {
            self.event_peek = None;
        }
    }

    fn hint_sequence(&mut self) {
        self.pending_sequence = true;
        // Clear any peeked event since the interpretation may change
        if matches!(self.event_peek, Some(ParseEvent::StructStart(_))) {
            self.event_peek = None;
        }
    }
}

/// Probe stream for ASN.1.
///
/// ASN.1 DER doesn't include field names in the encoding - they're defined by the schema.
/// So probing always returns empty (positional matching is used).
pub struct Asn1Probe<'de> {
    _marker: core::marker::PhantomData<&'de ()>,
}

impl<'de> Asn1Probe<'de> {
    const fn new() -> Self {
        Self {
            _marker: core::marker::PhantomData,
        }
    }
}

impl<'de> ProbeStream<'de> for Asn1Probe<'de> {
    type Error = Asn1Error;

    fn next(&mut self) -> Result<Option<FieldEvidence<'de>>, Self::Error> {
        // ASN.1 uses positional fields, not named fields
        Ok(None)
    }
}
