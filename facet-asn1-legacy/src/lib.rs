#![no_std]
#![warn(missing_docs)]
#![forbid(unsafe_code)]
#![doc = include_str!("../README.md")]

#[macro_use]
extern crate alloc;

use alloc::{borrow::ToOwned, vec::Vec};
use core::{f64, num::FpCategory, str::FromStr};

use facet_core::{
    Def, Facet, NumericType, PrimitiveType, Shape, StructKind, TextualType, Type, UserType,
};
use facet_reflect::{HasFields, HeapValue, Partial, Peek};

mod tag;

const ASN1_TYPE_TAG_BOOLEAN: u8 = 0x01;
const ASN1_TYPE_TAG_INTEGER: u8 = 0x02;
const ASN1_TYPE_TAG_OCTET_STRING: u8 = 0x04;
const ASN1_TYPE_TAG_NULL: u8 = 0x05;
const ASN1_TYPE_TAG_REAL: u8 = 0x09;
const ASN1_TYPE_TAG_UTF8STRING: u8 = 0x0C;
const ASN1_TYPE_TAG_SEQUENCE: u8 = 0x10;

const ASN1_FORM_CONSTRUCTED: u8 = 0b1 << 5;

const ASN1_REAL_INFINITY: u8 = 0b01000000;
const ASN1_REAL_NEG_INFINITY: u8 = 0b01000001;
const ASN1_REAL_NAN: u8 = 0b01000010;
const ASN1_REAL_NEG_ZERO: u8 = 0b01000011;

const F64_MANTISSA_MASK: u64 = 0b1111111111111111111111111111111111111111111111111111;

/// `no_std` compatible Write trait used by the ASN.1 serializer.
pub trait Asn1Write {
    /// Write all these bytes to the writer.
    fn write(&mut self, buf: &[u8]);

    /// If the writer supports it, reserve space for `len` additional bytes.
    fn reserve(&mut self, additional: usize);
}

impl Asn1Write for &mut Vec<u8> {
    fn write(&mut self, buf: &[u8]) {
        self.extend(buf)
    }

    fn reserve(&mut self, additional: usize) {
        Vec::reserve(self, additional)
    }
}

impl Asn1Write for Vec<u8> {
    fn write(&mut self, buf: &[u8]) {
        self.extend(buf)
    }

    fn reserve(&mut self, additional: usize) {
        Vec::reserve(self, additional)
    }
}

enum Asn1TagForShapeError {
    TypeTag(tag::Asn1TagError),
    UnsupportedShape,
}

/// Get the BER/DER tag for a given shape
///
/// Returns `None` for CHOICE/Enum
fn ber_tag_for_shape(shape: &Shape) -> Result<Option<u8>, Asn1TagForShapeError> {
    let type_tag = shape
        .type_tag
        .map(|t| tag::Asn1TypeTag::from_str(t).map_err(Asn1TagForShapeError::TypeTag))
        .transpose()?
        .map(|t| t.ber());
    match (shape.def, shape.ty) {
        (Def::Scalar, Type::Primitive(pt)) => match pt {
            PrimitiveType::Boolean => Ok(Some(type_tag.unwrap_or(ASN1_TYPE_TAG_BOOLEAN))),
            PrimitiveType::Numeric(nt) => match nt {
                NumericType::Integer { .. } => Ok(Some(type_tag.unwrap_or(ASN1_TYPE_TAG_INTEGER))),
                NumericType::Float => Ok(Some(type_tag.unwrap_or(ASN1_TYPE_TAG_REAL))),
            },
            PrimitiveType::Textual(TextualType::Str) => {
                Ok(Some(type_tag.unwrap_or(ASN1_TYPE_TAG_UTF8STRING)))
            }
            _ => Err(Asn1TagForShapeError::UnsupportedShape),
        },
        (Def::Scalar, Type::User(UserType::Opaque)) => {
            Ok(Some(type_tag.unwrap_or(ASN1_TYPE_TAG_UTF8STRING)))
        }
        (Def::List(ld), _) => {
            if ld.t().is_type::<u8>() && shape.is_type::<Vec<u8>>() {
                Ok(Some(type_tag.unwrap_or(ASN1_TYPE_TAG_OCTET_STRING)))
            } else {
                Ok(Some(
                    type_tag.unwrap_or(ASN1_TYPE_TAG_SEQUENCE) | ASN1_FORM_CONSTRUCTED,
                ))
            }
        }
        (Def::Option(od), _) => Ok(type_tag.or(ber_tag_for_shape(od.t)?)),
        (_, Type::User(ut)) => match ut {
            UserType::Struct(st) => match st.kind {
                StructKind::Unit => Ok(Some(type_tag.unwrap_or(ASN1_TYPE_TAG_NULL))),
                StructKind::TupleStruct if st.fields.len() == 1 && shape.is_transparent() => {
                    Ok(type_tag.or(ber_tag_for_shape(st.fields[0].shape())?))
                }
                StructKind::TupleStruct | StructKind::Struct | StructKind::Tuple => Ok(Some(
                    type_tag.unwrap_or(ASN1_TYPE_TAG_SEQUENCE) | ASN1_FORM_CONSTRUCTED,
                )),
            },
            UserType::Enum(_) => {
                // Enum variants are matched against their discriminant or inner types
                Ok(None)
            }
            _ => Err(Asn1TagForShapeError::UnsupportedShape),
        },
        _ => Err(Asn1TagForShapeError::UnsupportedShape),
    }
}

#[derive(Debug)]
#[non_exhaustive]
/// Errors when serializing to an ASN.1 format
pub enum Asn1SerError {
    /// Invalid type tag
    TypeTag(tag::Asn1TagError),
    /// Unsupported shape
    UnsupportedShape,
    /// Enum unit variant discriminant too large
    InvalidDiscriminant(Option<i64>),
}

impl core::fmt::Display for Asn1SerError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Asn1SerError::TypeTag(error) => write!(f, "Bad type_tag: {error}"),
            Asn1SerError::UnsupportedShape => write!(f, "Unsupported shape"),
            Asn1SerError::InvalidDiscriminant(d) => {
                if let Some(d) = d {
                    write!(f, "Enum variant discriminant invalid: {d}")
                } else {
                    write!(f, "Enum variant discriminant invalid")
                }
            }
        }
    }
}

impl core::error::Error for Asn1SerError {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            Asn1SerError::TypeTag(error) => Some(error),
            _ => None,
        }
    }
}

impl From<Asn1TagForShapeError> for Asn1SerError {
    fn from(value: Asn1TagForShapeError) -> Self {
        match value {
            Asn1TagForShapeError::TypeTag(t) => Asn1SerError::TypeTag(t),
            Asn1TagForShapeError::UnsupportedShape => Asn1SerError::UnsupportedShape,
        }
    }
}

/// Serialize a Facet type to ASN.1 DER bytes
pub fn to_vec_der<'f, F: Facet<'f>>(value: &'f F) -> Result<Vec<u8>, Asn1SerError> {
    let mut buffer = Vec::new();
    let peek = Peek::new(value);
    let mut serializer = DerSerializer {
        writer: &mut buffer,
    };
    serialize_der_recursive(peek, &mut serializer, None)?;
    Ok(buffer)
}

struct DerSerializer<'w, W: Asn1Write> {
    writer: &'w mut W,
}

impl<'w, W: Asn1Write> DerSerializer<'w, W> {
    fn serialize_tlv(&mut self, tag: u8, value: &[u8]) {
        if value.len() < 128 {
            self.writer.write(&[tag, value.len() as u8]);
        } else {
            let len_bytes_len = core::cmp::max(value.len() / 256, 1);
            let len_bytes = value.len().to_be_bytes();
            self.writer.write(&[tag, len_bytes_len as u8]);
            self.writer
                .write(&len_bytes[len_bytes.len() - len_bytes_len..]);
        }
        self.writer.write(value);
    }

    fn serialize_i64(&mut self, tag: u8, value: i64) {
        let bytes = value.to_be_bytes();
        let mut leading_zeroes = 0;
        for window in bytes.windows(2) {
            let byte = window[0] as i8;
            let bit = window[1] as i8 >> 7;
            if byte ^ bit == 0 {
                leading_zeroes += 1;
            } else {
                break;
            }
        }
        self.serialize_tlv(tag, &bytes[leading_zeroes..])
    }

    fn serialize_f64(&mut self, tag: u8, value: f64) {
        match value.classify() {
            FpCategory::Nan => self.serialize_tlv(tag, &[ASN1_REAL_NAN]),
            FpCategory::Infinite => {
                if value.is_sign_positive() {
                    self.serialize_tlv(tag, &[ASN1_REAL_INFINITY])
                } else {
                    self.serialize_tlv(tag, &[ASN1_REAL_NEG_INFINITY])
                }
            }
            FpCategory::Zero | FpCategory::Subnormal => {
                // Subnormals cannot be represented in DER and are rounded to zero
                if value.is_sign_positive() {
                    self.serialize_unit(tag)
                } else {
                    self.serialize_tlv(tag, &[ASN1_REAL_NEG_ZERO])
                }
            }
            FpCategory::Normal => {
                let sign_negative = value.is_sign_negative();
                let bits = value.to_bits();
                // The exponent is always 11 bits in f64, so we can fit it inside an i16.
                let mut exponent = ((bits >> 52) & 0b11111111111) as i16 - 1023;
                let mut mantissa = bits & F64_MANTISSA_MASK | (0b1 << 52);
                let mut normalization_factor = 52;
                while mantissa & 0b1 == 0 {
                    mantissa >>= 1;
                    normalization_factor -= 1;
                }
                exponent -= normalization_factor;
                let mantissa_bytes = mantissa.to_be_bytes();
                let mut leading_zero_bytes = 0;
                for byte in mantissa_bytes {
                    if byte == 0 {
                        leading_zero_bytes += 1;
                    } else {
                        break;
                    }
                }
                let exponent_bytes = exponent.to_be_bytes();
                // If the exponent can be represented as an i8, then we must do so.
                let short_exp = exponent_bytes[0] == 0 || exponent_bytes[0] == 0xFF;
                let len = 2 + (!short_exp as usize) + mantissa_bytes.len() - leading_zero_bytes;
                // This identifying byte contains the encoding method, as well as the sign and
                // exponent length.
                let structure_byte = 0b10000000 | ((sign_negative as u8) << 6) | (!short_exp as u8);
                self.writer.write(&[tag, len as u8, structure_byte]);
                if short_exp {
                    self.writer.write(&[exponent_bytes[1]]);
                } else {
                    self.writer.write(&exponent_bytes);
                }
                self.writer.write(&mantissa_bytes[leading_zero_bytes..]);
            }
        }
    }

    fn serialize_bool(&mut self, tag: u8, value: bool) {
        let byte = if value { 0xFF } else { 0x00 };
        self.serialize_tlv(tag, &[byte])
    }

    fn serialize_str(&mut self, tag: u8, value: &str) {
        self.serialize_tlv(tag, value.as_bytes())
    }

    fn serialize_unit(&mut self, tag: u8) {
        self.serialize_tlv(tag, &[])
    }
}

fn serialize_der_recursive<'w, W: Asn1Write>(
    pv: Peek<'_, '_>,
    serializer: &'w mut DerSerializer<'w, W>,
    wrapper_tag: Option<u8>,
) -> Result<(), Asn1SerError> {
    let shape = pv.shape();
    let tag = wrapper_tag.or(ber_tag_for_shape(shape)?);
    match (shape.def, shape.ty) {
        (Def::Scalar, Type::Primitive(pt)) => match pt {
            PrimitiveType::Boolean => {
                serializer.serialize_bool(tag.unwrap(), *pv.get::<bool>().unwrap());
                Ok(())
            }
            PrimitiveType::Numeric(nt) => match nt {
                NumericType::Integer { .. } => {
                    let value = if shape.is_type::<i8>() {
                        *pv.get::<i8>().unwrap() as i64
                    } else if shape.is_type::<i16>() {
                        *pv.get::<i16>().unwrap() as i64
                    } else if shape.is_type::<i32>() {
                        *pv.get::<i32>().unwrap() as i64
                    } else if shape.is_type::<i64>() {
                        *pv.get::<i64>().unwrap()
                    } else {
                        return Err(Asn1SerError::UnsupportedShape);
                    };
                    serializer.serialize_i64(tag.unwrap(), value);
                    Ok(())
                }
                NumericType::Float => {
                    let value = if shape.is_type::<f32>() {
                        *pv.get::<f32>().unwrap() as f64
                    } else if shape.is_type::<f64>() {
                        *pv.get::<f64>().unwrap()
                    } else {
                        return Err(Asn1SerError::UnsupportedShape);
                    };
                    serializer.serialize_f64(tag.unwrap(), value);
                    Ok(())
                }
            },
            PrimitiveType::Textual(TextualType::Str) => {
                let value = pv.get::<&str>().unwrap();
                serializer.serialize_str(tag.unwrap(), value);
                Ok(())
            }
            _ => Err(Asn1SerError::UnsupportedShape),
        },
        (Def::Scalar, Type::User(UserType::Opaque)) => {
            if shape.vtable.has_display() {
                serializer.serialize_str(tag.unwrap(), &alloc::format!("{pv}"));
                Ok(())
            } else {
                Err(Asn1SerError::UnsupportedShape)
            }
        }
        (Def::List(ld), _) => {
            if ld.t().is_type::<u8>() && shape.is_type::<Vec<u8>>() {
                serializer.serialize_tlv(tag.unwrap(), pv.get::<Vec<u8>>().unwrap());
            } else {
                let pv = pv.into_list().unwrap();
                let mut value = Vec::new();
                for pv in pv.iter() {
                    let mut inner_serializer = DerSerializer { writer: &mut value };
                    serialize_der_recursive(pv, &mut inner_serializer, None)?;
                }
                serializer.serialize_tlv(tag.unwrap(), &value);
            }
            Ok(())
        }
        (Def::Option(_), _) => {
            let pv = pv.into_option().unwrap();
            if let Some(pv) = pv.value() {
                serialize_der_recursive(pv, serializer, tag)?;
            }
            Ok(())
        }
        (_, Type::User(ut)) => match ut {
            UserType::Struct(st) => match st.kind {
                StructKind::Unit => {
                    serializer.serialize_unit(tag.unwrap());
                    Ok(())
                }
                StructKind::TupleStruct if st.fields.len() == 1 && shape.is_transparent() => {
                    let inner = pv.into_struct().unwrap().field(0).unwrap();
                    serialize_der_recursive(inner, serializer, tag)
                }
                StructKind::TupleStruct | StructKind::Struct | StructKind::Tuple => {
                    let pv = pv.into_struct().unwrap();
                    let mut value = Vec::new();
                    for (_, pv) in pv.fields() {
                        let mut inner_serializer = DerSerializer { writer: &mut value };
                        serialize_der_recursive(pv, &mut inner_serializer, None)?;
                    }
                    serializer.serialize_tlv(tag.unwrap(), &value);
                    Ok(())
                }
            },
            UserType::Enum(_) => {
                let pv = pv.into_enum().unwrap();
                let v = pv.active_variant().unwrap();
                let discriminant = v.discriminant;
                match v.data.kind {
                    StructKind::Unit => {
                        if discriminant
                            .is_some_and(|discriminant| !(0..128).contains(&discriminant))
                        {
                            return Err(Asn1SerError::InvalidDiscriminant(discriminant));
                        }
                        let tag = (discriminant.unwrap_or(ASN1_TYPE_TAG_NULL as i64) as u8)
                            | tag::ASN1_CLASS_CONTEXT_SPECIFIC;
                        serializer.serialize_unit(tag);
                        Ok(())
                    }
                    StructKind::TupleStruct if pv.fields().count() == 1 => {
                        let inner = pv.innermost_peek();
                        serialize_der_recursive(inner, serializer, None)
                    }
                    StructKind::TupleStruct | StructKind::Struct | StructKind::Tuple => {
                        if discriminant
                            .is_some_and(|discriminant| !(0..128).contains(&discriminant))
                        {
                            return Err(Asn1SerError::InvalidDiscriminant(discriminant));
                        }
                        let tag = (discriminant
                            .unwrap_or((ASN1_TYPE_TAG_SEQUENCE | ASN1_FORM_CONSTRUCTED) as i64)
                            as u8)
                            | tag::ASN1_CLASS_CONTEXT_SPECIFIC;
                        let mut value = Vec::new();
                        for (_, pv) in pv.fields() {
                            let mut inner_serializer = DerSerializer { writer: &mut value };
                            serialize_der_recursive(pv, &mut inner_serializer, None)?;
                        }
                        serializer.serialize_tlv(tag, &value);
                        Ok(())
                    }
                }
            }
            _ => Err(Asn1SerError::UnsupportedShape),
        },
        _ => Err(Asn1SerError::UnsupportedShape),
    }
}

/// Errors when deserializing from ASN.1 BER or DER bytes
#[derive(Debug)]
pub enum Asn1DeserError {
    /// Invalid type tag
    TypeTag(tag::Asn1TagError),
    /// Unsupported shape
    UnsupportedShape,
    /// Tag couldn't be matched to a struct, field, or enum variant
    UnknownTag {
        /// Tag value
        tag: u8,
        /// Position of this error in bytes
        position: usize,
    },
    /// Unexpected length for type
    LengthMismatch {
        /// Length value
        len: usize,
        /// Expected length value
        expected_len: usize,
        /// Position of this error in bytes
        position: usize,
    },
    /// Invalid boolean
    InvalidBool {
        /// Position of this error in bytes
        position: usize,
    },
    /// Invalid real
    InvalidReal {
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
    /// Sequence length didn't match content length
    SequenceSizeMismatch {
        /// Position of the end of the sequence in bytes
        sequence_end: usize,
        /// Position of the end of the sequence content
        content_end: usize,
    },
}

impl core::fmt::Display for Asn1DeserError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Asn1DeserError::TypeTag(error) => write!(f, "Bad type_tag: {error}"),
            Asn1DeserError::UnsupportedShape => write!(f, "Unsupported shape"),
            Asn1DeserError::UnknownTag { tag, position } => {
                write!(f, "Unknown tag {tag} at byte {position}")
            }
            Asn1DeserError::LengthMismatch {
                len,
                expected_len,
                position,
            } => {
                write!(
                    f,
                    "Unexpected length {len} for type at byte {position}, expected {expected_len}",
                )
            }
            Asn1DeserError::InvalidBool { position } => {
                write!(f, "Invalid value for boolean at byte {position}")
            }
            Asn1DeserError::InvalidReal { position } => {
                write!(f, "Invalid value for real at byte {position}")
            }
            Asn1DeserError::InvalidString { position, .. } => {
                write!(f, "Invalid string at byte {position}")
            }
            Asn1DeserError::SequenceSizeMismatch {
                sequence_end,
                content_end,
            } => {
                write!(
                    f,
                    "Sequence ending at byte {sequence_end} didn't match content ending at byte {content_end}"
                )
            }
        }
    }
}

impl core::error::Error for Asn1DeserError {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            Asn1DeserError::TypeTag(source) => Some(source),
            Asn1DeserError::InvalidString { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl From<Asn1TagForShapeError> for Asn1DeserError {
    fn from(value: Asn1TagForShapeError) -> Self {
        match value {
            Asn1TagForShapeError::TypeTag(t) => Asn1DeserError::TypeTag(t),
            Asn1TagForShapeError::UnsupportedShape => Asn1DeserError::UnsupportedShape,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum EncodingRules {
    // Basic,
    // Canonical,
    Distinguished,
}

#[derive(Debug, PartialEq)]
enum PopReason {
    TopLevel,
    ObjectVal,
    ListVal { end: usize },
    Some,
    Object { end: usize },
}

#[derive(Debug)]
enum DeserializeTask {
    Value { with_tag: Option<u8> },
    Field(usize),
    Pop(PopReason),
}

struct Asn1DeserializerStack<'input> {
    _rules: EncodingRules,
    input: &'input [u8],
    pos: usize,
    stack: Vec<DeserializeTask>,
}

impl<'input> Asn1DeserializerStack<'input> {
    fn next_tl(&mut self, expected_tag: u8) -> Result<usize, Asn1DeserError> {
        let tag = self.input[self.pos];
        if tag != expected_tag {
            return Err(Asn1DeserError::UnknownTag {
                tag,
                position: self.pos,
            });
        }
        let len = self.input[self.pos + 1] as usize;
        self.pos += 2;
        let len = if len < 128 {
            len
        } else {
            let len_len = len - 128;
            self.pos += len_len;
            let len_bytes = &self.input[(self.pos - len_len)..self.pos];
            len_bytes.iter().fold(0usize, |mut acc, x| {
                acc <<= 8;
                acc += *x as usize;
                acc
            })
        };
        Ok(len)
    }

    fn next_tlv(&mut self, expected_tag: u8) -> Result<&'input [u8], Asn1DeserError> {
        let len = self.next_tl(expected_tag)?;
        self.pos += len;
        Ok(&self.input[(self.pos - len)..self.pos])
    }

    fn next_bool(&mut self, tag: u8) -> Result<bool, Asn1DeserError> {
        let bytes = self.next_tlv(tag)?;
        match *bytes {
            [0x00] => Ok(false),
            [0xFF] => Ok(true),
            [_] => Err(Asn1DeserError::InvalidBool { position: self.pos }),
            _ => Err(Asn1DeserError::LengthMismatch {
                len: bytes.len(),
                expected_len: 1,
                position: self.pos - bytes.len(),
            }),
        }
    }

    fn next_int(&mut self, tag: u8) -> Result<i64, Asn1DeserError> {
        let bytes = self.next_tlv(tag)?;
        Ok(bytes[1..].iter().fold(bytes[0] as i8 as i64, |mut acc, x| {
            acc <<= 8;
            acc |= *x as i64;
            acc
        }))
    }

    fn next_float(&mut self, tag: u8) -> Result<f64, Asn1DeserError> {
        let bytes = self.next_tlv(tag)?;
        Ok(if bytes.is_empty() {
            0.0f64
        } else {
            match bytes[0] {
                ASN1_REAL_INFINITY => f64::INFINITY,
                ASN1_REAL_NEG_INFINITY => f64::NEG_INFINITY,
                ASN1_REAL_NAN => f64::NAN,
                ASN1_REAL_NEG_ZERO => -0.0f64,
                struct_byte => {
                    if struct_byte & 0b10111100 != 0b10000000 {
                        return Err(Asn1DeserError::InvalidReal {
                            position: self.pos - bytes.len(),
                        });
                    }
                    let sign_negative = (struct_byte >> 6 & 0b1) > 0;
                    let exponent_len = ((struct_byte & 0b11) + 1) as usize;
                    if bytes.len() < exponent_len + 2 {
                        return Err(Asn1DeserError::LengthMismatch {
                            len: bytes.len(),
                            expected_len: exponent_len + 2,
                            position: self.pos - bytes.len(),
                        });
                    }
                    if exponent_len > 1 && matches!(bytes[1], 0x00 | 0xFF) {
                        return Err(Asn1DeserError::InvalidReal {
                            position: self.pos - bytes.len(),
                        });
                    }
                    if bytes.len() > 2 + exponent_len
                        && matches!(bytes[1 + exponent_len], 0x00 | 0xFF)
                    {
                        return Err(Asn1DeserError::InvalidReal {
                            position: self.pos - bytes.len(),
                        });
                    }
                    let mut exponent = bytes[2..1 + exponent_len].iter().fold(
                        bytes[1] as i8 as i64,
                        |mut acc, x| {
                            acc <<= 8;
                            acc |= *x as u64 as i64;
                            acc
                        },
                    );
                    if exponent > 1023 {
                        if sign_negative {
                            f64::NEG_INFINITY
                        } else {
                            f64::INFINITY
                        }
                    } else {
                        let mut mantissa =
                            bytes[1 + exponent_len..]
                                .iter()
                                .take(7)
                                .fold(0, |mut acc, x| {
                                    acc <<= 8;
                                    acc |= *x as u64;
                                    acc
                                });
                        let mut normalization_factor = 52;
                        while mantissa & (0b1 << 52) == 0 && normalization_factor > 0 {
                            mantissa <<= 1;
                            normalization_factor -= 1;
                        }
                        exponent += normalization_factor + 1023;
                        f64::from_bits(
                            (sign_negative as u64) << 63
                                | ((exponent as u64) & 0b11111111111) << 52
                                | (mantissa & F64_MANTISSA_MASK),
                        )
                    }
                }
            }
        })
    }

    fn next_str(&mut self, tag: u8) -> Result<&str, Asn1DeserError> {
        let bytes = self.next_tlv(tag)?;
        core::str::from_utf8(bytes).map_err(|source| Asn1DeserError::InvalidString {
            position: self.pos,
            source,
        })
    }

    fn next<'f>(
        &mut self,
        wip: Partial<'f>,
        with_tag: Option<u8>,
    ) -> Result<Partial<'f>, Asn1DeserError> {
        let shape = wip.shape();
        let tag_for_shape = with_tag.or(ber_tag_for_shape(shape)?);
        match (shape.def, shape.ty) {
            (Def::Scalar, Type::Primitive(pt)) => match pt {
                PrimitiveType::Boolean => {
                    let wip = wip.set(self.next_bool(tag_for_shape.unwrap())?).unwrap();
                    Ok(wip)
                }
                PrimitiveType::Numeric(nt) => match nt {
                    NumericType::Integer { .. } => {
                        let number = self.next_int(tag_for_shape.unwrap())?;
                        let wip = if shape.is_type::<i8>() {
                            wip.set(number as i8).unwrap()
                        } else if shape.is_type::<i16>() {
                            wip.set(number as i16).unwrap()
                        } else if shape.is_type::<i32>() {
                            wip.set(number as i32).unwrap()
                        } else if shape.is_type::<i64>() {
                            wip.set(number).unwrap()
                        } else {
                            wip
                        };
                        Ok(wip)
                    }
                    NumericType::Float => {
                        let value = self.next_float(tag_for_shape.unwrap())?;
                        let wip = if shape.is_type::<f32>() {
                            wip.set(value as f32).unwrap()
                        } else if shape.is_type::<f64>() {
                            wip.set(value).unwrap()
                        } else {
                            wip
                        };
                        Ok(wip)
                    }
                },
                PrimitiveType::Textual(TextualType::Str) => {
                    let value = self.next_str(tag_for_shape.unwrap())?;
                    let wip = wip.set(value.to_owned()).unwrap();
                    Ok(wip)
                }
                _ => Err(Asn1DeserError::UnsupportedShape),
            },
            (Def::Scalar, Type::User(UserType::Opaque)) => {
                if shape.vtable.has_parse() {
                    let value = self.next_str(tag_for_shape.unwrap())?.to_owned();
                    let wip = wip.set(value).unwrap();
                    Ok(wip)
                } else {
                    Err(Asn1DeserError::UnsupportedShape)
                }
            }
            (Def::List(_), _) => {
                if shape.is_type::<Vec<u8>>() {
                    let bytes = self.next_tlv(tag_for_shape.unwrap())?;
                    let wip = wip.set(bytes.to_vec()).unwrap();
                    Ok(wip)
                } else {
                    let len = self.next_tl(tag_for_shape.unwrap())?;
                    self.stack.push(DeserializeTask::Pop(PopReason::ListVal {
                        end: self.pos + len,
                    }));
                    self.stack.push(DeserializeTask::Value { with_tag: None });
                    Ok(wip)
                }
            }
            (Def::Option(od), _) => {
                if self.pos == self.input.len() {
                    let wip = wip.set_default().unwrap();
                    return Ok(wip);
                }
                let tag = self.input[self.pos];
                let wip = match tag_for_shape {
                    Some(t) if t == tag => {
                        let wip = wip.begin_some().unwrap();
                        self.stack.push(DeserializeTask::Pop(PopReason::Some));
                        self.stack
                            .push(DeserializeTask::Value { with_tag: Some(t) });
                        wip
                    }
                    Some(_) => wip.set_default().unwrap(),
                    None => {
                        if let Type::User(UserType::Enum(et)) = od.t.ty {
                            let mut found = false;
                            for v in et.variants {
                                if let Some(variant_tag) = match v.data.kind {
                                    StructKind::Tuple if v.data.fields.len() == 1 => {
                                        ber_tag_for_shape(v.data.fields[0].shape())?
                                    }
                                    StructKind::Unit
                                    | StructKind::TupleStruct
                                    | StructKind::Struct
                                    | StructKind::Tuple => v.discriminant.map(|discriminant| {
                                        discriminant as u8 | tag::ASN1_CLASS_CONTEXT_SPECIFIC
                                    }),
                                } && tag == variant_tag
                                {
                                    found = true;
                                    break;
                                }
                            }
                            if found {
                                let wip = wip.begin_some().unwrap();
                                self.stack.push(DeserializeTask::Pop(PopReason::Some));
                                self.stack.push(DeserializeTask::Value { with_tag: None });
                                wip
                            } else {
                                wip.set_default().unwrap()
                            }
                        } else {
                            wip.set_default().unwrap()
                        }
                    }
                };
                Ok(wip)
            }
            (_, Type::User(ut)) => match ut {
                UserType::Struct(st) => match st.kind {
                    StructKind::Unit => {
                        let len = self.next_tl(tag_for_shape.unwrap())?;
                        if len != 0 {
                            Err(Asn1DeserError::LengthMismatch {
                                len,
                                expected_len: 0,
                                position: self.pos,
                            })
                        } else {
                            Ok(wip)
                        }
                    }
                    StructKind::TupleStruct if st.fields.len() == 1 && shape.is_transparent() => {
                        let wip = wip.begin_nth_field(0).unwrap();
                        self.stack.push(DeserializeTask::Pop(PopReason::ObjectVal));
                        self.stack.push(DeserializeTask::Value {
                            with_tag: tag_for_shape,
                        });
                        Ok(wip)
                    }
                    StructKind::TupleStruct | StructKind::Struct | StructKind::Tuple => {
                        let len = self.next_tl(tag_for_shape.unwrap())?;
                        self.stack.push(DeserializeTask::Pop(PopReason::Object {
                            end: self.pos + len,
                        }));
                        for i in (0..st.fields.len()).rev() {
                            self.stack.push(DeserializeTask::Field(i));
                        }
                        Ok(wip)
                    }
                },
                UserType::Enum(et) => {
                    let tag = self.input[self.pos];
                    for (i, v) in et.variants.iter().enumerate() {
                        match v.data.kind {
                            StructKind::Unit => {
                                if let Some(discriminant) = v.discriminant {
                                    let expected_tag =
                                        discriminant as u8 | tag::ASN1_CLASS_CONTEXT_SPECIFIC;
                                    if tag == expected_tag {
                                        let wip = wip.select_nth_variant(i).unwrap();
                                        let len = self.next_tl(tag)?;
                                        if len != 0 {
                                            return Err(Asn1DeserError::LengthMismatch {
                                                len,
                                                expected_len: 0,
                                                position: self.pos,
                                            });
                                        } else {
                                            return Ok(wip);
                                        }
                                    }
                                }
                            }
                            StructKind::Tuple if v.data.fields.len() == 1 => {
                                let inner_tag = ber_tag_for_shape(v.data.fields[0].shape())?;
                                if inner_tag.is_some_and(|vtag| vtag == tag) {
                                    let wip = wip.select_nth_variant(i).unwrap();
                                    self.stack.push(DeserializeTask::Pop(PopReason::ObjectVal));
                                    self.stack.push(DeserializeTask::Value { with_tag: None });
                                    return Ok(wip);
                                }
                            }
                            StructKind::TupleStruct | StructKind::Struct | StructKind::Tuple => {
                                if let Some(discriminant) = v.discriminant {
                                    let expected_tag =
                                        discriminant as u8 | tag::ASN1_CLASS_CONTEXT_SPECIFIC;
                                    if tag == expected_tag {
                                        let wip = wip.select_nth_variant(i).unwrap();
                                        let len = self.next_tl(tag)?;
                                        self.stack.push(DeserializeTask::Pop(PopReason::Object {
                                            end: self.pos + len,
                                        }));
                                        for i in (0..v.data.fields.len()).rev() {
                                            self.stack.push(DeserializeTask::Field(i));
                                        }
                                        return Ok(wip);
                                    }
                                }
                            }
                        }
                    }
                    Err(Asn1DeserError::UnknownTag {
                        tag,
                        position: self.pos,
                    })
                }
                _ => Err(Asn1DeserError::UnsupportedShape),
            },
            _ => Err(Asn1DeserError::UnsupportedShape),
        }
    }
}

/// Deserialize an ASN.1 DER slice given some some [`Partial`] into a [`HeapValue`]
pub fn deserialize_der_wip<'facet>(
    input: &[u8],
    wip: Partial<'facet>,
) -> Result<HeapValue<'facet>, Asn1DeserError> {
    let mut runner = Asn1DeserializerStack {
        _rules: EncodingRules::Distinguished,
        input,
        pos: 0,
        stack: vec![
            DeserializeTask::Pop(PopReason::TopLevel),
            DeserializeTask::Value { with_tag: None },
        ],
    };

    let mut wip = wip;
    loop {
        match runner.stack.pop() {
            Some(DeserializeTask::Pop(reason)) => match reason {
                PopReason::TopLevel => {
                    return Ok(wip.build().unwrap());
                }
                PopReason::Object { end } => {
                    if runner.pos != end {
                        return Err(Asn1DeserError::SequenceSizeMismatch {
                            sequence_end: end,
                            content_end: runner.pos,
                        });
                    }
                }
                PopReason::ListVal { end } => {
                    if runner.pos < end {
                        runner
                            .stack
                            .push(DeserializeTask::Pop(PopReason::ListVal { end }));
                        runner.stack.push(DeserializeTask::Value { with_tag: None });
                    } else if runner.pos > end {
                        return Err(Asn1DeserError::SequenceSizeMismatch {
                            sequence_end: end,
                            content_end: runner.pos,
                        });
                    }
                }
                _ => {
                    wip = wip.end().unwrap();
                }
            },
            Some(DeserializeTask::Value { with_tag }) => {
                wip = runner.next(wip, with_tag)?;
            }
            Some(DeserializeTask::Field(index)) => {
                runner
                    .stack
                    .push(DeserializeTask::Pop(PopReason::ObjectVal));
                runner.stack.push(DeserializeTask::Value { with_tag: None });
                wip = wip.begin_nth_field(index).unwrap();
            }
            None => unreachable!("Instruction stack is empty"),
        }
    }
}

/// Deserialize a slice of ASN.1 DER bytes into a Facet type
pub fn deserialize_der<'f, F: facet_core::Facet<'f>>(input: &[u8]) -> Result<F, Asn1DeserError> {
    let v = deserialize_der_wip(input, Partial::alloc_shape(F::SHAPE).unwrap())?;
    let f: F = v.materialize().unwrap();
    Ok(f)
}
