//! The self-describing (tag-led) codec.
//!
//! This module encodes and decodes a [`Schema`] in self-describing form. A
//! schema is an ordinary phon value, so it rides the one mode that needs nothing
//! agreed in advance — that is how two peers bootstrap schema exchange
//! (`r[self-describing.bootstraps-schemas]`). The encoding is a hand-written,
//! full-fidelity walk of the typed `Schema` (the coarse [`Value`](crate::Value)
//! can't round-trip a schema's `u32` counts and enum variants), using the rich
//! tag table directly.
//!
//! Decoding is the first real untrusted-input path: every length, tag, depth,
//! and UTF-8 check from `r[validate.*]` runs here, via [`crate::bytes::Reader`].
//!
//! The coarse `Value` codec for the `Dynamic` kind is also here
//! ([`value_to_bytes`] / [`value_from_bytes`]): it folds the rich self-describing
//! tag set onto `facet_value::Value`'s coarser cases (one `number`, one `array`,
//! one `object`), so a schema-less decode recovers a `Value` and `Dynamic`
//! round-trips one.
//!
//! Spec: "Self-describing mode", `r[value]`.

use std::collections::HashSet;

use facet_value::{VArray, VBytes, VNumber, VObject, VString, Value, ValueType};

use crate::bytes::{
    DecodeError, Reader, Sink, write_bool, write_bytes, write_f64, write_i128, write_i64,
    write_str, write_u8, write_u32, write_u64, write_u128,
};
use crate::schema::{
    ChannelDirection, Field, Primitive, Schema, SchemaId, SchemaKind, SchemaRef, Variant,
    VariantPayload,
};

/// Maximum schema nesting depth accepted on decode (`r[validate.depth]`). A
/// schema nesting deeper than this is a decode error, not a stack overflow.
const MAX_DEPTH: usize = 128;

/// Self-describing tag bytes (`r[self-describing.tag-led]`).
mod tag {
    pub const UNIT: u8 = 0x00;
    pub const BOOL: u8 = 0x01;
    pub const U8: u8 = 0x02;
    pub const U16: u8 = 0x03;
    pub const U32: u8 = 0x04;
    pub const U64: u8 = 0x05;
    pub const U128: u8 = 0x06;
    pub const I8: u8 = 0x07;
    pub const I16: u8 = 0x08;
    pub const I32: u8 = 0x09;
    pub const I64: u8 = 0x0A;
    pub const I128: u8 = 0x0B;
    pub const F32: u8 = 0x0C;
    pub const F64: u8 = 0x0D;
    pub const CHAR: u8 = 0x0E;
    pub const STRING: u8 = 0x0F;
    pub const BYTES: u8 = 0x10;
    pub const LIST: u8 = 0x11;
    pub const SET: u8 = 0x12;
    pub const MAP: u8 = 0x13;
    pub const ARRAY: u8 = 0x14;
    pub const TUPLE: u8 = 0x15;
    pub const STRUCT: u8 = 0x16;
    pub const ENUM: u8 = 0x17;
    pub const OPTION_NONE: u8 = 0x18;
    pub const OPTION_SOME: u8 = 0x19;
    pub const TENSOR: u8 = 0x1A;
}

// ============================================================================
// Public API
// ============================================================================

/// Encode a schema to self-describing bytes.
#[must_use]
pub fn schema_to_bytes(schema: &Schema) -> Vec<u8> {
    let mut out = Vec::new();
    enc_schema(&mut out, schema);
    out
}

/// Decode a schema from self-describing bytes, rejecting trailing bytes.
///
/// # Errors
/// Returns a [`DecodeError`] for any malformed input — out-of-range tags,
/// lengths beyond the buffer, excessive nesting, invalid UTF-8, or leftover
/// bytes.
pub fn schema_from_bytes(buf: &[u8]) -> Result<Schema, DecodeError> {
    let mut r = Reader::new(buf);
    let schema = dec_schema(&mut r, 0)?;
    if r.remaining() != 0 {
        return Err(DecodeError::TrailingBytes(r.remaining()));
    }
    Ok(schema)
}

// ============================================================================
// Encoding — scalar/value helpers
// ============================================================================

fn v_u32<S: Sink>(out: &mut S, n: u32) {
    write_u8(out, tag::U32);
    write_u32(out, n);
}

fn v_u64<S: Sink>(out: &mut S, n: u64) {
    write_u8(out, tag::U64);
    write_u64(out, n);
}

fn v_bool<S: Sink>(out: &mut S, b: bool) {
    write_u8(out, tag::BOOL);
    write_bool(out, b);
}

fn v_str<S: Sink>(out: &mut S, s: &str) {
    write_u8(out, tag::STRING);
    write_str(out, s);
}

fn v_unit<S: Sink>(out: &mut S) {
    write_u8(out, tag::UNIT);
}

/// Begin a struct value: the tag, the struct name, and the field count. Each
/// field then follows as a name string plus its value.
fn st<S: Sink>(out: &mut S, name: &str, fields: u32) {
    write_u8(out, tag::STRUCT);
    write_str(out, name);
    write_u32(out, fields);
}

/// Begin a list value of `len` elements.
fn list_begin<S: Sink>(out: &mut S, len: usize) {
    write_u8(out, tag::LIST);
    write_u32(out, len as u32);
}

// ============================================================================
// Encoding — schema
// ============================================================================

fn enc_schema<S: Sink>(out: &mut S, s: &Schema) {
    st(out, "Schema", 3);
    write_str(out, "id");
    v_u64(out, s.id.0);
    write_str(out, "type_params");
    list_begin(out, s.type_params.len());
    for p in &s.type_params {
        v_str(out, p);
    }
    write_str(out, "kind");
    enc_kind(out, &s.kind);
}

// r[impl self-describing.enum-payload]
fn enc_kind<S: Sink>(out: &mut S, k: &SchemaKind) {
    write_u8(out, tag::ENUM);
    match k {
        SchemaKind::Primitive(p) => {
            write_str(out, "Primitive");
            enc_primitive(out, *p);
        }
        SchemaKind::Struct { name, fields } => {
            write_str(out, "Struct");
            st(out, "Struct", 2);
            write_str(out, "name");
            v_str(out, name);
            write_str(out, "fields");
            enc_field_list(out, fields);
        }
        SchemaKind::Enum { name, variants } => {
            write_str(out, "Enum");
            st(out, "Enum", 2);
            write_str(out, "name");
            v_str(out, name);
            write_str(out, "variants");
            list_begin(out, variants.len());
            for v in variants {
                enc_variant(out, v);
            }
        }
        SchemaKind::Tuple { elements } => {
            write_str(out, "Tuple");
            st(out, "Tuple", 1);
            write_str(out, "elements");
            enc_ref_list(out, elements);
        }
        SchemaKind::List { element } => {
            write_str(out, "List");
            st(out, "List", 1);
            write_str(out, "element");
            enc_ref(out, element);
        }
        SchemaKind::Set { element } => {
            write_str(out, "Set");
            st(out, "Set", 1);
            write_str(out, "element");
            enc_ref(out, element);
        }
        SchemaKind::Option { element } => {
            write_str(out, "Option");
            st(out, "Option", 1);
            write_str(out, "element");
            enc_ref(out, element);
        }
        SchemaKind::Map { key, value } => {
            write_str(out, "Map");
            st(out, "Map", 2);
            write_str(out, "key");
            enc_ref(out, key);
            write_str(out, "value");
            enc_ref(out, value);
        }
        SchemaKind::Array {
            element,
            dimensions,
        } => {
            write_str(out, "Array");
            st(out, "Array", 2);
            write_str(out, "element");
            enc_ref(out, element);
            write_str(out, "dimensions");
            list_begin(out, dimensions.len());
            for d in dimensions {
                v_u64(out, *d);
            }
        }
        SchemaKind::Tensor { element, rank } => {
            write_str(out, "Tensor");
            st(out, "Tensor", 2);
            write_str(out, "element");
            enc_ref(out, element);
            write_str(out, "rank");
            match rank {
                None => write_u8(out, tag::OPTION_NONE),
                Some(r) => {
                    write_u8(out, tag::OPTION_SOME);
                    v_u32(out, *r);
                }
            }
        }
        SchemaKind::Channel { direction, element } => {
            write_str(out, "Channel");
            st(out, "Channel", 2);
            write_str(out, "direction");
            enc_direction(out, *direction);
            write_str(out, "element");
            enc_ref(out, element);
        }
        SchemaKind::Dynamic => {
            write_str(out, "Dynamic");
            v_unit(out);
        }
        SchemaKind::External { kind, metadata } => {
            write_str(out, "External");
            st(out, "External", 2);
            write_str(out, "kind");
            v_str(out, kind);
            write_str(out, "metadata");
            match metadata {
                None => write_u8(out, tag::OPTION_NONE),
                Some(r) => {
                    write_u8(out, tag::OPTION_SOME);
                    enc_ref(out, r);
                }
            }
        }
    }
}

fn enc_primitive<S: Sink>(out: &mut S, p: Primitive) {
    write_u8(out, tag::ENUM);
    write_str(out, p.tag());
    v_unit(out);
}

fn enc_direction<S: Sink>(out: &mut S, d: ChannelDirection) {
    write_u8(out, tag::ENUM);
    write_str(
        out,
        match d {
            ChannelDirection::Tx => "tx",
            ChannelDirection::Rx => "rx",
        },
    );
    v_unit(out);
}

fn enc_ref<S: Sink>(out: &mut S, r: &SchemaRef) {
    write_u8(out, tag::ENUM);
    match r {
        SchemaRef::Concrete { id, args } => {
            write_str(out, "Concrete");
            st(out, "Concrete", 2);
            write_str(out, "id");
            v_u64(out, id.0);
            write_str(out, "args");
            enc_ref_list(out, args);
        }
        SchemaRef::Var { name } => {
            write_str(out, "Var");
            st(out, "Var", 1);
            write_str(out, "name");
            v_str(out, name);
        }
    }
}

fn enc_field<S: Sink>(out: &mut S, f: &Field) {
    st(out, "Field", 3);
    write_str(out, "name");
    v_str(out, &f.name);
    write_str(out, "schema");
    enc_ref(out, &f.schema);
    write_str(out, "required");
    v_bool(out, f.required);
}

fn enc_variant<S: Sink>(out: &mut S, v: &Variant) {
    st(out, "Variant", 3);
    write_str(out, "name");
    v_str(out, &v.name);
    write_str(out, "index");
    v_u32(out, v.index);
    write_str(out, "payload");
    enc_variant_payload(out, &v.payload);
}

fn enc_variant_payload<S: Sink>(out: &mut S, p: &VariantPayload) {
    write_u8(out, tag::ENUM);
    match p {
        VariantPayload::Unit => {
            write_str(out, "Unit");
            v_unit(out);
        }
        VariantPayload::Newtype(r) => {
            write_str(out, "Newtype");
            enc_ref(out, r);
        }
        VariantPayload::Tuple(refs) => {
            write_str(out, "Tuple");
            enc_ref_list(out, refs);
        }
        VariantPayload::Struct(fields) => {
            write_str(out, "Struct");
            enc_field_list(out, fields);
        }
    }
}

fn enc_ref_list<S: Sink>(out: &mut S, refs: &[SchemaRef]) {
    list_begin(out, refs.len());
    for r in refs {
        enc_ref(out, r);
    }
}

fn enc_field_list<S: Sink>(out: &mut S, fields: &[Field]) {
    list_begin(out, fields.len());
    for f in fields {
        enc_field(out, f);
    }
}

// ============================================================================
// Decoding — primitives
// ============================================================================

fn check_depth(depth: usize) -> Result<(), DecodeError> {
    if depth > MAX_DEPTH {
        Err(DecodeError::DepthExceeded)
    } else {
        Ok(())
    }
}

// r[impl validate.tags]
fn expect(r: &mut Reader, t: u8, what: &'static str) -> Result<(), DecodeError> {
    let got = r.read_u8()?;
    if got == t {
        Ok(())
    } else {
        Err(DecodeError::UnexpectedTag { expected: what, got })
    }
}

fn d_u32(r: &mut Reader) -> Result<u32, DecodeError> {
    expect(r, tag::U32, "u32")?;
    r.read_u32()
}

fn d_u64(r: &mut Reader) -> Result<u64, DecodeError> {
    expect(r, tag::U64, "u64")?;
    r.read_u64()
}

fn d_bool(r: &mut Reader) -> Result<bool, DecodeError> {
    expect(r, tag::BOOL, "bool")?;
    r.read_bool()
}

fn d_str(r: &mut Reader) -> Result<String, DecodeError> {
    expect(r, tag::STRING, "string")?;
    Ok(r.read_str()?.to_string())
}

fn d_unit(r: &mut Reader) -> Result<(), DecodeError> {
    expect(r, tag::UNIT, "unit")
}

/// Read a struct header (tag, name, field count), verifying the field count.
fn st_begin(r: &mut Reader, fields: u32) -> Result<(), DecodeError> {
    expect(r, tag::STRUCT, "struct")?;
    r.read_str()?; // struct name (informational)
    if r.read_u32()? != fields {
        return Err(DecodeError::Malformed("struct field count"));
    }
    Ok(())
}

/// Read and discard a struct field's name (fields are positional here).
fn fname(r: &mut Reader) -> Result<(), DecodeError> {
    r.read_str()?;
    Ok(())
}

/// Read a list header, returning the element count (bounded by the buffer).
fn list_len(r: &mut Reader) -> Result<usize, DecodeError> {
    expect(r, tag::LIST, "list")?;
    r.read_len(1)
}

// ============================================================================
// Decoding — schema
// ============================================================================

// r[impl validate.depth]
fn dec_schema(r: &mut Reader, depth: usize) -> Result<Schema, DecodeError> {
    check_depth(depth)?;
    st_begin(r, 3)?;
    fname(r)?;
    let id = SchemaId(d_u64(r)?);
    fname(r)?;
    let n = list_len(r)?;
    let mut type_params = Vec::with_capacity(n);
    for _ in 0..n {
        type_params.push(d_str(r)?);
    }
    fname(r)?;
    let kind = dec_kind(r, depth + 1)?;
    Ok(Schema {
        id,
        type_params,
        kind,
    })
}

fn dec_kind(r: &mut Reader, depth: usize) -> Result<SchemaKind, DecodeError> {
    check_depth(depth)?;
    expect(r, tag::ENUM, "enum")?;
    let variant = r.read_str()?.to_string();
    Ok(match variant.as_str() {
        "Primitive" => SchemaKind::Primitive(dec_primitive(r)?),
        "Struct" => {
            st_begin(r, 2)?;
            fname(r)?;
            let name = d_str(r)?;
            fname(r)?;
            let fields = dec_field_list(r, depth + 1)?;
            SchemaKind::Struct { name, fields }
        }
        "Enum" => {
            st_begin(r, 2)?;
            fname(r)?;
            let name = d_str(r)?;
            fname(r)?;
            let count = list_len(r)?;
            let mut variants = Vec::with_capacity(count);
            for _ in 0..count {
                variants.push(dec_variant(r, depth + 1)?);
            }
            SchemaKind::Enum { name, variants }
        }
        "Tuple" => {
            st_begin(r, 1)?;
            fname(r)?;
            SchemaKind::Tuple {
                elements: dec_ref_list(r, depth + 1)?,
            }
        }
        "List" => {
            st_begin(r, 1)?;
            fname(r)?;
            SchemaKind::List {
                element: dec_ref(r, depth + 1)?,
            }
        }
        "Set" => {
            st_begin(r, 1)?;
            fname(r)?;
            SchemaKind::Set {
                element: dec_ref(r, depth + 1)?,
            }
        }
        "Option" => {
            st_begin(r, 1)?;
            fname(r)?;
            SchemaKind::Option {
                element: dec_ref(r, depth + 1)?,
            }
        }
        "Map" => {
            st_begin(r, 2)?;
            fname(r)?;
            let key = dec_ref(r, depth + 1)?;
            fname(r)?;
            let value = dec_ref(r, depth + 1)?;
            SchemaKind::Map { key, value }
        }
        "Array" => {
            st_begin(r, 2)?;
            fname(r)?;
            let element = dec_ref(r, depth + 1)?;
            fname(r)?;
            let count = list_len(r)?;
            let mut dimensions = Vec::with_capacity(count);
            for _ in 0..count {
                dimensions.push(d_u64(r)?);
            }
            SchemaKind::Array {
                element,
                dimensions,
            }
        }
        "Tensor" => {
            st_begin(r, 2)?;
            fname(r)?;
            let element = dec_ref(r, depth + 1)?;
            fname(r)?;
            let rank = match r.read_u8()? {
                tag::OPTION_NONE => None,
                tag::OPTION_SOME => Some(d_u32(r)?),
                got => return Err(DecodeError::UnexpectedTag { expected: "option", got }),
            };
            SchemaKind::Tensor { element, rank }
        }
        "Channel" => {
            st_begin(r, 2)?;
            fname(r)?;
            let direction = dec_direction(r)?;
            fname(r)?;
            let element = dec_ref(r, depth + 1)?;
            SchemaKind::Channel { direction, element }
        }
        "Dynamic" => {
            d_unit(r)?;
            SchemaKind::Dynamic
        }
        "External" => {
            st_begin(r, 2)?;
            fname(r)?;
            let kind = d_str(r)?;
            fname(r)?;
            let metadata = match r.read_u8()? {
                tag::OPTION_NONE => None,
                tag::OPTION_SOME => Some(dec_ref(r, depth + 1)?),
                got => return Err(DecodeError::UnexpectedTag { expected: "option", got }),
            };
            SchemaKind::External { kind, metadata }
        }
        _ => return Err(DecodeError::UnknownVariant(variant)),
    })
}

fn dec_primitive(r: &mut Reader) -> Result<Primitive, DecodeError> {
    expect(r, tag::ENUM, "enum")?;
    let name = r.read_str()?.to_string();
    d_unit(r)?;
    Primitive::from_tag(&name).ok_or(DecodeError::UnknownVariant(name))
}

fn dec_direction(r: &mut Reader) -> Result<ChannelDirection, DecodeError> {
    expect(r, tag::ENUM, "enum")?;
    let name = r.read_str()?.to_string();
    d_unit(r)?;
    match name.as_str() {
        "tx" => Ok(ChannelDirection::Tx),
        "rx" => Ok(ChannelDirection::Rx),
        _ => Err(DecodeError::UnknownVariant(name)),
    }
}

fn dec_ref(r: &mut Reader, depth: usize) -> Result<SchemaRef, DecodeError> {
    check_depth(depth)?;
    expect(r, tag::ENUM, "enum")?;
    let variant = r.read_str()?.to_string();
    match variant.as_str() {
        "Concrete" => {
            st_begin(r, 2)?;
            fname(r)?;
            let id = SchemaId(d_u64(r)?);
            fname(r)?;
            let args = dec_ref_list(r, depth + 1)?;
            Ok(SchemaRef::Concrete { id, args })
        }
        "Var" => {
            st_begin(r, 1)?;
            fname(r)?;
            Ok(SchemaRef::Var { name: d_str(r)? })
        }
        _ => Err(DecodeError::UnknownVariant(variant)),
    }
}

fn dec_field(r: &mut Reader, depth: usize) -> Result<Field, DecodeError> {
    check_depth(depth)?;
    st_begin(r, 3)?;
    fname(r)?;
    let name = d_str(r)?;
    fname(r)?;
    let schema = dec_ref(r, depth + 1)?;
    fname(r)?;
    let required = d_bool(r)?;
    Ok(Field {
        name,
        schema,
        required,
    })
}

fn dec_variant(r: &mut Reader, depth: usize) -> Result<Variant, DecodeError> {
    check_depth(depth)?;
    st_begin(r, 3)?;
    fname(r)?;
    let name = d_str(r)?;
    fname(r)?;
    let index = d_u32(r)?;
    fname(r)?;
    let payload = dec_variant_payload(r, depth + 1)?;
    Ok(Variant {
        name,
        index,
        payload,
    })
}

fn dec_variant_payload(r: &mut Reader, depth: usize) -> Result<VariantPayload, DecodeError> {
    check_depth(depth)?;
    expect(r, tag::ENUM, "enum")?;
    let variant = r.read_str()?.to_string();
    match variant.as_str() {
        "Unit" => {
            d_unit(r)?;
            Ok(VariantPayload::Unit)
        }
        "Newtype" => Ok(VariantPayload::Newtype(dec_ref(r, depth + 1)?)),
        "Tuple" => Ok(VariantPayload::Tuple(dec_ref_list(r, depth + 1)?)),
        "Struct" => Ok(VariantPayload::Struct(dec_field_list(r, depth + 1)?)),
        _ => Err(DecodeError::UnknownVariant(variant)),
    }
}

fn dec_ref_list(r: &mut Reader, depth: usize) -> Result<Vec<SchemaRef>, DecodeError> {
    let n = list_len(r)?;
    let mut v = Vec::with_capacity(n);
    for _ in 0..n {
        v.push(dec_ref(r, depth + 1)?);
    }
    Ok(v)
}

fn dec_field_list(r: &mut Reader, depth: usize) -> Result<Vec<Field>, DecodeError> {
    let n = list_len(r)?;
    let mut v = Vec::with_capacity(n);
    for _ in 0..n {
        v.push(dec_field(r, depth + 1)?);
    }
    Ok(v)
}

// ============================================================================
// The coarse Value codec
// ============================================================================

/// Why encoding a [`Value`] failed.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum EncodeError {
    /// A `Value` case phon has no self-describing tag for, and no agreed
    /// encoding yet: date/time, qualified name, or uuid.
    Unsupported(&'static str),
}

impl core::fmt::Display for EncodeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            EncodeError::Unsupported(k) => {
                write!(f, "no self-describing encoding for value kind: {k}")
            }
        }
    }
}

impl std::error::Error for EncodeError {}

/// Encode a [`Value`] to self-describing bytes.
///
/// # Errors
/// [`EncodeError::Unsupported`] for the `facet_value` cases phon has no tag for
/// (date/time, qname, uuid).
pub fn value_to_bytes(value: &Value) -> Result<Vec<u8>, EncodeError> {
    let mut out = Vec::new();
    write_value(&mut out, value)?;
    Ok(out)
}

/// Decode a [`Value`] from self-describing bytes, rejecting trailing bytes.
///
/// # Errors
/// [`DecodeError`] for any malformed input.
pub fn value_from_bytes(buf: &[u8]) -> Result<Value, DecodeError> {
    let mut r = Reader::new(buf);
    let v = read_value(&mut r)?;
    if r.remaining() != 0 {
        return Err(DecodeError::TrailingBytes(r.remaining()));
    }
    Ok(v)
}

/// Write a [`Value`] into a sink in self-describing form. Each `Value` case has
/// a fixed tag, so `Dynamic` bytes are canonical (`r[value]`).
///
/// # Errors
/// As [`value_to_bytes`].
// r[impl value]
pub fn write_value<S: Sink>(out: &mut S, value: &Value) -> Result<(), EncodeError> {
    match value.value_type() {
        ValueType::Null => write_u8(out, tag::OPTION_NONE),
        ValueType::Bool => {
            write_u8(out, tag::BOOL);
            write_bool(out, value.as_bool().unwrap());
        }
        ValueType::Number => enc_number(out, value.as_number().unwrap()),
        ValueType::String => {
            write_u8(out, tag::STRING);
            write_str(out, value.as_string().unwrap().as_str());
        }
        ValueType::Bytes => {
            write_u8(out, tag::BYTES);
            write_bytes(out, value.as_bytes().unwrap().as_slice());
        }
        ValueType::Char => {
            write_u8(out, tag::CHAR);
            write_u32(out, value.as_char().unwrap() as u32);
        }
        ValueType::Array => {
            let a = value.as_array().unwrap();
            write_u8(out, tag::LIST);
            write_u32(out, a.len() as u32);
            for i in 0..a.len() {
                write_value(out, a.get(i).unwrap())?;
            }
        }
        ValueType::Object => {
            let o = value.as_object().unwrap();
            write_u8(out, tag::MAP);
            write_u32(out, o.len() as u32);
            for (key, val) in o.iter() {
                write_u8(out, tag::STRING);
                write_str(out, key.as_str());
                write_value(out, val)?;
            }
        }
        ValueType::DateTime => return Err(EncodeError::Unsupported("datetime")),
        ValueType::QName => return Err(EncodeError::Unsupported("qname")),
        ValueType::Uuid => return Err(EncodeError::Unsupported("uuid")),
        // ValueType is #[non_exhaustive]: a future kind has no encoding yet.
        _ => return Err(EncodeError::Unsupported("unknown")),
    }
    Ok(())
}

/// A number's wire tag follows its canonical storage: float to `f64`, otherwise
/// the narrowest of `i64`/`u64`/`i128`/`u128` that holds it (matching
/// `VNumber`'s magnitude canonicalization, so the choice is deterministic).
fn enc_number<S: Sink>(out: &mut S, n: &VNumber) {
    if n.is_float() {
        write_u8(out, tag::F64);
        write_f64(out, n.to_f64_lossy());
    } else if let Some(i) = n.to_i64() {
        write_u8(out, tag::I64);
        write_i64(out, i);
    } else if let Some(u) = n.to_u64() {
        write_u8(out, tag::U64);
        write_u64(out, u);
    } else if let Some(i) = n.to_i128() {
        write_u8(out, tag::I128);
        write_i128(out, i);
    } else {
        write_u8(out, tag::U128);
        write_u128(out, n.to_u128().expect("a non-float integer fits one width"));
    }
}

/// Read a [`Value`] from a reader (for embedding, e.g. a `Dynamic` field).
///
/// # Errors
/// [`DecodeError`] for any malformed input.
pub fn read_value(r: &mut Reader) -> Result<Value, DecodeError> {
    dec_value(r, 0)
}

// r[impl validate.tags]
fn dec_value(r: &mut Reader, depth: usize) -> Result<Value, DecodeError> {
    check_depth(depth)?;
    let t = r.read_u8()?;
    Ok(match t {
        tag::UNIT | tag::OPTION_NONE => Value::NULL,
        tag::BOOL => Value::from(r.read_bool()?),
        tag::U8 => Value::from(r.read_u8()?),
        tag::U16 => Value::from(r.read_u16()?),
        tag::U32 => Value::from(r.read_u32()?),
        tag::U64 => Value::from(r.read_u64()?),
        tag::U128 => Value::from(r.read_u128()?),
        tag::I8 => Value::from(r.read_i8()?),
        tag::I16 => Value::from(r.read_i16()?),
        tag::I32 => Value::from(r.read_i32()?),
        tag::I64 => Value::from(r.read_i64()?),
        tag::I128 => Value::from(r.read_i128()?),
        tag::F32 => Value::from(r.read_f32()?),
        tag::F64 => Value::from(r.read_f64()?),
        tag::CHAR => Value::from(r.read_char()?),
        tag::STRING => VString::new(r.read_str()?).into(),
        tag::BYTES => VBytes::new(r.read_bytes()?).into(),
        // list and tuple both fold to a flat array.
        tag::LIST | tag::TUPLE => {
            let n = r.read_len(1)?;
            let mut a = VArray::new();
            for _ in 0..n {
                a.push(dec_value(r, depth + 1)?);
            }
            a.into()
        }
        // r[impl validate.uniqueness]
        tag::SET => {
            let n = r.read_len(1)?;
            let mut a = VArray::new();
            let mut seen: HashSet<Value> = HashSet::new();
            for _ in 0..n {
                let elem = dec_value(r, depth + 1)?;
                if !seen.insert(elem.clone()) {
                    return Err(DecodeError::DuplicateElement);
                }
                a.push(elem);
            }
            a.into()
        }
        tag::MAP => dec_map(r, depth)?,
        tag::ARRAY | tag::TENSOR => dec_dimensioned(r, depth)?,
        tag::STRUCT => dec_struct_value(r, depth)?,
        tag::ENUM => dec_enum_value(r, depth)?,
        tag::OPTION_SOME => dec_value(r, depth + 1)?,
        other => return Err(DecodeError::UnknownTag(other)),
    })
}

/// A `map` folds to an object when its keys are all strings, else to an array of
/// `[key, value]` pairs. Keys must be unique either way (`r[validate.uniqueness]`).
fn dec_map(r: &mut Reader, depth: usize) -> Result<Value, DecodeError> {
    let n = r.read_len(2)?;
    let mut entries: Vec<(Value, Value)> = Vec::new();
    let mut seen: HashSet<Value> = HashSet::new();
    let mut all_string = true;
    for _ in 0..n {
        let key = dec_value(r, depth + 1)?;
        let val = dec_value(r, depth + 1)?;
        if !seen.insert(key.clone()) {
            return Err(DecodeError::DuplicateKey);
        }
        if key.value_type() != ValueType::String {
            all_string = false;
        }
        entries.push((key, val));
    }
    if all_string {
        let mut o = VObject::new();
        for (key, val) in entries {
            o.insert(VString::new(key.as_string().unwrap().as_str()), val);
        }
        Ok(o.into())
    } else {
        let mut a = VArray::new();
        for (key, val) in entries {
            let mut pair = VArray::new();
            pair.push(key);
            pair.push(val);
            a.push(pair);
        }
        Ok(a.into())
    }
}

/// `array` and `tensor` fold to a flat array of their elements. The dimensions
/// are validated (`r[validate.dimensions]`): rank and the element product are
/// bounded by the buffer, and the product is computed with checked arithmetic.
// r[impl validate.dimensions]
fn dec_dimensioned(r: &mut Reader, depth: usize) -> Result<Value, DecodeError> {
    let rank = r.read_u32()? as usize;
    if rank.checked_mul(8).is_none_or(|bytes| bytes > r.remaining()) {
        return Err(DecodeError::LengthTooLarge {
            count: rank as u64,
            remaining: r.remaining(),
        });
    }
    let mut product: u64 = 1;
    for _ in 0..rank {
        let dim = r.read_u64()?;
        product = product
            .checked_mul(dim)
            .ok_or(DecodeError::Malformed("array/tensor dimension overflow"))?;
    }
    if product > r.remaining() as u64 {
        return Err(DecodeError::LengthTooLarge {
            count: product,
            remaining: r.remaining(),
        });
    }
    let mut a = VArray::new();
    for _ in 0..product {
        a.push(dec_value(r, depth + 1)?);
    }
    Ok(a.into())
}

/// A `struct` folds to an object keyed by field name (names must be unique).
fn dec_struct_value(r: &mut Reader, depth: usize) -> Result<Value, DecodeError> {
    r.read_str()?; // struct name, folded away
    let n = r.read_len(1)?;
    let mut o = VObject::new();
    let mut seen: HashSet<String> = HashSet::new();
    for _ in 0..n {
        let field = r.read_str()?.to_string();
        if !seen.insert(field.clone()) {
            return Err(DecodeError::DuplicateKey);
        }
        let val = dec_value(r, depth + 1)?;
        o.insert(VString::new(&field), val);
    }
    Ok(o.into())
}

/// An `enum` folds to a one-entry object mapping the variant name to its single
/// payload value (`r[self-describing.enum-payload]`).
fn dec_enum_value(r: &mut Reader, depth: usize) -> Result<Value, DecodeError> {
    let variant = r.read_str()?.to_string();
    let payload = dec_value(r, depth + 1)?;
    let mut o = VObject::new();
    o.insert(VString::new(&variant), payload);
    Ok(o.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::primitive_id;

    fn concrete(p: Primitive) -> SchemaRef {
        SchemaRef::concrete(primitive_id(p))
    }

    fn roundtrip(schema: &Schema) {
        let bytes = schema_to_bytes(schema);
        let back = schema_from_bytes(&bytes).expect("decode");
        assert_eq!(schema, &back);
    }

    #[test]
    fn roundtrip_struct() {
        roundtrip(&Schema {
            id: SchemaId(0xABCD),
            type_params: Vec::new(),
            kind: SchemaKind::Struct {
                name: "Point".to_string(),
                fields: vec![
                    Field {
                        name: "x".to_string(),
                        schema: concrete(Primitive::U32),
                        required: true,
                    },
                    Field {
                        name: "y".to_string(),
                        schema: concrete(Primitive::F64),
                        required: false,
                    },
                ],
            },
        });
    }

    #[test]
    fn roundtrip_enum_all_payload_shapes() {
        roundtrip(&Schema {
            id: SchemaId(7),
            type_params: Vec::new(),
            kind: SchemaKind::Enum {
                name: "E".to_string(),
                variants: vec![
                    Variant {
                        name: "Nil".to_string(),
                        index: 0,
                        payload: VariantPayload::Unit,
                    },
                    Variant {
                        name: "One".to_string(),
                        index: 1,
                        payload: VariantPayload::Newtype(concrete(Primitive::U32)),
                    },
                    Variant {
                        name: "Pair".to_string(),
                        index: 2,
                        payload: VariantPayload::Tuple(vec![
                            concrete(Primitive::U32),
                            concrete(Primitive::String),
                        ]),
                    },
                    Variant {
                        name: "Rec".to_string(),
                        index: 3,
                        payload: VariantPayload::Struct(vec![Field {
                            name: "a".to_string(),
                            schema: concrete(Primitive::Bool),
                            required: true,
                        }]),
                    },
                ],
            },
        });
    }

    #[test]
    fn roundtrip_every_kind() {
        let r = concrete(Primitive::U32);
        let kinds = vec![
            SchemaKind::Primitive(Primitive::I128),
            SchemaKind::Tuple {
                elements: vec![r.clone(), concrete(Primitive::Bool)],
            },
            SchemaKind::List { element: r.clone() },
            SchemaKind::Set { element: r.clone() },
            SchemaKind::Option { element: r.clone() },
            SchemaKind::Map {
                key: concrete(Primitive::String),
                value: r.clone(),
            },
            SchemaKind::Array {
                element: r.clone(),
                dimensions: vec![256, 256],
            },
            SchemaKind::Tensor {
                element: r.clone(),
                rank: Some(2),
            },
            SchemaKind::Tensor {
                element: r.clone(),
                rank: None,
            },
            SchemaKind::Channel {
                direction: ChannelDirection::Rx,
                element: r.clone(),
            },
            SchemaKind::Dynamic,
            SchemaKind::External {
                kind: "blob".to_string(),
                metadata: Some(concrete(Primitive::U64)),
            },
            SchemaKind::External {
                kind: "fd".to_string(),
                metadata: None,
            },
        ];
        for (i, kind) in kinds.into_iter().enumerate() {
            roundtrip(&Schema {
                id: SchemaId(i as u64),
                type_params: Vec::new(),
                kind,
            });
        }
    }

    #[test]
    fn roundtrip_generic_with_var_and_args() {
        // A parametric schema with a Var, and a concrete ref carrying args.
        roundtrip(&Schema {
            id: SchemaId(42),
            type_params: vec!["T".to_string()],
            kind: SchemaKind::Struct {
                name: "Wrapper".to_string(),
                fields: vec![
                    Field {
                        name: "value".to_string(),
                        schema: SchemaRef::var("T"),
                        required: true,
                    },
                    Field {
                        name: "list".to_string(),
                        schema: SchemaRef::generic(SchemaId(999), vec![SchemaRef::var("T")]),
                        required: true,
                    },
                ],
            },
        });
    }

    #[test]
    fn rejects_trailing_bytes() {
        let mut bytes = schema_to_bytes(&Schema {
            id: SchemaId(1),
            type_params: Vec::new(),
            kind: SchemaKind::Dynamic,
        });
        bytes.push(0x00);
        assert!(matches!(
            schema_from_bytes(&bytes),
            Err(DecodeError::TrailingBytes(1))
        ));
    }

    #[test]
    fn rejects_truncated_input() {
        let bytes = schema_to_bytes(&Schema {
            id: SchemaId(1),
            type_params: Vec::new(),
            kind: SchemaKind::Dynamic,
        });
        for n in 0..bytes.len() {
            assert!(
                schema_from_bytes(&bytes[..n]).is_err(),
                "truncation at {n} should fail"
            );
        }
    }

    #[test]
    fn rejects_unknown_tag() {
        // Replace the leading struct tag with an undefined tag byte.
        let mut bytes = schema_to_bytes(&Schema {
            id: SchemaId(1),
            type_params: Vec::new(),
            kind: SchemaKind::Dynamic,
        });
        bytes[0] = 0x7F;
        assert!(schema_from_bytes(&bytes).is_err());
    }

    #[test]
    fn rejects_oversized_length() {
        // A struct claiming a huge field count must be rejected before allocating.
        let mut bytes = Vec::new();
        write_u8(&mut bytes, tag::STRUCT);
        write_str(&mut bytes, "Schema");
        write_u32(&mut bytes, u32::MAX); // absurd field count
        assert!(matches!(
            schema_from_bytes(&bytes),
            Err(DecodeError::Malformed(_))
        ));
    }

    // --- Value codec --------------------------------------------------------

    fn rt_value(v: Value) {
        let bytes = value_to_bytes(&v).expect("encode");
        let back = value_from_bytes(&bytes).expect("decode");
        assert_eq!(v, back);
        // byte-stable after the first encode.
        assert_eq!(value_to_bytes(&back).unwrap(), bytes);
    }

    fn ival(n: i64) -> Vec<u8> {
        let mut b = Vec::new();
        write_u8(&mut b, tag::I64);
        write_i64(&mut b, n);
        b
    }

    fn sval(s: &str) -> Vec<u8> {
        let mut b = Vec::new();
        write_u8(&mut b, tag::STRING);
        write_str(&mut b, s);
        b
    }

    #[test]
    fn value_roundtrip_scalars() {
        rt_value(Value::NULL);
        rt_value(Value::from(true));
        rt_value(Value::from(false));
        rt_value(Value::from(7i64));
        rt_value(Value::from(-5i64));
        rt_value(Value::from(u64::MAX));
        rt_value(Value::from(u128::MAX));
        rt_value(Value::from(i128::MIN));
        rt_value(Value::from(2.5f64));
        rt_value(VString::new("héllo λ").into());
        rt_value(VBytes::new(&[0, 1, 2, 255]).into());
        rt_value(Value::from('λ'));
    }

    #[test]
    fn value_roundtrip_composite() {
        let mut arr = VArray::new();
        arr.push(Value::from(1i64));
        arr.push(VString::new("x"));
        arr.push(Value::NULL);
        rt_value(arr.into());

        let mut obj = VObject::new();
        obj.insert(VString::new("a"), Value::from(1i64));
        obj.insert(VString::new("b"), Value::from(true));
        let mut nested = VArray::new();
        nested.push(Value::from('z'));
        obj.insert(VString::new("c"), Value::from(nested));
        rt_value(obj.into());
    }

    #[test]
    fn unit_and_option_none_fold_to_null() {
        assert!(value_from_bytes(&[tag::UNIT]).unwrap().is_null());
        assert!(value_from_bytes(&[tag::OPTION_NONE]).unwrap().is_null());
    }

    #[test]
    fn set_and_tuple_fold_to_array() {
        let mut bytes = Vec::new();
        write_u8(&mut bytes, tag::SET);
        write_u32(&mut bytes, 2);
        bytes.extend(ival(1));
        bytes.extend(ival(2));
        let v = value_from_bytes(&bytes).unwrap();
        assert_eq!(v.as_array().unwrap().len(), 2);

        let mut t = Vec::new();
        write_u8(&mut t, tag::TUPLE);
        write_u32(&mut t, 2);
        t.extend(ival(1));
        t.extend(sval("x"));
        assert_eq!(value_from_bytes(&t).unwrap().as_array().unwrap().len(), 2);
    }

    #[test]
    fn set_rejects_duplicate_elements() {
        let mut bytes = Vec::new();
        write_u8(&mut bytes, tag::SET);
        write_u32(&mut bytes, 2);
        bytes.extend(ival(9));
        bytes.extend(ival(9));
        assert_eq!(
            value_from_bytes(&bytes),
            Err(DecodeError::DuplicateElement)
        );
    }

    #[test]
    fn map_folds_to_object_and_rejects_duplicate_keys() {
        let mut bytes = Vec::new();
        write_u8(&mut bytes, tag::MAP);
        write_u32(&mut bytes, 2);
        bytes.extend(sval("a"));
        bytes.extend(ival(1));
        bytes.extend(sval("b"));
        bytes.extend(ival(2));
        let v = value_from_bytes(&bytes).unwrap();
        assert_eq!(v.as_object().unwrap().len(), 2);

        let mut dup = Vec::new();
        write_u8(&mut dup, tag::MAP);
        write_u32(&mut dup, 2);
        dup.extend(sval("a"));
        dup.extend(ival(1));
        dup.extend(sval("a"));
        dup.extend(ival(2));
        assert_eq!(value_from_bytes(&dup), Err(DecodeError::DuplicateKey));
    }

    #[test]
    fn struct_and_enum_fold_to_object() {
        // struct: name "S", one field "f" = 1
        let mut s = Vec::new();
        write_u8(&mut s, tag::STRUCT);
        write_str(&mut s, "S");
        write_u32(&mut s, 1);
        write_str(&mut s, "f");
        s.extend(ival(1));
        assert!(value_from_bytes(&s).unwrap().as_object().is_some());

        // enum: variant "V" with payload 1 -> object { "V": 1 }
        let mut e = Vec::new();
        write_u8(&mut e, tag::ENUM);
        write_str(&mut e, "V");
        e.extend(ival(1));
        let obj = value_from_bytes(&e).unwrap();
        assert_eq!(obj.as_object().unwrap().len(), 1);
    }

    #[test]
    fn array_tag_folds_to_flat_array() {
        // rank 1, dim [3], three i64 elements
        let mut bytes = Vec::new();
        write_u8(&mut bytes, tag::ARRAY);
        write_u32(&mut bytes, 1);
        write_u64(&mut bytes, 3);
        for n in 0..3 {
            bytes.extend(ival(n));
        }
        assert_eq!(value_from_bytes(&bytes).unwrap().as_array().unwrap().len(), 3);
    }

    #[test]
    fn value_rejects_malformed_input() {
        // unknown tag
        assert_eq!(value_from_bytes(&[0x7F]), Err(DecodeError::UnknownTag(0x7F)));
        // truncated string value
        let mut s = sval("hello");
        s.truncate(s.len() - 2);
        assert!(value_from_bytes(&s).is_err());
        // oversized list count
        let mut big = Vec::new();
        write_u8(&mut big, tag::LIST);
        write_u32(&mut big, u32::MAX);
        assert!(matches!(
            value_from_bytes(&big),
            Err(DecodeError::LengthTooLarge { .. })
        ));
        // excessive nesting
        let mut deep = vec![tag::OPTION_SOME; MAX_DEPTH + 2];
        deep.push(tag::OPTION_NONE);
        assert_eq!(value_from_bytes(&deep), Err(DecodeError::DepthExceeded));
    }

    #[test]
    fn encode_rejects_unsupported_kinds() {
        let uuid: Value = facet_value::VUuid::from_u128(0).into();
        assert_eq!(
            value_to_bytes(&uuid),
            Err(EncodeError::Unsupported("uuid"))
        );
    }
}
