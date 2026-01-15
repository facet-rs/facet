//! MsgPack parser implementing FormatParser.
//!
//! This implements full FormatParser support for MsgPack deserialization,
//! with Tier-2 JIT support for compatible types.

extern crate alloc;

use alloc::{borrow::Cow, format, vec::Vec};

use crate::error::{MsgPackError, codes};
use facet_format::{
    ContainerKind, FieldEvidence, FieldKey, FieldLocationHint, FormatParser, ParseEvent,
    ProbeStream, ScalarValue,
};

// MsgPack format constants
const MSGPACK_NIL: u8 = 0xc0;
const MSGPACK_FALSE: u8 = 0xc2;
const MSGPACK_TRUE: u8 = 0xc3;
const MSGPACK_BIN8: u8 = 0xc4;
const MSGPACK_BIN16: u8 = 0xc5;
const MSGPACK_BIN32: u8 = 0xc6;
const MSGPACK_FLOAT32: u8 = 0xca;
const MSGPACK_FLOAT64: u8 = 0xcb;
const MSGPACK_UINT8: u8 = 0xcc;
const MSGPACK_UINT16: u8 = 0xcd;
const MSGPACK_UINT32: u8 = 0xce;
const MSGPACK_UINT64: u8 = 0xcf;
const MSGPACK_INT8: u8 = 0xd0;
const MSGPACK_INT16: u8 = 0xd1;
const MSGPACK_INT32: u8 = 0xd2;
const MSGPACK_INT64: u8 = 0xd3;
const MSGPACK_STR8: u8 = 0xd9;
const MSGPACK_STR16: u8 = 0xda;
const MSGPACK_STR32: u8 = 0xdb;
const MSGPACK_ARRAY16: u8 = 0xdc;
const MSGPACK_ARRAY32: u8 = 0xdd;
const MSGPACK_MAP16: u8 = 0xde;
const MSGPACK_MAP32: u8 = 0xdf;

const MSGPACK_POSFIXINT_MAX: u8 = 0x7f;
const MSGPACK_FIXMAP_MIN: u8 = 0x80;
const MSGPACK_FIXMAP_MAX: u8 = 0x8f;
const MSGPACK_FIXARRAY_MIN: u8 = 0x90;
const MSGPACK_FIXARRAY_MAX: u8 = 0x9f;
const MSGPACK_FIXSTR_MIN: u8 = 0xa0;
const MSGPACK_FIXSTR_MAX: u8 = 0xbf;
const MSGPACK_NEGFIXINT_MIN: u8 = 0xe0;

/// MsgPack parser for deserialization.
///
/// Supports both Tier-0 (FormatParser) and Tier-2 (JIT) modes.
pub struct MsgPackParser<'de> {
    input: &'de [u8],
    pos: usize,
    /// Stack tracking nested containers and their remaining items
    stack: Vec<ContextState>,
    /// Cached event for peek_event
    event_peek: Option<ParseEvent<'de>>,
}

#[derive(Debug, Clone, Copy)]
enum ContextState {
    /// Inside a map, waiting for a key (remaining pairs)
    MapKey { remaining: usize },
    /// Inside a map, waiting for a value (remaining pairs after this one)
    MapValue { remaining: usize },
    /// Inside an array (remaining items)
    Array { remaining: usize },
}

impl<'de> MsgPackParser<'de> {
    /// Create a new MsgPack parser from input bytes.
    pub const fn new(input: &'de [u8]) -> Self {
        Self {
            input,
            pos: 0,
            stack: Vec::new(),
            event_peek: None,
        }
    }

    /// Peek at the next byte without consuming it.
    fn peek_byte(&self) -> Result<u8, MsgPackError> {
        self.input
            .get(self.pos)
            .copied()
            .ok_or_else(|| MsgPackError::from_code(codes::UNEXPECTED_EOF, self.pos))
    }

    /// Read a single byte.
    fn read_byte(&mut self) -> Result<u8, MsgPackError> {
        let byte = self.peek_byte()?;
        self.pos += 1;
        Ok(byte)
    }

    /// Read N bytes as a slice.
    fn read_bytes(&mut self, n: usize) -> Result<&'de [u8], MsgPackError> {
        if self.pos + n > self.input.len() {
            return Err(MsgPackError::from_code(codes::UNEXPECTED_EOF, self.pos));
        }
        let slice = &self.input[self.pos..self.pos + n];
        self.pos += n;
        Ok(slice)
    }

    /// Read a u16 in big-endian.
    fn read_u16(&mut self) -> Result<u16, MsgPackError> {
        let bytes = self.read_bytes(2)?;
        Ok(u16::from_be_bytes([bytes[0], bytes[1]]))
    }

    /// Read a u32 in big-endian.
    fn read_u32(&mut self) -> Result<u32, MsgPackError> {
        let bytes = self.read_bytes(4)?;
        Ok(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    /// Read a u64 in big-endian.
    fn read_u64(&mut self) -> Result<u64, MsgPackError> {
        let bytes = self.read_bytes(8)?;
        Ok(u64::from_be_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]))
    }

    /// Read an i8.
    fn read_i8(&mut self) -> Result<i8, MsgPackError> {
        Ok(self.read_byte()? as i8)
    }

    /// Read an i16 in big-endian.
    fn read_i16(&mut self) -> Result<i16, MsgPackError> {
        let bytes = self.read_bytes(2)?;
        Ok(i16::from_be_bytes([bytes[0], bytes[1]]))
    }

    /// Read an i32 in big-endian.
    fn read_i32(&mut self) -> Result<i32, MsgPackError> {
        let bytes = self.read_bytes(4)?;
        Ok(i32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    /// Read an i64 in big-endian.
    fn read_i64(&mut self) -> Result<i64, MsgPackError> {
        let bytes = self.read_bytes(8)?;
        Ok(i64::from_be_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]))
    }

    /// Read an f32 in big-endian.
    fn read_f32(&mut self) -> Result<f32, MsgPackError> {
        let bytes = self.read_bytes(4)?;
        Ok(f32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    /// Read an f64 in big-endian.
    fn read_f64(&mut self) -> Result<f64, MsgPackError> {
        let bytes = self.read_bytes(8)?;
        Ok(f64::from_be_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]))
    }

    /// Read a string length based on prefix.
    fn read_str_len(&mut self, prefix: u8) -> Result<usize, MsgPackError> {
        match prefix {
            MSGPACK_FIXSTR_MIN..=MSGPACK_FIXSTR_MAX => Ok((prefix & 0x1f) as usize),
            MSGPACK_STR8 => Ok(self.read_byte()? as usize),
            MSGPACK_STR16 => Ok(self.read_u16()? as usize),
            MSGPACK_STR32 => Ok(self.read_u32()? as usize),
            _ => Err(MsgPackError {
                code: codes::EXPECTED_INT,
                pos: self.pos,
                message: format!("expected string, got 0x{:02x}", prefix),
            }),
        }
    }

    /// Read a string value.
    fn read_string(&mut self) -> Result<Cow<'de, str>, MsgPackError> {
        let prefix = self.read_byte()?;
        let len = self.read_str_len(prefix)?;
        let bytes = self.read_bytes(len)?;
        core::str::from_utf8(bytes)
            .map(Cow::Borrowed)
            .map_err(|_| MsgPackError {
                code: codes::EXPECTED_INT,
                pos: self.pos - len,
                message: "invalid UTF-8 in string".into(),
            })
    }

    /// Read an array length.
    fn read_array_len(&mut self, prefix: u8) -> Result<usize, MsgPackError> {
        match prefix {
            MSGPACK_FIXARRAY_MIN..=MSGPACK_FIXARRAY_MAX => Ok((prefix & 0x0f) as usize),
            MSGPACK_ARRAY16 => Ok(self.read_u16()? as usize),
            MSGPACK_ARRAY32 => Ok(self.read_u32()? as usize),
            _ => Err(MsgPackError::from_code(codes::EXPECTED_ARRAY, self.pos)),
        }
    }

    /// Read a map length.
    fn read_map_len(&mut self, prefix: u8) -> Result<usize, MsgPackError> {
        match prefix {
            MSGPACK_FIXMAP_MIN..=MSGPACK_FIXMAP_MAX => Ok((prefix & 0x0f) as usize),
            MSGPACK_MAP16 => Ok(self.read_u16()? as usize),
            MSGPACK_MAP32 => Ok(self.read_u32()? as usize),
            _ => Err(MsgPackError {
                code: codes::EXPECTED_INT,
                pos: self.pos,
                message: format!("expected map, got 0x{:02x}", prefix),
            }),
        }
    }

    /// Finish processing a value and update parent container state.
    fn finish_value(&mut self) {
        if let Some(context) = self.stack.last_mut() {
            match context {
                ContextState::MapValue { remaining } => {
                    // Finished a value, go back to expecting a key (or end)
                    *context = ContextState::MapKey {
                        remaining: *remaining,
                    };
                }
                ContextState::MapKey { remaining } => {
                    // This shouldn't happen (keys transition to values), but handle it
                    if *remaining > 0 {
                        *remaining -= 1;
                    }
                }
                ContextState::Array { remaining } => {
                    if *remaining > 0 {
                        *remaining -= 1;
                    }
                }
            }
        }
    }

    /// Produce the next parse event.
    fn produce_event(&mut self) -> Result<Option<ParseEvent<'de>>, MsgPackError> {
        // Check if we need to emit container end events
        // This can happen when a container has been fully consumed
        if let Some(context) = self.stack.last() {
            match context {
                ContextState::MapKey { remaining: 0 } => {
                    self.stack.pop();
                    self.finish_value();
                    return Ok(Some(ParseEvent::StructEnd));
                }
                ContextState::Array { remaining: 0 } => {
                    self.stack.pop();
                    self.finish_value();
                    return Ok(Some(ParseEvent::SequenceEnd));
                }
                _ => {}
            }
        }

        // Check if we're at EOF
        if self.pos >= self.input.len() {
            return Ok(None);
        }

        // Determine what to do based on context
        // Check if we're expecting a map key and get the remaining count
        let expecting_key_remaining = match self.stack.last() {
            Some(ContextState::MapKey { remaining }) => Some(*remaining),
            _ => None,
        };

        if let Some(remaining) = expecting_key_remaining {
            // We expect a key (string)
            let key = self.read_string()?;

            // Update the stack: decrement remaining and transition to expecting value
            let new_remaining = remaining - 1;
            if let Some(state) = self.stack.last_mut() {
                *state = ContextState::MapValue {
                    remaining: new_remaining,
                };
            }

            return Ok(Some(ParseEvent::FieldKey(FieldKey::new(
                key,
                FieldLocationHint::KeyValue,
            ))));
        }

        // Parse the next value
        let prefix = self.read_byte()?;

        match prefix {
            // Nil
            MSGPACK_NIL => {
                self.finish_value();
                Ok(Some(ParseEvent::Scalar(ScalarValue::Null)))
            }

            // Boolean
            MSGPACK_FALSE => {
                self.finish_value();
                Ok(Some(ParseEvent::Scalar(ScalarValue::Bool(false))))
            }
            MSGPACK_TRUE => {
                self.finish_value();
                Ok(Some(ParseEvent::Scalar(ScalarValue::Bool(true))))
            }

            // Positive fixint (0x00-0x7f)
            0x00..=MSGPACK_POSFIXINT_MAX => {
                self.finish_value();
                Ok(Some(ParseEvent::Scalar(ScalarValue::U64(prefix as u64))))
            }

            // Negative fixint (0xe0-0xff)
            MSGPACK_NEGFIXINT_MIN..=0xff => {
                self.finish_value();
                Ok(Some(ParseEvent::Scalar(ScalarValue::I64(
                    prefix as i8 as i64,
                ))))
            }

            // Unsigned integers
            MSGPACK_UINT8 => {
                let v = self.read_byte()? as u64;
                self.finish_value();
                Ok(Some(ParseEvent::Scalar(ScalarValue::U64(v))))
            }
            MSGPACK_UINT16 => {
                let v = self.read_u16()? as u64;
                self.finish_value();
                Ok(Some(ParseEvent::Scalar(ScalarValue::U64(v))))
            }
            MSGPACK_UINT32 => {
                let v = self.read_u32()? as u64;
                self.finish_value();
                Ok(Some(ParseEvent::Scalar(ScalarValue::U64(v))))
            }
            MSGPACK_UINT64 => {
                let v = self.read_u64()?;
                self.finish_value();
                Ok(Some(ParseEvent::Scalar(ScalarValue::U64(v))))
            }

            // Signed integers
            MSGPACK_INT8 => {
                let v = self.read_i8()? as i64;
                self.finish_value();
                Ok(Some(ParseEvent::Scalar(ScalarValue::I64(v))))
            }
            MSGPACK_INT16 => {
                let v = self.read_i16()? as i64;
                self.finish_value();
                Ok(Some(ParseEvent::Scalar(ScalarValue::I64(v))))
            }
            MSGPACK_INT32 => {
                let v = self.read_i32()? as i64;
                self.finish_value();
                Ok(Some(ParseEvent::Scalar(ScalarValue::I64(v))))
            }
            MSGPACK_INT64 => {
                let v = self.read_i64()?;
                self.finish_value();
                Ok(Some(ParseEvent::Scalar(ScalarValue::I64(v))))
            }

            // Floats
            MSGPACK_FLOAT32 => {
                let v = self.read_f32()? as f64;
                self.finish_value();
                Ok(Some(ParseEvent::Scalar(ScalarValue::F64(v))))
            }
            MSGPACK_FLOAT64 => {
                let v = self.read_f64()?;
                self.finish_value();
                Ok(Some(ParseEvent::Scalar(ScalarValue::F64(v))))
            }

            // Strings (fixstr, str8, str16, str32)
            MSGPACK_FIXSTR_MIN..=MSGPACK_FIXSTR_MAX
            | MSGPACK_STR8
            | MSGPACK_STR16
            | MSGPACK_STR32 => {
                let len = self.read_str_len(prefix)?;
                let bytes = self.read_bytes(len)?;
                let s = core::str::from_utf8(bytes)
                    .map(Cow::Borrowed)
                    .map_err(|_| MsgPackError {
                        code: codes::EXPECTED_INT,
                        pos: self.pos - len,
                        message: "invalid UTF-8 in string".into(),
                    })?;
                self.finish_value();
                Ok(Some(ParseEvent::Scalar(ScalarValue::Str(s))))
            }

            // Binary data
            MSGPACK_BIN8 => {
                let len = self.read_byte()? as usize;
                let bytes = self.read_bytes(len)?;
                self.finish_value();
                Ok(Some(ParseEvent::Scalar(ScalarValue::Bytes(Cow::Borrowed(
                    bytes,
                )))))
            }
            MSGPACK_BIN16 => {
                let len = self.read_u16()? as usize;
                let bytes = self.read_bytes(len)?;
                self.finish_value();
                Ok(Some(ParseEvent::Scalar(ScalarValue::Bytes(Cow::Borrowed(
                    bytes,
                )))))
            }
            MSGPACK_BIN32 => {
                let len = self.read_u32()? as usize;
                let bytes = self.read_bytes(len)?;
                self.finish_value();
                Ok(Some(ParseEvent::Scalar(ScalarValue::Bytes(Cow::Borrowed(
                    bytes,
                )))))
            }

            // Arrays
            MSGPACK_FIXARRAY_MIN..=MSGPACK_FIXARRAY_MAX | MSGPACK_ARRAY16 | MSGPACK_ARRAY32 => {
                let len = self.read_array_len(prefix)?;
                self.stack.push(ContextState::Array { remaining: len });
                Ok(Some(ParseEvent::SequenceStart(ContainerKind::Array)))
            }

            // Maps
            MSGPACK_FIXMAP_MIN..=MSGPACK_FIXMAP_MAX | MSGPACK_MAP16 | MSGPACK_MAP32 => {
                let len = self.read_map_len(prefix)?;
                self.stack.push(ContextState::MapKey { remaining: len });
                Ok(Some(ParseEvent::StructStart(ContainerKind::Object)))
            }

            // Unsupported types (ext, etc.)
            _ => Err(MsgPackError {
                code: codes::UNSUPPORTED,
                pos: self.pos - 1,
                message: format!("unsupported MsgPack type: 0x{:02x}", prefix),
            }),
        }
    }

    /// Skip a complete value (used for skip_value and probing).
    fn skip_value_internal(&mut self) -> Result<(), MsgPackError> {
        let prefix = self.read_byte()?;

        match prefix {
            // Nil, booleans - already consumed
            MSGPACK_NIL | MSGPACK_FALSE | MSGPACK_TRUE => Ok(()),

            // Positive fixint - already consumed
            0x00..=MSGPACK_POSFIXINT_MAX => Ok(()),

            // Negative fixint - already consumed
            MSGPACK_NEGFIXINT_MIN..=0xff => Ok(()),

            // Unsigned integers
            MSGPACK_UINT8 => {
                self.pos += 1;
                Ok(())
            }
            MSGPACK_UINT16 => {
                self.pos += 2;
                Ok(())
            }
            MSGPACK_UINT32 => {
                self.pos += 4;
                Ok(())
            }
            MSGPACK_UINT64 => {
                self.pos += 8;
                Ok(())
            }

            // Signed integers
            MSGPACK_INT8 => {
                self.pos += 1;
                Ok(())
            }
            MSGPACK_INT16 => {
                self.pos += 2;
                Ok(())
            }
            MSGPACK_INT32 => {
                self.pos += 4;
                Ok(())
            }
            MSGPACK_INT64 => {
                self.pos += 8;
                Ok(())
            }

            // Floats
            MSGPACK_FLOAT32 => {
                self.pos += 4;
                Ok(())
            }
            MSGPACK_FLOAT64 => {
                self.pos += 8;
                Ok(())
            }

            // Strings
            MSGPACK_FIXSTR_MIN..=MSGPACK_FIXSTR_MAX => {
                let len = (prefix & 0x1f) as usize;
                self.pos += len;
                Ok(())
            }
            MSGPACK_STR8 => {
                let len = self.read_byte()? as usize;
                self.pos += len;
                Ok(())
            }
            MSGPACK_STR16 => {
                let len = self.read_u16()? as usize;
                self.pos += len;
                Ok(())
            }
            MSGPACK_STR32 => {
                let len = self.read_u32()? as usize;
                self.pos += len;
                Ok(())
            }

            // Binary
            MSGPACK_BIN8 => {
                let len = self.read_byte()? as usize;
                self.pos += len;
                Ok(())
            }
            MSGPACK_BIN16 => {
                let len = self.read_u16()? as usize;
                self.pos += len;
                Ok(())
            }
            MSGPACK_BIN32 => {
                let len = self.read_u32()? as usize;
                self.pos += len;
                Ok(())
            }

            // Arrays - skip all elements
            MSGPACK_FIXARRAY_MIN..=MSGPACK_FIXARRAY_MAX => {
                let len = (prefix & 0x0f) as usize;
                for _ in 0..len {
                    self.skip_value_internal()?;
                }
                Ok(())
            }
            MSGPACK_ARRAY16 => {
                let len = self.read_u16()? as usize;
                for _ in 0..len {
                    self.skip_value_internal()?;
                }
                Ok(())
            }
            MSGPACK_ARRAY32 => {
                let len = self.read_u32()? as usize;
                for _ in 0..len {
                    self.skip_value_internal()?;
                }
                Ok(())
            }

            // Maps - skip all key-value pairs
            MSGPACK_FIXMAP_MIN..=MSGPACK_FIXMAP_MAX => {
                let len = (prefix & 0x0f) as usize;
                for _ in 0..len {
                    self.skip_value_internal()?; // key
                    self.skip_value_internal()?; // value
                }
                Ok(())
            }
            MSGPACK_MAP16 => {
                let len = self.read_u16()? as usize;
                for _ in 0..len {
                    self.skip_value_internal()?; // key
                    self.skip_value_internal()?; // value
                }
                Ok(())
            }
            MSGPACK_MAP32 => {
                let len = self.read_u32()? as usize;
                for _ in 0..len {
                    self.skip_value_internal()?; // key
                    self.skip_value_internal()?; // value
                }
                Ok(())
            }

            // Extension types - skip
            0xc7 => {
                // ext8
                let len = self.read_byte()? as usize;
                self.pos += 1 + len; // type byte + data
                Ok(())
            }
            0xc8 => {
                // ext16
                let len = self.read_u16()? as usize;
                self.pos += 1 + len;
                Ok(())
            }
            0xc9 => {
                // ext32
                let len = self.read_u32()? as usize;
                self.pos += 1 + len;
                Ok(())
            }
            0xd4 => {
                // fixext1
                self.pos += 2;
                Ok(())
            }
            0xd5 => {
                // fixext2
                self.pos += 3;
                Ok(())
            }
            0xd6 => {
                // fixext4
                self.pos += 5;
                Ok(())
            }
            0xd7 => {
                // fixext8
                self.pos += 9;
                Ok(())
            }
            0xd8 => {
                // fixext16
                self.pos += 17;
                Ok(())
            }

            _ => Err(MsgPackError {
                code: codes::UNSUPPORTED,
                pos: self.pos - 1,
                message: format!("unsupported MsgPack type: 0x{:02x}", prefix),
            }),
        }
    }

    /// Build probe evidence by scanning ahead in a map.
    fn build_probe(&self) -> Result<Vec<FieldEvidence<'de>>, MsgPackError> {
        // Create a temporary parser to scan ahead
        let mut probe_pos = self.pos;
        let mut evidence = Vec::new();

        // If we've peeked a StructStart, we need to scan the map that follows
        // Check if next byte is a map prefix
        if probe_pos >= self.input.len() {
            return Ok(evidence);
        }

        let prefix = self.input[probe_pos];
        let map_len = match prefix {
            MSGPACK_FIXMAP_MIN..=MSGPACK_FIXMAP_MAX => {
                probe_pos += 1;
                (prefix & 0x0f) as usize
            }
            MSGPACK_MAP16 => {
                if probe_pos + 3 > self.input.len() {
                    return Ok(evidence);
                }
                probe_pos += 1;
                let len = u16::from_be_bytes([self.input[probe_pos], self.input[probe_pos + 1]]);
                probe_pos += 2;
                len as usize
            }
            MSGPACK_MAP32 => {
                if probe_pos + 5 > self.input.len() {
                    return Ok(evidence);
                }
                probe_pos += 1;
                let len = u32::from_be_bytes([
                    self.input[probe_pos],
                    self.input[probe_pos + 1],
                    self.input[probe_pos + 2],
                    self.input[probe_pos + 3],
                ]);
                probe_pos += 4;
                len as usize
            }
            _ => return Ok(evidence), // Not a map
        };

        // Scan each key-value pair
        for _ in 0..map_len {
            if probe_pos >= self.input.len() {
                break;
            }

            // Read key (must be a string)
            let key_prefix = self.input[probe_pos];
            let key_len = match key_prefix {
                MSGPACK_FIXSTR_MIN..=MSGPACK_FIXSTR_MAX => {
                    probe_pos += 1;
                    (key_prefix & 0x1f) as usize
                }
                MSGPACK_STR8 => {
                    if probe_pos + 2 > self.input.len() {
                        break;
                    }
                    probe_pos += 1;
                    let len = self.input[probe_pos] as usize;
                    probe_pos += 1;
                    len
                }
                MSGPACK_STR16 => {
                    if probe_pos + 3 > self.input.len() {
                        break;
                    }
                    probe_pos += 1;
                    let len = u16::from_be_bytes([self.input[probe_pos], self.input[probe_pos + 1]])
                        as usize;
                    probe_pos += 2;
                    len
                }
                MSGPACK_STR32 => {
                    if probe_pos + 5 > self.input.len() {
                        break;
                    }
                    probe_pos += 1;
                    let len = u32::from_be_bytes([
                        self.input[probe_pos],
                        self.input[probe_pos + 1],
                        self.input[probe_pos + 2],
                        self.input[probe_pos + 3],
                    ]) as usize;
                    probe_pos += 4;
                    len
                }
                _ => break, // Non-string key, stop probing
            };

            if probe_pos + key_len > self.input.len() {
                break;
            }

            let key_bytes = &self.input[probe_pos..probe_pos + key_len];
            probe_pos += key_len;

            let key = match core::str::from_utf8(key_bytes) {
                Ok(s) => Cow::Borrowed(s),
                Err(_) => break,
            };

            // Try to read scalar value for evidence
            if probe_pos >= self.input.len() {
                evidence.push(FieldEvidence::new(key, FieldLocationHint::KeyValue, None));
                break;
            }

            let value_prefix = self.input[probe_pos];
            let scalar_value = match value_prefix {
                MSGPACK_NIL => {
                    probe_pos += 1;
                    Some(ScalarValue::Null)
                }
                MSGPACK_FALSE => {
                    probe_pos += 1;
                    Some(ScalarValue::Bool(false))
                }
                MSGPACK_TRUE => {
                    probe_pos += 1;
                    Some(ScalarValue::Bool(true))
                }
                0x00..=MSGPACK_POSFIXINT_MAX => {
                    probe_pos += 1;
                    Some(ScalarValue::U64(value_prefix as u64))
                }
                MSGPACK_NEGFIXINT_MIN..=0xff => {
                    probe_pos += 1;
                    Some(ScalarValue::I64(value_prefix as i8 as i64))
                }
                MSGPACK_FIXSTR_MIN..=MSGPACK_FIXSTR_MAX => {
                    let str_len = (value_prefix & 0x1f) as usize;
                    probe_pos += 1;
                    if probe_pos + str_len <= self.input.len() {
                        let str_bytes = &self.input[probe_pos..probe_pos + str_len];
                        probe_pos += str_len;
                        core::str::from_utf8(str_bytes)
                            .ok()
                            .map(|s| ScalarValue::Str(Cow::Borrowed(s)))
                    } else {
                        None
                    }
                }
                // For complex types, skip and don't include scalar value
                _ => {
                    // Use a temporary parser to skip the value
                    let mut tmp = MsgPackParser::new(&self.input[probe_pos..]);
                    if tmp.skip_value_internal().is_ok() {
                        probe_pos += tmp.pos;
                        None
                    } else {
                        break;
                    }
                }
            };

            if let Some(sv) = scalar_value {
                evidence.push(FieldEvidence::with_scalar_value(
                    key,
                    FieldLocationHint::KeyValue,
                    None,
                    sv,
                ));
            } else {
                evidence.push(FieldEvidence::new(key, FieldLocationHint::KeyValue, None));
            }
        }

        Ok(evidence)
    }
}

impl<'de> FormatParser<'de> for MsgPackParser<'de> {
    type Error = MsgPackError;
    type Probe<'a>
        = MsgPackProbe<'de>
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
        let evidence = self.build_probe()?;
        Ok(MsgPackProbe { evidence, idx: 0 })
    }
}

#[cfg(feature = "jit")]
impl<'de> facet_format::FormatJitParser<'de> for MsgPackParser<'de> {
    type FormatJit = crate::jit::MsgPackJitFormat;

    fn jit_input(&self) -> &'de [u8] {
        self.input
    }

    fn jit_pos(&self) -> Option<usize> {
        // Tier-2 JIT is only safe at root boundary:
        // - No peeked event (position would be ambiguous)
        // - Empty stack (we're at root level, not inside a container)
        if self.event_peek.is_some() {
            return None;
        }
        if !self.stack.is_empty() {
            return None;
        }
        Some(self.pos)
    }

    fn jit_set_pos(&mut self, pos: usize) {
        self.pos = pos;
        self.event_peek = None;
        // Stack should already be empty (jit_pos enforces this)
        debug_assert!(self.stack.is_empty());
    }

    fn jit_format(&self) -> Self::FormatJit {
        crate::jit::MsgPackJitFormat
    }

    fn jit_error(&self, _input: &'de [u8], error_pos: usize, error_code: i32) -> Self::Error {
        MsgPackError::from_code(error_code, error_pos)
    }
}

/// Probe stream for MsgPack.
pub struct MsgPackProbe<'de> {
    evidence: Vec<FieldEvidence<'de>>,
    idx: usize,
}

impl<'de> ProbeStream<'de> for MsgPackProbe<'de> {
    type Error = MsgPackError;

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
