#![warn(missing_docs)]
#![forbid(unsafe_code)]
#![doc = include_str!("../README.md")]

use std::io::Write;

use facet_core::{
    Def, Facet, NumberBits, ScalarAffinity, SequenceType, ShapeAttribute, Signedness, StructKind,
    Type, UserType,
};
use facet_reflect::{HasFields, HeapValue, Peek, ScalarType, Wip};

/// Errors when serializing to XDR bytes
#[derive(Debug)]
pub enum XdrSerError {
    /// IO error
    Io(std::io::Error),
    /// Too many bytes for field
    TooManyBytes,
    /// Enum variant discriminant too large
    TooManyVariants,
}

impl core::fmt::Display for XdrSerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            XdrSerError::Io(error) => write!(f, "IO error: {}", error),
            XdrSerError::TooManyBytes => write!(f, "Too many bytes for field"),
            XdrSerError::TooManyVariants => write!(f, "Enum variant discriminant too large"),
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
    serializer.serialize_iterative(peek)?;
    Ok(buffer)
}

#[derive(Debug)]
enum SerializeTask<'mem, 'facet> {
    Value(Peek<'mem, 'facet>),
}

struct XdrSerializer<'w, W: Write> {
    writer: &'w mut W,
}

impl<W: Write> XdrSerializer<'_, W> {
    fn serialize_u32(&mut self, value: u32) -> Result<(), XdrSerError> {
        self.writer
            .write_all(&value.to_be_bytes())
            .map_err(XdrSerError::Io)
    }

    fn serialize_u64(&mut self, value: u64) -> Result<(), XdrSerError> {
        self.writer
            .write_all(&value.to_be_bytes())
            .map_err(XdrSerError::Io)
    }

    fn serialize_i32(&mut self, value: i32) -> Result<(), XdrSerError> {
        self.writer
            .write_all(&value.to_be_bytes())
            .map_err(XdrSerError::Io)
    }

    fn serialize_i64(&mut self, value: i64) -> Result<(), XdrSerError> {
        self.writer
            .write_all(&value.to_be_bytes())
            .map_err(XdrSerError::Io)
    }

    fn serialize_f32(&mut self, value: f32) -> Result<(), XdrSerError> {
        self.writer
            .write_all(&value.to_be_bytes())
            .map_err(XdrSerError::Io)
    }

    fn serialize_f64(&mut self, value: f64) -> Result<(), XdrSerError> {
        self.writer
            .write_all(&value.to_be_bytes())
            .map_err(XdrSerError::Io)
    }

    fn serialize_bool(&mut self, value: bool) -> Result<(), XdrSerError> {
        if value {
            self.writer.write_all(&1u32.to_be_bytes())
        } else {
            self.writer.write_all(&0u32.to_be_bytes())
        }
        .map_err(XdrSerError::Io)
    }

    fn serialize_char(&mut self, value: char) -> Result<(), XdrSerError> {
        self.serialize_u32(value as u32)
    }

    fn serialize_str(&mut self, value: &str) -> Result<(), XdrSerError> {
        let bytes = value.as_bytes();
        self.serialize_bytes(bytes)
    }

    fn serialize_bytes(&mut self, value: &[u8]) -> Result<(), XdrSerError> {
        if value.len() > u32::MAX as usize {
            return Err(XdrSerError::TooManyBytes);
        }
        let len = value.len() as u32;
        self.writer
            .write_all(&len.to_be_bytes())
            .map_err(XdrSerError::Io)?;
        let pad_len = value.len() % 4;
        self.writer.write_all(value).map_err(XdrSerError::Io)?;
        if pad_len != 0 {
            let pad = vec![0u8; 4 - pad_len];
            self.writer.write_all(&pad).map_err(XdrSerError::Io)?;
        }
        Ok(())
    }

    fn start_enum_variant(&mut self, discriminant: u64) -> Result<(), XdrSerError> {
        if discriminant > u32::MAX as u64 {
            return Err(XdrSerError::TooManyVariants);
        }
        self.writer
            .write_all(&(discriminant as u32).to_be_bytes())
            .map_err(XdrSerError::Io)
    }

    fn start_array(&mut self, len: Option<usize>) -> Result<(), XdrSerError> {
        if let Some(len) = len {
            if len > u32::MAX as usize {
                return Err(XdrSerError::TooManyBytes);
            }
            self.writer
                .write_all(&(len as u32).to_be_bytes())
                .map_err(XdrSerError::Io)
        } else {
            panic!("array length missing");
        }
    }

    pub fn serialize_iterative(&mut self, peek: Peek<'_, '_>) -> Result<(), XdrSerError> {
        let mut stack = Vec::new();
        stack.push(SerializeTask::Value(peek));
        while let Some(task) = stack.pop() {
            match task {
                SerializeTask::Value(mut peek) => {
                    if peek
                        .shape()
                        .attributes
                        .iter()
                        .any(|attr| *attr == ShapeAttribute::Transparent)
                    {
                        let ps = peek.into_struct().unwrap();
                        peek = ps.field(0).unwrap();
                    }
                    match (peek.shape().def, peek.shape().ty) {
                        (Def::Scalar(_), _) => {
                            let peek = peek.innermost_peek();

                            match peek.scalar_type() {
                                Some(ScalarType::Unit) => {}
                                Some(ScalarType::Bool) => {
                                    self.serialize_bool(*peek.get::<bool>().unwrap())?
                                }
                                Some(ScalarType::Char) => {
                                    self.serialize_char(*peek.get::<char>().unwrap())?
                                }
                                Some(ScalarType::Str) => {
                                    self.serialize_str(peek.get::<&str>().unwrap())?
                                }
                                Some(ScalarType::String) => {
                                    self.serialize_str(peek.get::<String>().unwrap())?
                                }
                                Some(ScalarType::CowStr) => self.serialize_str(
                                    peek.get::<std::borrow::Cow<'_, str>>().unwrap().as_ref(),
                                )?,
                                Some(ScalarType::F32) => {
                                    self.serialize_f32(*peek.get::<f32>().unwrap())?
                                }
                                Some(ScalarType::F64) => {
                                    self.serialize_f64(*peek.get::<f64>().unwrap())?
                                }
                                Some(ScalarType::U8) => {
                                    self.serialize_u32(*peek.get::<u8>().unwrap() as u32)?
                                }
                                Some(ScalarType::U16) => {
                                    self.serialize_u32(*peek.get::<u16>().unwrap() as u32)?
                                }
                                Some(ScalarType::U32) => {
                                    self.serialize_u32(*peek.get::<u32>().unwrap())?
                                }
                                Some(ScalarType::U64) => {
                                    self.serialize_u64(*peek.get::<u64>().unwrap())?
                                }
                                Some(ScalarType::I8) => {
                                    self.serialize_i32(*peek.get::<i8>().unwrap() as i32)?
                                }
                                Some(ScalarType::I16) => {
                                    self.serialize_i32(*peek.get::<i16>().unwrap() as i32)?
                                }
                                Some(ScalarType::I32) => {
                                    self.serialize_i32(*peek.get::<i32>().unwrap())?
                                }
                                Some(ScalarType::I64) => {
                                    self.serialize_i64(*peek.get::<i64>().unwrap())?
                                }
                                Some(_) | None => {}
                            }
                        }
                        (Def::List(ld), _) => {
                            if ld.t().is_type::<u8>() {
                                self.serialize_bytes(peek.get::<Vec<u8>>().unwrap())?
                            } else {
                                let peek_list = peek.into_list_like().unwrap();
                                let len = peek_list.len();
                                self.start_array(Some(len))?;
                                let mut array_stack = Vec::new();
                                for item_peek in peek_list.iter() {
                                    array_stack.push(SerializeTask::Value(item_peek));
                                }
                                for task in array_stack.into_iter().rev() {
                                    stack.push(task);
                                }
                            }
                        }
                        (Def::Slice(sd), _) => {
                            if sd.t.is_type::<u8>() {
                                self.serialize_bytes(peek.get::<&[u8]>().unwrap())?
                            } else {
                                let peek_list = peek.into_list_like().unwrap();
                                let len = peek_list.len();
                                self.start_array(Some(len))?;
                                let mut array_stack = Vec::new();
                                for item_peek in peek_list.iter() {
                                    array_stack.push(SerializeTask::Value(item_peek));
                                }
                                for task in array_stack.into_iter().rev() {
                                    stack.push(task);
                                }
                            }
                        }
                        (Def::Array(ad), _) => {
                            if ad.t().is_type::<u8>() {
                                let bytes: Vec<u8> = peek
                                    .into_list_like()
                                    .unwrap()
                                    .iter()
                                    .map(|p| *p.get::<u8>().unwrap())
                                    .collect();
                                let pad_len = bytes.len() % 4;
                                self.writer.write_all(&bytes).map_err(XdrSerError::Io)?;
                                if pad_len != 0 {
                                    let pad = vec![0u8; 4 - pad_len];
                                    self.writer.write_all(&pad).map_err(XdrSerError::Io)?;
                                }
                            } else {
                                let peek_list = peek.into_list_like().unwrap();
                                let mut array_stack = Vec::new();
                                for item_peek in peek_list.iter() {
                                    array_stack.push(SerializeTask::Value(item_peek));
                                }
                                for task in array_stack.into_iter().rev() {
                                    stack.push(task);
                                }
                            }
                        }
                        (Def::Option(_), _) => {
                            let opt = peek.into_option().unwrap();
                            if let Some(inner_peek) = opt.value() {
                                self.start_enum_variant(1)?;
                                stack.push(SerializeTask::Value(inner_peek));
                            } else {
                                self.start_enum_variant(0)?;
                            }
                        }
                        (_, Type::User(ut)) => match ut {
                            UserType::Struct(struct_type) => {
                                let peek = peek.into_struct().unwrap();
                                match struct_type.kind {
                                    StructKind::TupleStruct | StructKind::Tuple => {
                                        for (_, field_peek) in peek.fields_for_serialize().rev() {
                                            stack.push(SerializeTask::Value(field_peek));
                                        }
                                    }
                                    StructKind::Struct => {
                                        for (_, field_peek) in peek.fields_for_serialize().rev() {
                                            stack.push(SerializeTask::Value(field_peek));
                                        }
                                    }
                                    _ => {}
                                }
                            }
                            UserType::Enum(_) => {
                                let peek = peek.into_enum().unwrap();
                                let variant = peek.active_variant().unwrap();
                                let variant_index = peek.variant_index().unwrap();
                                let discriminant = variant
                                    .discriminant
                                    .map(|d| d as u64)
                                    .unwrap_or(variant_index as u64);
                                self.start_enum_variant(discriminant)?;
                                for (_, field_peek) in peek.fields_for_serialize() {
                                    stack.push(SerializeTask::Value(field_peek));
                                }
                            }
                            _ => {}
                        },
                        (_, Type::Sequence(SequenceType::Tuple(_))) => {
                            let peek = peek.into_tuple().unwrap();
                            let mut array_stack = Vec::new();
                            for (_, item_peek) in peek.fields() {
                                array_stack.push(SerializeTask::Value(item_peek));
                            }
                            for task in array_stack.into_iter().rev() {
                                stack.push(task);
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        Ok(())
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
                write!(f, "Invalid boolean at byte {}", position)
            }
            XdrDeserError::InvalidOptional { position } => {
                write!(f, "Invalid discriminant for optional at byte {}", position)
            }
            XdrDeserError::InvalidVariant { position } => {
                write!(f, "Invalid enum discriminant at byte {}", position)
            }
            XdrDeserError::InvalidString { position, .. } => {
                write!(f, "Invalid string at byte {}", position)
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

struct XdrDeserializerStack<'r> {
    input: &'r [u8],
    pos: usize,
    stack: Vec<DeserializeTask>,
}

impl<'r> XdrDeserializerStack<'r> {
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

    fn next_data(&mut self, expected_len: Option<u32>) -> Result<&'r [u8], XdrDeserError> {
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

    fn next<'f>(&mut self, wip: Wip<'f>) -> Result<Wip<'f>, XdrDeserError> {
        match (wip.shape().def, wip.shape().ty) {
            (Def::Scalar(sd), _) => match sd.affinity {
                ScalarAffinity::Number(na) => match na.bits {
                    NumberBits::Integer { bits, sign } => match (bits, sign) {
                        (8, Signedness::Unsigned) => {
                            let value = self.next_u32()? as u8;
                            Ok(wip.put(value).unwrap())
                        }
                        (16, Signedness::Unsigned) => {
                            let value = self.next_u32()? as u16;
                            Ok(wip.put(value).unwrap())
                        }
                        (32, Signedness::Unsigned) => {
                            let value = self.next_u32()?;
                            Ok(wip.put(value).unwrap())
                        }
                        (64, Signedness::Unsigned) => {
                            let value = self.next_u64()?;
                            Ok(wip.put(value).unwrap())
                        }
                        (8, Signedness::Signed) => {
                            let value = self.next_u32()? as i8;
                            Ok(wip.put(value).unwrap())
                        }
                        (16, Signedness::Signed) => {
                            let value = self.next_u32()? as i16;
                            Ok(wip.put(value).unwrap())
                        }
                        (32, Signedness::Signed) => {
                            let value = self.next_u32()? as i32;
                            Ok(wip.put(value).unwrap())
                        }
                        (64, Signedness::Signed) => {
                            let value = self.next_u64()? as i64;
                            Ok(wip.put(value).unwrap())
                        }
                        _ => Err(XdrDeserError::UnsupportedNumericType),
                    },
                    NumberBits::Float {
                        sign_bits,
                        exponent_bits,
                        mantissa_bits,
                        ..
                    } => {
                        let bits = sign_bits + exponent_bits + mantissa_bits;
                        if bits == 32 {
                            let bits = self.next_u32()?;
                            let float = f32::from_bits(bits);
                            Ok(wip.put(float).unwrap())
                        } else if bits == 64 {
                            let bits = self.next_u64()?;
                            let float = f64::from_bits(bits);
                            Ok(wip.put(float).unwrap())
                        } else {
                            Err(XdrDeserError::UnsupportedNumericType)
                        }
                    }
                    _ => Err(XdrDeserError::UnsupportedNumericType),
                },
                ScalarAffinity::String(_) => {
                    let string = core::str::from_utf8(self.next_data(None)?).map_err(|e| {
                        XdrDeserError::InvalidString {
                            position: self.pos - 1,
                            source: e,
                        }
                    })?;
                    Ok(wip.put(string.to_owned()).unwrap())
                }
                ScalarAffinity::Boolean(_) => match self.next_u32()? {
                    0 => Ok(wip.put(false).unwrap()),
                    1 => Ok(wip.put(true).unwrap()),
                    _ => Err(XdrDeserError::InvalidBoolean {
                        position: self.pos - 4,
                    }),
                },
                ScalarAffinity::Char(_) => {
                    let value = self.next_u32()?;
                    Ok(wip.put(char::from_u32(value).unwrap()).unwrap())
                }
                _ => Err(XdrDeserError::UnsupportedType),
            },
            (Def::List(ld), _) => {
                if ld.t().is_type::<u8>() {
                    let data = self.next_data(None)?;
                    Ok(wip.put(data.to_vec()).unwrap())
                } else {
                    let len = self.next_u32()?;
                    if len == 0 {
                        Ok(wip.put_empty_list().unwrap())
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
                    let mut wip = wip;
                    self.pos += len;
                    let pad_len = len % 4;
                    for byte in &self.input[self.pos - len..self.pos] {
                        wip = wip.push().unwrap().put(*byte).unwrap().pop().unwrap();
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
                    Ok(wip.put(data.to_vec()).unwrap())
                } else {
                    let len = self.next_u32()?;
                    for _ in 0..len {
                        self.stack.push(DeserializeTask::ListItem);
                    }
                    Ok(wip)
                }
            }
            (Def::Option(_), _) => match self.next_u32()? {
                0 => Ok(wip.put_default().unwrap()),
                1 => {
                    self.stack.push(DeserializeTask::Pop(PopReason::Some));
                    self.stack.push(DeserializeTask::Value);
                    Ok(wip.push_some().unwrap())
                }
                _ => Err(XdrDeserError::InvalidOptional {
                    position: self.pos - 4,
                }),
            },
            (_, Type::User(ut)) => match ut {
                UserType::Struct(st) => {
                    for (index, _field) in st.fields.iter().enumerate().rev() {
                        if !wip.is_field_set(index).unwrap() {
                            self.stack.push(DeserializeTask::Field(index));
                        }
                    }
                    Ok(wip)
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
                        Ok(wip.variant(discriminant as usize).unwrap())
                    } else {
                        Err(XdrDeserError::InvalidVariant {
                            position: self.pos - 4,
                        })
                    }
                }
                _ => Err(XdrDeserError::UnsupportedType),
            },
            (_, Type::Sequence(SequenceType::Tuple(tt))) => {
                for _field in tt.fields.iter() {
                    self.stack.push(DeserializeTask::ListItem);
                }
                Ok(wip)
            }
            _ => Err(XdrDeserError::UnsupportedType),
        }
    }
}

/// Deserialize an XDR slice given some some [`Wip`] into a [`HeapValue`]
pub fn deserialize_wip<'f>(input: &[u8], mut wip: Wip<'f>) -> Result<HeapValue<'f>, XdrDeserError> {
    let mut runner = XdrDeserializerStack {
        input,
        pos: 0,
        stack: vec![
            DeserializeTask::Pop(PopReason::TopLevel),
            DeserializeTask::Value,
        ],
    };

    loop {
        let frame_count = wip.frames_count();
        debug_assert!(
            frame_count
                >= runner
                    .stack
                    .iter()
                    .filter(|f| matches!(f, DeserializeTask::Pop(_)))
                    .count()
        );

        match runner.stack.pop() {
            Some(DeserializeTask::Pop(reason)) => {
                if reason == PopReason::TopLevel {
                    return Ok(wip.build().unwrap());
                } else {
                    wip = wip.pop().unwrap();
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
                wip = wip.field(index).unwrap();
            }
            Some(DeserializeTask::ListItem) => {
                runner
                    .stack
                    .push(DeserializeTask::Pop(PopReason::ObjectOrListVal));
                runner.stack.push(DeserializeTask::Value);
                wip = wip.push().unwrap();
            }
            None => unreachable!("Instruction stack is empty"),
        }
    }
}

/// Deserialize a slice of XDR bytes into any Facet type
pub fn deserialize<'f, F: facet_core::Facet<'f>>(input: &[u8]) -> Result<F, XdrDeserError> {
    let v = deserialize_wip(input, Wip::alloc_shape(F::SHAPE).unwrap())?;
    let f: F = v.materialize().unwrap();
    Ok(f)
}
