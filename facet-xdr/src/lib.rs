#![warn(missing_docs)]
#![forbid(unsafe_code)]
#![doc = include_str!("../README.md")]

use std::io::Write;

use facet_core::{Def, Facet, NumericType, PrimitiveType, StructKind, TextualType, Type, UserType};
use facet_reflect::{HeapValue, Partial, Peek};
use facet_serialize::{Serializer, serialize_iterative};

/// Errors when serializing to XDR bytes
#[derive(Debug)]
pub enum XdrSerError {
    /// IO error
    Io(std::io::Error),
    /// Too many bytes for field
    TooManyBytes,
    /// Enum variant discriminant too large
    TooManyVariants,
    /// Unsupported type
    UnsupportedType,
}

impl core::fmt::Display for XdrSerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            XdrSerError::Io(error) => write!(f, "IO error: {error}"),
            XdrSerError::TooManyBytes => write!(f, "Too many bytes for field"),
            XdrSerError::TooManyVariants => write!(f, "Enum variant discriminant too large"),
            XdrSerError::UnsupportedType => write!(f, "Unsupported type"),
        }
    }
}

impl core::error::Error for XdrSerError {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            XdrSerError::Io(error) => Some(error),
            _ => None,
        }
    }
}

/// Serialize any Facet type to XDR bytes
pub fn to_vec<'f, F: Facet<'f>>(value: &'f F) -> Result<Vec<u8>, XdrSerError> {
    let mut buffer = Vec::new();
    let peek = Peek::new(value);
    let mut serializer = XdrSerializer {
        writer: &mut buffer,
    };
    serialize_iterative(peek, &mut serializer)?;
    Ok(buffer)
}

struct XdrSerializer<'w, W: Write> {
    writer: &'w mut W,
}

impl<W: Write> Serializer for XdrSerializer<'_, W> {
    type Error = XdrSerError;

    fn serialize_u8(&mut self, value: u8) -> Result<(), Self::Error> {
        self.serialize_u32(value as u32)
    }

    fn serialize_u16(&mut self, value: u16) -> Result<(), Self::Error> {
        self.serialize_u32(value as u32)
    }

    fn serialize_u32(&mut self, value: u32) -> Result<(), Self::Error> {
        self.writer
            .write_all(&value.to_be_bytes())
            .map_err(Self::Error::Io)
    }

    fn serialize_u64(&mut self, value: u64) -> Result<(), Self::Error> {
        self.writer
            .write_all(&value.to_be_bytes())
            .map_err(Self::Error::Io)
    }

    fn serialize_u128(&mut self, _value: u128) -> Result<(), Self::Error> {
        Err(Self::Error::UnsupportedType)
    }

    fn serialize_i8(&mut self, value: i8) -> Result<(), Self::Error> {
        self.serialize_i32(value as i32)
    }

    fn serialize_i16(&mut self, value: i16) -> Result<(), Self::Error> {
        self.serialize_i32(value as i32)
    }

    fn serialize_i32(&mut self, value: i32) -> Result<(), Self::Error> {
        self.writer
            .write_all(&value.to_be_bytes())
            .map_err(Self::Error::Io)
    }

    fn serialize_i64(&mut self, value: i64) -> Result<(), Self::Error> {
        self.writer
            .write_all(&value.to_be_bytes())
            .map_err(Self::Error::Io)
    }

    fn serialize_i128(&mut self, _value: i128) -> Result<(), Self::Error> {
        Err(Self::Error::UnsupportedType)
    }

    fn serialize_f32(&mut self, value: f32) -> Result<(), Self::Error> {
        self.writer
            .write_all(&value.to_be_bytes())
            .map_err(Self::Error::Io)
    }

    fn serialize_f64(&mut self, value: f64) -> Result<(), Self::Error> {
        self.writer
            .write_all(&value.to_be_bytes())
            .map_err(Self::Error::Io)
    }

    fn serialize_bool(&mut self, value: bool) -> Result<(), Self::Error> {
        if value {
            self.writer.write_all(&1u32.to_be_bytes())
        } else {
            self.writer.write_all(&0u32.to_be_bytes())
        }
        .map_err(Self::Error::Io)
    }

    fn serialize_char(&mut self, value: char) -> Result<(), Self::Error> {
        self.serialize_u32(value as u32)
    }

    fn serialize_str(&mut self, value: &str) -> Result<(), Self::Error> {
        let bytes = value.as_bytes();
        self.serialize_bytes(bytes)
    }

    fn serialize_bytes(&mut self, value: &[u8]) -> Result<(), Self::Error> {
        if value.len() > u32::MAX as usize {
            return Err(Self::Error::TooManyBytes);
        }
        let len = value.len() as u32;
        self.writer
            .write_all(&len.to_be_bytes())
            .map_err(Self::Error::Io)?;
        let pad_len = value.len() % 4;
        self.writer.write_all(value).map_err(Self::Error::Io)?;
        if pad_len != 0 {
            let pad = vec![0u8; 4 - pad_len];
            self.writer.write_all(&pad).map_err(Self::Error::Io)?;
        }
        Ok(())
    }

    fn start_some(&mut self) -> Result<(), Self::Error> {
        self.writer
            .write_all(&1u32.to_be_bytes())
            .map_err(Self::Error::Io)
    }

    fn serialize_none(&mut self) -> Result<(), Self::Error> {
        self.writer
            .write_all(&0u32.to_be_bytes())
            .map_err(Self::Error::Io)
    }

    fn serialize_unit(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn serialize_unit_variant(
        &mut self,
        _variant_index: usize,
        _variant_name: &'static str,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    fn start_object(&mut self, _len: Option<usize>) -> Result<(), Self::Error> {
        Ok(())
    }

    fn serialize_field_name(&mut self, _name: &'static str) -> Result<(), Self::Error> {
        Ok(())
    }

    fn start_array(&mut self, len: Option<usize>) -> Result<(), Self::Error> {
        if let Some(len) = len {
            if len > u32::MAX as usize {
                return Err(Self::Error::TooManyBytes);
            }
            self.writer
                .write_all(&(len as u32).to_be_bytes())
                .map_err(Self::Error::Io)
        } else {
            panic!("array length missing");
        }
    }

    fn start_map(&mut self, _len: Option<usize>) -> Result<(), Self::Error> {
        Ok(())
    }

    fn start_enum_variant(&mut self, discriminant: u64) -> Result<(), Self::Error> {
        if discriminant > u32::MAX as u64 {
            return Err(Self::Error::TooManyVariants);
        }
        self.writer
            .write_all(&(discriminant as u32).to_be_bytes())
            .map_err(Self::Error::Io)
    }
}

/// Errors when deserializing from XDR bytes
#[derive(Debug)]
pub enum XdrDeserError {
    /// Unsupported numeric type
    UnsupportedNumericType,
    /// Unsupported type
    UnsupportedType,
    /// Unexpected end of input
    UnexpectedEof,
    /// Invalid boolean
    InvalidBoolean {
        /// Position of this error in bytes
        position: usize,
    },
    /// Invalid discriminant for optional
    InvalidOptional {
        /// Position of this error in bytes
        position: usize,
    },
    /// Invalid enum discriminant
    InvalidVariant {
        /// Position of this error in bytes
        position: usize,
    },
    /// Invalid string
    InvalidString {
        /// Position of this error in bytes
        position: usize,
        /// Underlying UTF-8 error
        source: core::str::Utf8Error,
    },
}

impl core::fmt::Display for XdrDeserError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            XdrDeserError::UnsupportedNumericType => write!(f, "Unsupported numeric type"),
            XdrDeserError::UnsupportedType => write!(f, "Unsupported type"),
            XdrDeserError::UnexpectedEof => {
                write!(f, "Unexpected end of input")
            }
            XdrDeserError::InvalidBoolean { position } => {
                write!(f, "Invalid boolean at byte {position}")
            }
            XdrDeserError::InvalidOptional { position } => {
                write!(f, "Invalid discriminant for optional at byte {position}")
            }
            XdrDeserError::InvalidVariant { position } => {
                write!(f, "Invalid enum discriminant at byte {position}")
            }
            XdrDeserError::InvalidString { position, .. } => {
                write!(f, "Invalid string at byte {position}")
            }
        }
    }
}

impl core::error::Error for XdrDeserError {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            XdrDeserError::InvalidString { source, .. } => Some(source),
            _ => None,
        }
    }
}

#[derive(Debug, PartialEq)]
enum PopReason {
    TopLevel,
    ObjectOrListVal,
    Some,
}

#[derive(Debug)]
enum DeserializeTask {
    Value,
    Field(usize),
    ListItem,
    Pop(PopReason),
}

struct XdrDeserializerStack<'input> {
    input: &'input [u8],
    pos: usize,
    stack: Vec<DeserializeTask>,
}

impl<'input> XdrDeserializerStack<'input> {
    fn next_u32(&mut self) -> Result<u32, XdrDeserError> {
        assert_eq!(self.pos % 4, 0);
        if self.input[self.pos..].len() < 4 {
            return Err(XdrDeserError::UnexpectedEof);
        }
        let bytes = &self.input[self.pos..self.pos + 4];
        self.pos += 4;
        Ok(u32::from_be_bytes(bytes.try_into().unwrap()))
    }

    fn next_u64(&mut self) -> Result<u64, XdrDeserError> {
        assert_eq!(self.pos % 4, 0);
        if self.input[self.pos..].len() < 8 {
            return Err(XdrDeserError::UnexpectedEof);
        }
        let bytes = &self.input[self.pos..self.pos + 8];
        self.pos += 8;
        Ok(u64::from_be_bytes(bytes.try_into().unwrap()))
    }

    fn next_data(&mut self, expected_len: Option<u32>) -> Result<&'input [u8], XdrDeserError> {
        let len = self.next_u32()? as usize;
        if let Some(expected_len) = expected_len {
            assert_eq!(len, expected_len as usize);
        }
        self.pos += len;
        let pad_len = len % 4;
        let data = &self.input[self.pos - len..self.pos];
        if pad_len != 0 {
            self.pos += 4 - pad_len;
        }
        Ok(data)
    }

    fn next<'f>(&mut self, mut wip: Partial<'f>) -> Result<Partial<'f>, XdrDeserError> {
        match (wip.shape().def, wip.shape().ty) {
            (Def::Scalar, Type::Primitive(PrimitiveType::Numeric(numeric_type))) => {
                let size = wip.shape().layout.sized_layout().unwrap().size();
                match numeric_type {
                    NumericType::Integer { signed: false } => match size {
                        1 => {
                            let value = self.next_u32()? as u8;
                            wip.set(value).unwrap();
                            Ok(wip)
                        }
                        2 => {
                            let value = self.next_u32()? as u16;
                            wip.set(value).unwrap();
                            Ok(wip)
                        }
                        4 => {
                            let value = self.next_u32()?;
                            wip.set(value).unwrap();
                            Ok(wip)
                        }
                        8 => {
                            let value = self.next_u64()?;
                            wip.set(value).unwrap();
                            Ok(wip)
                        }
                        _ => {
                            // Handle usize - use 64-bit on most platforms
                            let value = self.next_u64()? as usize;
                            wip.set(value).unwrap();
                            Ok(wip)
                        }
                    },
                    NumericType::Integer { signed: true } => match size {
                        1 => {
                            let value = self.next_u32()? as i8;
                            wip.set(value).unwrap();
                            Ok(wip)
                        }
                        2 => {
                            let value = self.next_u32()? as i16;
                            wip.set(value).unwrap();
                            Ok(wip)
                        }
                        4 => {
                            let value = self.next_u32()? as i32;
                            wip.set(value).unwrap();
                            Ok(wip)
                        }
                        8 => {
                            let value = self.next_u64()? as i64;
                            wip.set(value).unwrap();
                            Ok(wip)
                        }
                        _ => {
                            // Handle isize - use 64-bit on most platforms
                            let value = self.next_u64()? as isize;
                            wip.set(value).unwrap();
                            Ok(wip)
                        }
                    },
                    NumericType::Float => match size {
                        4 => {
                            let bits = self.next_u32()?;
                            let float = f32::from_bits(bits);
                            wip.set(float).unwrap();
                            Ok(wip)
                        }
                        8 => {
                            let bits = self.next_u64()?;
                            let float = f64::from_bits(bits);
                            wip.set(float).unwrap();
                            Ok(wip)
                        }
                        _ => Err(XdrDeserError::UnsupportedNumericType),
                    },
                }
            }
            (Def::Scalar, Type::Primitive(PrimitiveType::Textual(TextualType::Str))) => {
                let string = core::str::from_utf8(self.next_data(None)?).map_err(|e| {
                    XdrDeserError::InvalidString {
                        position: self.pos - 1,
                        source: e,
                    }
                })?;
                wip.set(string.to_owned()).unwrap();
                Ok(wip)
            }
            (Def::Scalar, Type::Primitive(PrimitiveType::Boolean)) => match self.next_u32()? {
                0 => {
                    wip.set(false).unwrap();
                    Ok(wip)
                }
                1 => {
                    wip.set(true).unwrap();
                    Ok(wip)
                }
                _ => Err(XdrDeserError::InvalidBoolean {
                    position: self.pos - 4,
                }),
            },
            (Def::Scalar, Type::Primitive(PrimitiveType::Textual(TextualType::Char))) => {
                let value = self.next_u32()?;
                wip.set(char::from_u32(value).unwrap()).unwrap();
                Ok(wip)
            }
            (Def::Scalar, _) => {
                // For other scalar types (like Path, UUID, etc.), try string deserialization
                let string = core::str::from_utf8(self.next_data(None)?).map_err(|e| {
                    XdrDeserError::InvalidString {
                        position: self.pos - 1,
                        source: e,
                    }
                })?;
                wip.set(string.to_owned()).unwrap();
                Ok(wip)
            }
            (Def::List(ld), _) => {
                if ld.t().is_type::<u8>() {
                    let data = self.next_data(None)?;
                    wip.set(data.to_vec()).unwrap();
                    Ok(wip)
                } else {
                    let len = self.next_u32()?;
                    wip.begin_list().unwrap();
                    if len == 0 {
                        Ok(wip)
                    } else {
                        for _ in 0..len {
                            self.stack.push(DeserializeTask::ListItem);
                        }
                        Ok(wip)
                    }
                }
            }
            (Def::Array(ad), _) => {
                let len = ad.n;
                if ad.t().is_type::<u8>() {
                    self.pos += len;
                    let pad_len = len % 4;
                    for byte in &self.input[self.pos - len..self.pos] {
                        wip.begin_list_item().unwrap();
                        wip.set(*byte).unwrap();
                        wip.end().unwrap();
                    }
                    if pad_len != 0 {
                        self.pos += 4 - pad_len;
                    }
                    Ok(wip)
                } else {
                    for _ in 0..len {
                        self.stack.push(DeserializeTask::ListItem);
                    }
                    Ok(wip)
                }
            }
            (Def::Slice(sd), _) => {
                if sd.t().is_type::<u8>() {
                    let data = self.next_data(None)?;
                    wip.set(data.to_vec()).unwrap();
                    Ok(wip)
                } else {
                    let len = self.next_u32()?;
                    for _ in 0..len {
                        self.stack.push(DeserializeTask::ListItem);
                    }
                    Ok(wip)
                }
            }
            (Def::Option(_), _) => match self.next_u32()? {
                0 => {
                    wip.set_default().unwrap();
                    Ok(wip)
                }
                1 => {
                    self.stack.push(DeserializeTask::Pop(PopReason::Some));
                    self.stack.push(DeserializeTask::Value);
                    wip.select_variant(1).unwrap();
                    Ok(wip)
                }
                _ => Err(XdrDeserError::InvalidOptional {
                    position: self.pos - 4,
                }),
            },
            (_, Type::User(ut)) => match ut {
                UserType::Struct(st) => {
                    if st.kind == StructKind::Tuple {
                        // Handle tuple structs
                        for _field in st.fields.iter() {
                            self.stack.push(DeserializeTask::ListItem);
                        }
                        Ok(wip)
                    } else {
                        // Handle regular structs
                        for (index, _field) in st.fields.iter().enumerate().rev() {
                            if !wip.is_field_set(index).unwrap() {
                                self.stack.push(DeserializeTask::Field(index));
                            }
                        }
                        Ok(wip)
                    }
                }
                UserType::Enum(et) => {
                    let discriminant = self.next_u32()?;
                    if let Some(variant) = et
                        .variants
                        .iter()
                        .find(|v| v.discriminant == Some(discriminant as i64))
                        .or(et.variants.get(discriminant as usize))
                    {
                        for (index, _field) in variant.data.fields.iter().enumerate().rev() {
                            self.stack.push(DeserializeTask::Field(index));
                        }
                        wip.select_variant(discriminant as i64).unwrap();
                        Ok(wip)
                    } else {
                        Err(XdrDeserError::InvalidVariant {
                            position: self.pos - 4,
                        })
                    }
                }
                _ => Err(XdrDeserError::UnsupportedType),
            },
            _ => Err(XdrDeserError::UnsupportedType),
        }
    }
}

/// Deserialize an XDR slice given some some [`Partial`] into a [`HeapValue`]
pub fn deserialize_wip<'facet>(
    input: &[u8],
    mut wip: Partial<'facet>,
) -> Result<HeapValue<'facet>, XdrDeserError> {
    let mut runner = XdrDeserializerStack {
        input,
        pos: 0,
        stack: vec![
            DeserializeTask::Pop(PopReason::TopLevel),
            DeserializeTask::Value,
        ],
    };

    loop {
        // We no longer have access to frames_count
        // The frame count assertion has been removed as it's an internal implementation detail

        match runner.stack.pop() {
            Some(DeserializeTask::Pop(reason)) => {
                if reason == PopReason::TopLevel {
                    return Ok(wip.build().unwrap());
                } else {
                    wip.end().unwrap();
                }
            }
            Some(DeserializeTask::Value) => {
                wip = runner.next(wip)?;
            }
            Some(DeserializeTask::Field(index)) => {
                runner
                    .stack
                    .push(DeserializeTask::Pop(PopReason::ObjectOrListVal));
                runner.stack.push(DeserializeTask::Value);
                wip.begin_nth_field(index).unwrap();
            }
            Some(DeserializeTask::ListItem) => {
                runner
                    .stack
                    .push(DeserializeTask::Pop(PopReason::ObjectOrListVal));
                runner.stack.push(DeserializeTask::Value);
                wip.begin_list_item().unwrap();
            }
            None => unreachable!("Instruction stack is empty"),
        }
    }
}

/// Deserialize a slice of XDR bytes into any Facet type
pub fn deserialize<'f, F: facet_core::Facet<'f>>(input: &[u8]) -> Result<F, XdrDeserError> {
    let v = deserialize_wip(input, Partial::alloc_shape(F::SHAPE).unwrap())?;
    let f: F = v.materialize().unwrap();
    Ok(f)
}
