#![warn(missing_docs)]
#![forbid(unsafe_code)]
#![doc = include_str!("../README.md")]

use std::io::Write;

use facet_core::{Def, Facet, NumericType, PrimitiveType, StructKind, TextualType, Type, UserType};
use facet_reflect::{HasFields, HeapValue, Partial, Peek, ScalarType};

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
    serialize_value(peek, &mut buffer)?;
    Ok(buffer)
}

fn serialize_value<W: Write>(peek: Peek<'_, '_>, writer: &mut W) -> Result<(), XdrSerError> {
    match (peek.shape().def, peek.shape().ty) {
        (Def::Scalar, _) => {
            let peek = peek.innermost_peek();
            serialize_scalar(peek, writer)
        }
        (Def::List(ld), _) => {
            // Special case for Vec<u8> - serialize as opaque
            if ld.t().is_type::<u8>() && peek.shape().is_type::<Vec<u8>>() {
                let bytes = peek.get::<Vec<u8>>().unwrap();
                serialize_bytes(bytes, writer)
            } else {
                let list = peek.into_list_like().unwrap();
                let items: Vec<_> = list.iter().collect();
                serialize_array(items, writer)
            }
        }
        (Def::Array(ad), _) => {
            if ad.t().is_type::<u8>() {
                // Fixed-size byte array
                let bytes: Vec<u8> = peek
                    .into_list_like()
                    .unwrap()
                    .iter()
                    .map(|p| *p.get::<u8>().unwrap())
                    .collect();
                // For fixed-size arrays, don't write length prefix
                writer.write_all(&bytes).map_err(XdrSerError::Io)?;
                let pad_len = bytes.len() % 4;
                if pad_len != 0 {
                    let pad = vec![0u8; 4 - pad_len];
                    writer.write_all(&pad).map_err(XdrSerError::Io)?;
                }
                Ok(())
            } else {
                let list = peek.into_list_like().unwrap();
                let items: Vec<_> = list.iter().collect();
                // For fixed-size arrays, don't write length prefix
                for item in items {
                    serialize_value(item, writer)?;
                }
                Ok(())
            }
        }
        (Def::Slice(sd), _) => {
            if sd.t().is_type::<u8>() {
                let bytes = peek.get::<[u8]>().unwrap();
                serialize_bytes(bytes, writer)
            } else {
                let list = peek.into_list_like().unwrap();
                let items: Vec<_> = list.iter().collect();
                serialize_array(items, writer)
            }
        }
        (Def::Option(_), _) => {
            let opt = peek.into_option().unwrap();
            if let Some(inner) = opt.value() {
                // Some - discriminant 1
                writer
                    .write_all(&1u32.to_be_bytes())
                    .map_err(XdrSerError::Io)?;
                serialize_value(inner, writer)
            } else {
                // None - discriminant 0
                writer
                    .write_all(&0u32.to_be_bytes())
                    .map_err(XdrSerError::Io)?;
                Ok(())
            }
        }
        (Def::Pointer(_), _) => {
            let ptr = peek.into_pointer().unwrap();
            if let Some(inner) = ptr.borrow_inner() {
                serialize_value(inner, writer)
            } else {
                Err(XdrSerError::UnsupportedType)
            }
        }
        (_, Type::User(UserType::Struct(sd))) => match sd.kind {
            StructKind::Unit => {
                // Unit structs serialize as nothing in XDR
                Ok(())
            }
            StructKind::Tuple | StructKind::TupleStruct => {
                let ps = peek.into_struct().unwrap();
                for (_, field_value) in ps.fields_for_serialize() {
                    serialize_value(field_value, writer)?;
                }
                Ok(())
            }
            StructKind::Struct => {
                let ps = peek.into_struct().unwrap();
                for (_, field_value) in ps.fields_for_serialize() {
                    serialize_value(field_value, writer)?;
                }
                Ok(())
            }
        },
        (_, Type::User(UserType::Enum(et))) => {
            let pe = peek.into_enum().unwrap();
            let variant = pe.active_variant().expect("Failed to get active variant");

            // Get discriminant - find variant index if no explicit discriminant
            let variant_index = et
                .variants
                .iter()
                .position(|v| v.name == variant.name)
                .unwrap_or(0);
            let discriminant = variant.discriminant.unwrap_or(variant_index as i64);
            if discriminant < 0 || discriminant > u32::MAX as i64 {
                return Err(XdrSerError::TooManyVariants);
            }

            // Write discriminant
            writer
                .write_all(&(discriminant as u32).to_be_bytes())
                .map_err(XdrSerError::Io)?;

            // Serialize variant fields
            for (_, field_value) in pe.fields_for_serialize() {
                serialize_value(field_value, writer)?;
            }
            Ok(())
        }
        (_, Type::Pointer(_)) => {
            // Handle string types
            if let Some(s) = peek.as_str() {
                serialize_str(s, writer)
            } else if let Some(bytes) = peek.as_bytes() {
                serialize_bytes(bytes, writer)
            } else {
                let innermost = peek.innermost_peek();
                if innermost.shape() != peek.shape() {
                    serialize_value(innermost, writer)
                } else {
                    Err(XdrSerError::UnsupportedType)
                }
            }
        }
        _ => Err(XdrSerError::UnsupportedType),
    }
}

fn serialize_scalar<W: Write>(peek: Peek<'_, '_>, writer: &mut W) -> Result<(), XdrSerError> {
    match peek.scalar_type() {
        Some(ScalarType::Unit) => Ok(()),
        Some(ScalarType::Bool) => {
            let v = *peek.get::<bool>().unwrap();
            let val: u32 = if v { 1 } else { 0 };
            writer
                .write_all(&val.to_be_bytes())
                .map_err(XdrSerError::Io)
        }
        Some(ScalarType::Char) => {
            let c = *peek.get::<char>().unwrap();
            writer
                .write_all(&(c as u32).to_be_bytes())
                .map_err(XdrSerError::Io)
        }
        Some(ScalarType::Str) => {
            let s = peek.get::<str>().unwrap();
            serialize_str(s, writer)
        }
        Some(ScalarType::String) => {
            let s = peek.get::<String>().unwrap();
            serialize_str(s, writer)
        }
        Some(ScalarType::CowStr) => {
            let s = peek.get::<std::borrow::Cow<'_, str>>().unwrap();
            serialize_str(s, writer)
        }
        Some(ScalarType::F32) => {
            let v = *peek.get::<f32>().unwrap();
            writer.write_all(&v.to_be_bytes()).map_err(XdrSerError::Io)
        }
        Some(ScalarType::F64) => {
            let v = *peek.get::<f64>().unwrap();
            writer.write_all(&v.to_be_bytes()).map_err(XdrSerError::Io)
        }
        Some(ScalarType::U8) => {
            let v = *peek.get::<u8>().unwrap();
            writer
                .write_all(&(v as u32).to_be_bytes())
                .map_err(XdrSerError::Io)
        }
        Some(ScalarType::U16) => {
            let v = *peek.get::<u16>().unwrap();
            writer
                .write_all(&(v as u32).to_be_bytes())
                .map_err(XdrSerError::Io)
        }
        Some(ScalarType::U32) => {
            let v = *peek.get::<u32>().unwrap();
            writer.write_all(&v.to_be_bytes()).map_err(XdrSerError::Io)
        }
        Some(ScalarType::U64) => {
            let v = *peek.get::<u64>().unwrap();
            writer.write_all(&v.to_be_bytes()).map_err(XdrSerError::Io)
        }
        Some(ScalarType::U128) => Err(XdrSerError::UnsupportedType),
        Some(ScalarType::USize) => {
            let v = *peek.get::<usize>().unwrap();
            writer
                .write_all(&(v as u64).to_be_bytes())
                .map_err(XdrSerError::Io)
        }
        Some(ScalarType::I8) => {
            let v = *peek.get::<i8>().unwrap();
            writer
                .write_all(&(v as i32).to_be_bytes())
                .map_err(XdrSerError::Io)
        }
        Some(ScalarType::I16) => {
            let v = *peek.get::<i16>().unwrap();
            writer
                .write_all(&(v as i32).to_be_bytes())
                .map_err(XdrSerError::Io)
        }
        Some(ScalarType::I32) => {
            let v = *peek.get::<i32>().unwrap();
            writer.write_all(&v.to_be_bytes()).map_err(XdrSerError::Io)
        }
        Some(ScalarType::I64) => {
            let v = *peek.get::<i64>().unwrap();
            writer.write_all(&v.to_be_bytes()).map_err(XdrSerError::Io)
        }
        Some(ScalarType::I128) => Err(XdrSerError::UnsupportedType),
        Some(ScalarType::ISize) => {
            let v = *peek.get::<isize>().unwrap();
            writer
                .write_all(&(v as i64).to_be_bytes())
                .map_err(XdrSerError::Io)
        }
        Some(_) | None => Err(XdrSerError::UnsupportedType),
    }
}

fn serialize_str<W: Write>(s: &str, writer: &mut W) -> Result<(), XdrSerError> {
    serialize_bytes(s.as_bytes(), writer)
}

fn serialize_bytes<W: Write>(bytes: &[u8], writer: &mut W) -> Result<(), XdrSerError> {
    if bytes.len() > u32::MAX as usize {
        return Err(XdrSerError::TooManyBytes);
    }
    let len = bytes.len() as u32;
    writer
        .write_all(&len.to_be_bytes())
        .map_err(XdrSerError::Io)?;
    writer.write_all(bytes).map_err(XdrSerError::Io)?;
    let pad_len = bytes.len() % 4;
    if pad_len != 0 {
        let pad = vec![0u8; 4 - pad_len];
        writer.write_all(&pad).map_err(XdrSerError::Io)?;
    }
    Ok(())
}

fn serialize_array<W: Write>(items: Vec<Peek<'_, '_>>, writer: &mut W) -> Result<(), XdrSerError> {
    if items.len() > u32::MAX as usize {
        return Err(XdrSerError::TooManyBytes);
    }
    writer
        .write_all(&(items.len() as u32).to_be_bytes())
        .map_err(XdrSerError::Io)?;
    for item in items {
        serialize_value(item, writer)?;
    }
    Ok(())
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

    fn next<'f>(&mut self, wip: Partial<'f>) -> Result<Partial<'f>, XdrDeserError> {
        match (wip.shape().def, wip.shape().ty) {
            (Def::Scalar, Type::Primitive(PrimitiveType::Numeric(numeric_type))) => {
                let size = wip.shape().layout.sized_layout().unwrap().size();
                match numeric_type {
                    NumericType::Integer { signed: false } => match size {
                        1 => {
                            let value = self.next_u32()? as u8;
                            let wip = wip.set(value).unwrap();
                            Ok(wip)
                        }
                        2 => {
                            let value = self.next_u32()? as u16;
                            let wip = wip.set(value).unwrap();
                            Ok(wip)
                        }
                        4 => {
                            let value = self.next_u32()?;
                            let wip = wip.set(value).unwrap();
                            Ok(wip)
                        }
                        8 => {
                            let value = self.next_u64()?;
                            let wip = wip.set(value).unwrap();
                            Ok(wip)
                        }
                        _ => {
                            // Handle usize - use 64-bit on most platforms
                            let value = self.next_u64()? as usize;
                            let wip = wip.set(value).unwrap();
                            Ok(wip)
                        }
                    },
                    NumericType::Integer { signed: true } => match size {
                        1 => {
                            let value = self.next_u32()? as i8;
                            let wip = wip.set(value).unwrap();
                            Ok(wip)
                        }
                        2 => {
                            let value = self.next_u32()? as i16;
                            let wip = wip.set(value).unwrap();
                            Ok(wip)
                        }
                        4 => {
                            let value = self.next_u32()? as i32;
                            let wip = wip.set(value).unwrap();
                            Ok(wip)
                        }
                        8 => {
                            let value = self.next_u64()? as i64;
                            let wip = wip.set(value).unwrap();
                            Ok(wip)
                        }
                        _ => {
                            // Handle isize - use 64-bit on most platforms
                            let value = self.next_u64()? as isize;
                            let wip = wip.set(value).unwrap();
                            Ok(wip)
                        }
                    },
                    NumericType::Float => match size {
                        4 => {
                            let bits = self.next_u32()?;
                            let float = f32::from_bits(bits);
                            let wip = wip.set(float).unwrap();
                            Ok(wip)
                        }
                        8 => {
                            let bits = self.next_u64()?;
                            let float = f64::from_bits(bits);
                            let wip = wip.set(float).unwrap();
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
                let wip = wip.set(string.to_owned()).unwrap();
                Ok(wip)
            }
            (Def::Scalar, Type::Primitive(PrimitiveType::Boolean)) => match self.next_u32()? {
                0 => {
                    let wip = wip.set(false).unwrap();
                    Ok(wip)
                }
                1 => {
                    let wip = wip.set(true).unwrap();
                    Ok(wip)
                }
                _ => Err(XdrDeserError::InvalidBoolean {
                    position: self.pos - 4,
                }),
            },
            (Def::Scalar, Type::Primitive(PrimitiveType::Textual(TextualType::Char))) => {
                let value = self.next_u32()?;
                let wip = wip.set(char::from_u32(value).unwrap()).unwrap();
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
                let wip = wip.set(string.to_owned()).unwrap();
                Ok(wip)
            }
            (Def::List(ld), _) => {
                if ld.t().is_type::<u8>() {
                    let data = self.next_data(None)?;
                    let wip = wip.set(data.to_vec()).unwrap();
                    Ok(wip)
                } else {
                    let len = self.next_u32()?;
                    let wip = wip.begin_list().unwrap();
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
                    let mut wip = wip;
                    for byte in &self.input[self.pos - len..self.pos] {
                        wip = wip.begin_list_item().unwrap();
                        wip = wip.set(*byte).unwrap();
                        wip = wip.end().unwrap();
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
                    let wip = wip.set(data.to_vec()).unwrap();
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
                    let wip = wip.set_default().unwrap();
                    Ok(wip)
                }
                1 => {
                    self.stack.push(DeserializeTask::Pop(PopReason::Some));
                    self.stack.push(DeserializeTask::Value);
                    let wip = wip.select_variant(1).unwrap();
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
                        let wip = wip.select_variant(discriminant as i64).unwrap();
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
                    wip = wip.end().unwrap();
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
                wip = wip.begin_nth_field(index).unwrap();
            }
            Some(DeserializeTask::ListItem) => {
                runner
                    .stack
                    .push(DeserializeTask::Pop(PopReason::ObjectOrListVal));
                runner.stack.push(DeserializeTask::Value);
                wip = wip.begin_list_item().unwrap();
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
