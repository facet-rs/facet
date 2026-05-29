//! The compact (schema-driven) codec for the dynamic [`Value`].
//!
//! Compact mode carries no tags and no names: the schema says what comes next,
//! so the bytes are just the values, back to back, with alignment padding before
//! aligned scalars (`r[compact.schema-driven]`, `r[compact.alignment]`). This
//! module encodes/decodes a `facet_value::Value` against a schema — the
//! schema-less counterpart to the self-describing `Value` codec.
//!
//! References are resolved against a [`Registry`] (a schema closure); primitives
//! are recognized by their canonical id intrinsically. This first cut covers
//! primitives (except datetime/uuid/qname), struct, enum, tuple, list, set, map,
//! array, option, and dynamic; tensor, the string-carried primitives, channel,
//! external, and generics are not yet wired and return
//! [`CompactError::Unsupported`].
//!
//! Spec: "Compact mode".

use std::collections::HashMap;

use facet_value::{VArray, VBytes, VObject, VString, Value};
use phon_schema::bytes::{
    Reader, write_bool, write_f32, write_f64, write_i8, write_i16, write_i32, write_i64,
    write_i128, write_u8, write_u16, write_u32, write_u64, write_u128,
};
use phon_schema::{
    DecodeError, EncodeError, Field, Primitive, Schema, SchemaId, SchemaKind, SchemaRef, Variant,
    VariantPayload, extended_from_string, extended_to_string, primitive_id, read_value, write_value,
};

/// Maximum nesting depth on decode (`r[validate.depth]`).
const MAX_DEPTH: usize = 128;

// ============================================================================
// Errors
// ============================================================================

/// Why a compact encode or decode failed.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum CompactError {
    /// A referenced schema id is not in the registry (`r[schema-identity.unknown-is-error]`).
    UnknownSchema(SchemaId),
    /// A kind or feature not yet implemented in this codec.
    Unsupported(&'static str),
    /// The value's shape does not match the schema it is being encoded against.
    TypeMismatch {
        expected: &'static str,
    },
    /// An enum value names a variant the schema does not have.
    UnknownVariant(String),
    /// A decoded enum variant index is out of range.
    BadVariantIndex(u32),
    /// A generic schema applied with the wrong number of type arguments.
    GenericArity { params: usize, args: usize },
    /// A structurally malformed schema (e.g. an unbound type variable, or a
    /// primitive carrying type arguments).
    Malformed(&'static str),
    /// A decode-side validation failure from the byte reader.
    Decode(DecodeError),
    /// A dynamic (self-describing) sub-value failed to encode.
    Encode(EncodeError),
}

impl From<DecodeError> for CompactError {
    fn from(e: DecodeError) -> Self {
        CompactError::Decode(e)
    }
}

impl From<EncodeError> for CompactError {
    fn from(e: EncodeError) -> Self {
        CompactError::Encode(e)
    }
}

impl core::fmt::Display for CompactError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            CompactError::UnknownSchema(id) => write!(f, "unknown schema {id}"),
            CompactError::Unsupported(what) => write!(f, "compact codec does not support {what} yet"),
            CompactError::TypeMismatch { expected } => {
                write!(f, "value does not match schema (expected {expected})")
            }
            CompactError::UnknownVariant(name) => write!(f, "unknown enum variant {name:?}"),
            CompactError::BadVariantIndex(i) => write!(f, "enum variant index {i} out of range"),
            CompactError::GenericArity { params, args } => {
                write!(f, "generic expects {params} type arguments, got {args}")
            }
            CompactError::Malformed(what) => write!(f, "malformed schema: {what}"),
            CompactError::Decode(e) => write!(f, "decode: {e}"),
            CompactError::Encode(e) => write!(f, "encode: {e}"),
        }
    }
}

impl std::error::Error for CompactError {}

type Result<T> = core::result::Result<T, CompactError>;

// ============================================================================
// Registry
// ============================================================================

/// A resolved schema closure: composite schemas by id, plus intrinsic
/// recognition of the primitive ids.
pub struct Registry {
    composites: HashMap<SchemaId, Schema>,
    primitives: HashMap<SchemaId, Primitive>,
}

impl Registry {
    /// Build a registry from a closure of composite schemas. Primitive schemas
    /// need not be supplied — they are recognized by their canonical id.
    #[must_use]
    pub fn new(schemas: impl IntoIterator<Item = Schema>) -> Self {
        let primitives = Primitive::ALL
            .iter()
            .map(|&p| (primitive_id(p), p))
            .collect();
        let composites = schemas.into_iter().map(|s| (s.id, s)).collect();
        Registry {
            composites,
            primitives,
        }
    }

    fn primitive(&self, id: SchemaId) -> Option<Primitive> {
        self.primitives.get(&id).copied()
    }

    fn composite(&self, id: SchemaId) -> Option<&Schema> {
        self.composites.get(&id)
    }
}

// ============================================================================
// Alignment
// ============================================================================

// r[impl compact.alignment]
fn alignment(p: Primitive) -> usize {
    match p {
        Primitive::U16 | Primitive::I16 => 2,
        Primitive::U32 | Primitive::I32 | Primitive::F32 | Primitive::Char => 4,
        Primitive::U64 | Primitive::I64 | Primitive::F64 => 8,
        Primitive::U128 | Primitive::I128 => 16,
        _ => 1,
    }
}

fn pad_to(out: &mut Vec<u8>, n: usize) {
    while !out.len().is_multiple_of(n) {
        out.push(0);
    }
}

fn skip_pad(r: &mut Reader, n: usize) -> core::result::Result<(), DecodeError> {
    while !r.position().is_multiple_of(n) {
        r.read_u8()?;
    }
    Ok(())
}

// ============================================================================
// Public API
// ============================================================================

/// Encode `value` against the schema named by `root` in `registry`.
///
/// # Errors
/// [`CompactError`] if the value does not match the schema, a referenced schema
/// is missing, or the codec does not yet support a kind in play.
pub fn to_bytes(value: &Value, root: SchemaId, registry: &Registry) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    encode_ref(value, &SchemaRef::concrete(root), registry, &mut out)?;
    Ok(out)
}

/// Decode a value of schema `root` from `bytes`, rejecting trailing bytes.
///
/// # Errors
/// [`CompactError`] for malformed input or an unsupported kind.
pub fn from_bytes(bytes: &[u8], root: SchemaId, registry: &Registry) -> Result<Value> {
    let mut r = Reader::new(bytes);
    let v = decode_ref(&mut r, &SchemaRef::concrete(root), registry, 0)?;
    if r.remaining() != 0 {
        return Err(CompactError::Decode(DecodeError::TrailingBytes(r.remaining())));
    }
    Ok(v)
}

// ============================================================================
// Encoding
// ============================================================================

// r[impl type-system.generic-resolution]
fn encode_ref(value: &Value, r: &SchemaRef, reg: &Registry, out: &mut Vec<u8>) -> Result<()> {
    match r {
        SchemaRef::Var { .. } => Err(CompactError::Malformed("unbound type variable")),
        SchemaRef::Concrete { id, args } => {
            if let Some(p) = reg.primitive(*id) {
                if !args.is_empty() {
                    return Err(CompactError::Malformed("primitive carrying type arguments"));
                }
                encode_primitive(value, p, out)
            } else if let Some(schema) = reg.composite(*id) {
                if schema.type_params.len() != args.len() {
                    return Err(CompactError::GenericArity {
                        params: schema.type_params.len(),
                        args: args.len(),
                    });
                }
                if args.is_empty() {
                    encode_kind(value, &schema.kind, reg, out)
                } else {
                    let kind = substitute_kind(&schema.kind, &schema.type_params, args);
                    encode_kind(value, &kind, reg, out)
                }
            } else {
                Err(CompactError::UnknownSchema(*id))
            }
        }
    }
}

fn number(value: &Value) -> Result<&facet_value::VNumber> {
    value
        .as_number()
        .ok_or(CompactError::TypeMismatch { expected: "number" })
}

fn encode_primitive(value: &Value, p: Primitive, out: &mut Vec<u8>) -> Result<()> {
    pad_to(out, alignment(p));
    match p {
        Primitive::Bool => write_bool(
            out,
            value
                .as_bool()
                .ok_or(CompactError::TypeMismatch { expected: "bool" })?,
        ),
        Primitive::U8 => write_u8(out, number(value)?.to_u64().unwrap_or(0) as u8),
        Primitive::U16 => write_u16(out, number(value)?.to_u64().unwrap_or(0) as u16),
        Primitive::U32 => write_u32(out, number(value)?.to_u64().unwrap_or(0) as u32),
        Primitive::U64 => write_u64(out, number(value)?.to_u64().unwrap_or(0)),
        Primitive::U128 => write_u128(out, number(value)?.to_u128().unwrap_or(0)),
        Primitive::I8 => write_i8(out, number(value)?.to_i64().unwrap_or(0) as i8),
        Primitive::I16 => write_i16(out, number(value)?.to_i64().unwrap_or(0) as i16),
        Primitive::I32 => write_i32(out, number(value)?.to_i64().unwrap_or(0) as i32),
        Primitive::I64 => write_i64(out, number(value)?.to_i64().unwrap_or(0)),
        Primitive::I128 => write_i128(out, number(value)?.to_i128().unwrap_or(0)),
        Primitive::F32 => write_f32(out, number(value)?.to_f64_lossy() as f32),
        Primitive::F64 => write_f64(out, number(value)?.to_f64_lossy()),
        Primitive::Char => write_u32(
            out,
            value
                .as_char()
                .ok_or(CompactError::TypeMismatch { expected: "char" })? as u32,
        ),
        Primitive::String => {
            let s = value
                .as_string()
                .ok_or(CompactError::TypeMismatch { expected: "string" })?;
            write_u32(out, s.as_str().len() as u32);
            out.extend_from_slice(s.as_str().as_bytes());
        }
        Primitive::Bytes => {
            let b = value
                .as_bytes()
                .ok_or(CompactError::TypeMismatch { expected: "bytes" })?;
            write_u32(out, b.as_slice().len() as u32);
            out.extend_from_slice(b.as_slice());
        }
        Primitive::Unit => {
            if !value.is_null() {
                return Err(CompactError::TypeMismatch { expected: "unit" });
            }
        }
        Primitive::Never => return Err(CompactError::TypeMismatch { expected: "never" }),
        Primitive::DateTime | Primitive::Uuid | Primitive::QName => {
            let s = extended_to_string(value, p).map_err(CompactError::Encode)?;
            write_u32(out, s.len() as u32);
            out.extend_from_slice(s.as_bytes());
        }
    }
    Ok(())
}

// r[impl compact.schema-driven]
fn encode_kind(value: &Value, kind: &SchemaKind, reg: &Registry, out: &mut Vec<u8>) -> Result<()> {
    match kind {
        SchemaKind::Primitive(p) => encode_primitive(value, *p, out),
        SchemaKind::Struct { fields, .. } => {
            let obj = value
                .as_object()
                .ok_or(CompactError::TypeMismatch { expected: "object" })?;
            for field in fields {
                let fv = obj
                    .get(&VString::new(&field.name))
                    .ok_or(CompactError::TypeMismatch { expected: "struct field" })?;
                encode_ref(fv, &field.schema, reg, out)?;
            }
            Ok(())
        }
        SchemaKind::Tuple { elements } => {
            let arr = value
                .as_array()
                .ok_or(CompactError::TypeMismatch { expected: "tuple" })?;
            if arr.len() != elements.len() {
                return Err(CompactError::TypeMismatch { expected: "tuple arity" });
            }
            for (i, e) in elements.iter().enumerate() {
                encode_ref(arr.get(i).unwrap(), e, reg, out)?;
            }
            Ok(())
        }
        SchemaKind::List { element } | SchemaKind::Set { element } => {
            let arr = value
                .as_array()
                .ok_or(CompactError::TypeMismatch { expected: "list" })?;
            write_u32(out, arr.len() as u32);
            for i in 0..arr.len() {
                encode_ref(arr.get(i).unwrap(), element, reg, out)?;
            }
            Ok(())
        }
        SchemaKind::Array {
            element,
            dimensions,
        } => {
            let count = product(dimensions)?;
            let arr = value
                .as_array()
                .ok_or(CompactError::TypeMismatch { expected: "array" })?;
            if arr.len() as u64 != count {
                return Err(CompactError::TypeMismatch { expected: "array shape" });
            }
            for i in 0..arr.len() {
                encode_ref(arr.get(i).unwrap(), element, reg, out)?;
            }
            Ok(())
        }
        SchemaKind::Map { key, value: val } => {
            let obj = value
                .as_object()
                .ok_or(CompactError::TypeMismatch { expected: "map" })?;
            write_u32(out, obj.len() as u32);
            for (k, v) in obj.iter() {
                encode_ref(&Value::from(VString::new(k.as_str())), key, reg, out)?;
                encode_ref(v, val, reg, out)?;
            }
            Ok(())
        }
        SchemaKind::Option { element } => {
            if value.is_null() {
                write_u8(out, 0);
            } else {
                write_u8(out, 1);
                encode_ref(value, element, reg, out)?;
            }
            Ok(())
        }
        SchemaKind::Enum { variants, .. } => {
            let obj = value
                .as_object()
                .ok_or(CompactError::TypeMismatch { expected: "enum object" })?;
            if obj.len() != 1 {
                return Err(CompactError::TypeMismatch { expected: "single-variant enum object" });
            }
            let (name, payload) = obj.iter().next().unwrap();
            let variant = variants
                .iter()
                .find(|v| v.name == name.as_str())
                .ok_or_else(|| CompactError::UnknownVariant(name.as_str().to_string()))?;
            write_u32(out, variant.index);
            encode_payload(payload, &variant.payload, reg, out)
        }
        SchemaKind::Dynamic => {
            write_value(out, value)?;
            Ok(())
        }
        SchemaKind::Tensor { .. } => Err(CompactError::Unsupported("tensor")),
        SchemaKind::Channel { .. } => Err(CompactError::Unsupported("channel")),
        SchemaKind::External { .. } => Err(CompactError::Unsupported("external")),
    }
}

fn encode_payload(
    value: &Value,
    payload: &VariantPayload,
    reg: &Registry,
    out: &mut Vec<u8>,
) -> Result<()> {
    match payload {
        VariantPayload::Unit => Ok(()),
        VariantPayload::Newtype(r) => encode_ref(value, r, reg, out),
        VariantPayload::Tuple(refs) => {
            let arr = value
                .as_array()
                .ok_or(CompactError::TypeMismatch { expected: "tuple variant" })?;
            if arr.len() != refs.len() {
                return Err(CompactError::TypeMismatch { expected: "tuple variant arity" });
            }
            for (i, r) in refs.iter().enumerate() {
                encode_ref(arr.get(i).unwrap(), r, reg, out)?;
            }
            Ok(())
        }
        VariantPayload::Struct(fields) => {
            let obj = value
                .as_object()
                .ok_or(CompactError::TypeMismatch { expected: "struct variant" })?;
            for field in fields {
                let fv = obj
                    .get(&VString::new(&field.name))
                    .ok_or(CompactError::TypeMismatch { expected: "struct variant field" })?;
                encode_ref(fv, &field.schema, reg, out)?;
            }
            Ok(())
        }
    }
}

fn product(dimensions: &[u64]) -> Result<u64> {
    let mut p: u64 = 1;
    for &d in dimensions {
        p = p
            .checked_mul(d)
            .ok_or(CompactError::Decode(DecodeError::Malformed("array dimensions overflow")))?;
    }
    Ok(p)
}

// ============================================================================
// Generic resolution
// ============================================================================
//
// Resolving a parametric schema substitutes its type parameters with the
// arguments from a concrete reference, throughout its kind. Substitution is
// eager and per-reference: each `Concrete { id, args }` produces a Var-free kind
// before it is walked, so the walker never meets a `Var` (`r[type-system.generic-resolution]`).

fn substitute_ref(r: &SchemaRef, params: &[String], args: &[SchemaRef]) -> SchemaRef {
    match r {
        SchemaRef::Var { name } => params
            .iter()
            .position(|p| p == name)
            .map(|i| args[i].clone())
            .unwrap_or_else(|| r.clone()),
        SchemaRef::Concrete { id, args: inner } => SchemaRef::Concrete {
            id: *id,
            args: inner
                .iter()
                .map(|a| substitute_ref(a, params, args))
                .collect(),
        },
    }
}

fn substitute_field(f: &Field, params: &[String], args: &[SchemaRef]) -> Field {
    Field {
        name: f.name.clone(),
        schema: substitute_ref(&f.schema, params, args),
        required: f.required,
    }
}

fn substitute_kind(kind: &SchemaKind, params: &[String], args: &[SchemaRef]) -> SchemaKind {
    match kind {
        SchemaKind::Primitive(p) => SchemaKind::Primitive(*p),
        SchemaKind::Dynamic => SchemaKind::Dynamic,
        SchemaKind::Struct { name, fields } => SchemaKind::Struct {
            name: name.clone(),
            fields: fields
                .iter()
                .map(|f| substitute_field(f, params, args))
                .collect(),
        },
        SchemaKind::Enum { name, variants } => SchemaKind::Enum {
            name: name.clone(),
            variants: variants
                .iter()
                .map(|v| Variant {
                    name: v.name.clone(),
                    index: v.index,
                    payload: match &v.payload {
                        VariantPayload::Unit => VariantPayload::Unit,
                        VariantPayload::Newtype(r) => {
                            VariantPayload::Newtype(substitute_ref(r, params, args))
                        }
                        VariantPayload::Tuple(rs) => VariantPayload::Tuple(
                            rs.iter().map(|r| substitute_ref(r, params, args)).collect(),
                        ),
                        VariantPayload::Struct(fs) => VariantPayload::Struct(
                            fs.iter().map(|f| substitute_field(f, params, args)).collect(),
                        ),
                    },
                })
                .collect(),
        },
        SchemaKind::Tuple { elements } => SchemaKind::Tuple {
            elements: elements
                .iter()
                .map(|r| substitute_ref(r, params, args))
                .collect(),
        },
        SchemaKind::List { element } => SchemaKind::List {
            element: substitute_ref(element, params, args),
        },
        SchemaKind::Set { element } => SchemaKind::Set {
            element: substitute_ref(element, params, args),
        },
        SchemaKind::Option { element } => SchemaKind::Option {
            element: substitute_ref(element, params, args),
        },
        SchemaKind::Map { key, value } => SchemaKind::Map {
            key: substitute_ref(key, params, args),
            value: substitute_ref(value, params, args),
        },
        SchemaKind::Array {
            element,
            dimensions,
        } => SchemaKind::Array {
            element: substitute_ref(element, params, args),
            dimensions: dimensions.clone(),
        },
        SchemaKind::Tensor { element, rank } => SchemaKind::Tensor {
            element: substitute_ref(element, params, args),
            rank: *rank,
        },
        SchemaKind::Channel { direction, element } => SchemaKind::Channel {
            direction: *direction,
            element: substitute_ref(element, params, args),
        },
        SchemaKind::External { kind, metadata } => SchemaKind::External {
            kind: kind.clone(),
            metadata: metadata.as_ref().map(|r| substitute_ref(r, params, args)),
        },
    }
}

// ============================================================================
// Decoding
// ============================================================================

fn decode_ref(r: &mut Reader, rf: &SchemaRef, reg: &Registry, depth: usize) -> Result<Value> {
    if depth > MAX_DEPTH {
        return Err(CompactError::Decode(DecodeError::DepthExceeded));
    }
    match rf {
        SchemaRef::Var { .. } => Err(CompactError::Malformed("unbound type variable")),
        SchemaRef::Concrete { id, args } => {
            if let Some(p) = reg.primitive(*id) {
                if !args.is_empty() {
                    return Err(CompactError::Malformed("primitive carrying type arguments"));
                }
                decode_primitive(r, p)
            } else if let Some(schema) = reg.composite(*id) {
                if schema.type_params.len() != args.len() {
                    return Err(CompactError::GenericArity {
                        params: schema.type_params.len(),
                        args: args.len(),
                    });
                }
                if args.is_empty() {
                    decode_kind(r, &schema.kind, reg, depth + 1)
                } else {
                    let kind = substitute_kind(&schema.kind, &schema.type_params, args);
                    decode_kind(r, &kind, reg, depth + 1)
                }
            } else {
                Err(CompactError::UnknownSchema(*id))
            }
        }
    }
}

fn decode_primitive(r: &mut Reader, p: Primitive) -> Result<Value> {
    skip_pad(r, alignment(p))?;
    Ok(match p {
        Primitive::Bool => Value::from(r.read_bool()?),
        Primitive::U8 => Value::from(r.read_u8()?),
        Primitive::U16 => Value::from(r.read_u16()?),
        Primitive::U32 => Value::from(r.read_u32()?),
        Primitive::U64 => Value::from(r.read_u64()?),
        Primitive::U128 => Value::from(r.read_u128()?),
        Primitive::I8 => Value::from(r.read_i8()?),
        Primitive::I16 => Value::from(r.read_i16()?),
        Primitive::I32 => Value::from(r.read_i32()?),
        Primitive::I64 => Value::from(r.read_i64()?),
        Primitive::I128 => Value::from(r.read_i128()?),
        Primitive::F32 => Value::from(r.read_f32()?),
        Primitive::F64 => Value::from(r.read_f64()?),
        Primitive::Char => Value::from(r.read_char()?),
        Primitive::String => VString::new(r.read_str()?).into(),
        Primitive::Bytes => VBytes::new(r.read_bytes()?).into(),
        Primitive::Unit => Value::NULL,
        Primitive::Never => {
            return Err(CompactError::Decode(DecodeError::Malformed("never is uninhabited")));
        }
        Primitive::DateTime | Primitive::Uuid | Primitive::QName => {
            extended_from_string(r.read_str()?, p).map_err(CompactError::Decode)?
        }
    })
}

fn decode_kind(r: &mut Reader, kind: &SchemaKind, reg: &Registry, depth: usize) -> Result<Value> {
    match kind {
        SchemaKind::Primitive(p) => decode_primitive(r, *p),
        SchemaKind::Struct { fields, .. } => {
            let mut obj = VObject::new();
            for field in fields {
                let fv = decode_ref(r, &field.schema, reg, depth)?;
                obj.insert(VString::new(&field.name), fv);
            }
            Ok(obj.into())
        }
        SchemaKind::Tuple { elements } => {
            let mut arr = VArray::new();
            for e in elements {
                arr.push(decode_ref(r, e, reg, depth)?);
            }
            Ok(arr.into())
        }
        SchemaKind::List { element } => {
            let n = r.read_len(1)?;
            let mut arr = VArray::new();
            for _ in 0..n {
                arr.push(decode_ref(r, element, reg, depth)?);
            }
            Ok(arr.into())
        }
        SchemaKind::Set { element } => {
            let n = r.read_len(1)?;
            let mut arr = VArray::new();
            let mut seen = std::collections::HashSet::new();
            for _ in 0..n {
                let v = decode_ref(r, element, reg, depth)?;
                if !seen.insert(v.clone()) {
                    return Err(CompactError::Decode(DecodeError::DuplicateElement));
                }
                arr.push(v);
            }
            Ok(arr.into())
        }
        SchemaKind::Array {
            element,
            dimensions,
        } => {
            let count = product(dimensions)?;
            if count > r.remaining() as u64 {
                return Err(CompactError::Decode(DecodeError::LengthTooLarge {
                    count,
                    remaining: r.remaining(),
                }));
            }
            let mut arr = VArray::new();
            for _ in 0..count {
                arr.push(decode_ref(r, element, reg, depth)?);
            }
            Ok(arr.into())
        }
        SchemaKind::Map { key, value } => {
            let n = r.read_len(1)?;
            let mut obj = VObject::new();
            for _ in 0..n {
                let k = decode_ref(r, key, reg, depth)?;
                let v = decode_ref(r, value, reg, depth)?;
                let ks = k
                    .as_string()
                    .ok_or(CompactError::Unsupported("map with non-string keys"))?;
                if obj.insert(VString::new(ks.as_str()), v).is_some() {
                    return Err(CompactError::Decode(DecodeError::DuplicateKey));
                }
            }
            Ok(obj.into())
        }
        SchemaKind::Option { element } => match r.read_u8()? {
            0 => Ok(Value::NULL),
            1 => decode_ref(r, element, reg, depth),
            b => Err(CompactError::Decode(DecodeError::InvalidBool(b))),
        },
        SchemaKind::Enum { variants, .. } => {
            let index = r.read_u32()?;
            let variant = variants
                .iter()
                .find(|v| v.index == index)
                .ok_or(CompactError::BadVariantIndex(index))?;
            let payload = decode_payload(r, &variant.payload, reg, depth)?;
            let mut obj = VObject::new();
            obj.insert(VString::new(&variant.name), payload);
            Ok(obj.into())
        }
        SchemaKind::Dynamic => Ok(read_value(r)?),
        SchemaKind::Tensor { .. } => Err(CompactError::Unsupported("tensor")),
        SchemaKind::Channel { .. } => Err(CompactError::Unsupported("channel")),
        SchemaKind::External { .. } => Err(CompactError::Unsupported("external")),
    }
}

fn decode_payload(
    r: &mut Reader,
    payload: &VariantPayload,
    reg: &Registry,
    depth: usize,
) -> Result<Value> {
    match payload {
        VariantPayload::Unit => Ok(Value::NULL),
        VariantPayload::Newtype(rf) => decode_ref(r, rf, reg, depth),
        VariantPayload::Tuple(refs) => {
            let mut arr = VArray::new();
            for rf in refs {
                arr.push(decode_ref(r, rf, reg, depth)?);
            }
            Ok(arr.into())
        }
        VariantPayload::Struct(fields) => {
            let mut obj = VObject::new();
            for field in fields {
                let fv = decode_ref(r, &field.schema, reg, depth)?;
                obj.insert(VString::new(&field.name), fv);
            }
            Ok(obj.into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use phon_schema::{Field, Schema, SchemaKind, SchemaRef, Variant};

    fn prim(p: Primitive) -> SchemaRef {
        SchemaRef::concrete(primitive_id(p))
    }

    fn schema(id: u64, kind: SchemaKind) -> Schema {
        Schema {
            id: SchemaId(id),
            type_params: Vec::new(),
            kind,
        }
    }

    fn rt(value: Value, root: SchemaId, reg: &Registry) {
        let bytes = to_bytes(&value, root, reg).expect("encode");
        let back = from_bytes(&bytes, root, reg).expect("decode");
        assert_eq!(value, back);
        assert_eq!(to_bytes(&back, root, reg).unwrap(), bytes);
    }

    #[test]
    fn struct_with_alignment() {
        // Point { x: u32, y: f64 } — y is 8-aligned, so padding follows x.
        let point = schema(
            1,
            SchemaKind::Struct {
                name: "Point".to_string(),
                fields: vec![
                    Field {
                        name: "x".to_string(),
                        schema: prim(Primitive::U32),
                        required: true,
                    },
                    Field {
                        name: "y".to_string(),
                        schema: prim(Primitive::F64),
                        required: true,
                    },
                ],
            },
        );
        let reg = Registry::new([point]);
        let mut obj = VObject::new();
        obj.insert(VString::new("x"), Value::from(7u32));
        obj.insert(VString::new("y"), Value::from(2.5f64));
        let value: Value = obj.into();

        let bytes = to_bytes(&value, SchemaId(1), &reg).unwrap();
        // 4 bytes x, 4 bytes padding (to 8-align y), 8 bytes y.
        assert_eq!(bytes.len(), 16);
        assert_eq!(&bytes[4..8], &[0, 0, 0, 0]);
        rt(value, SchemaId(1), &reg);
    }

    #[test]
    fn list_run_is_aligned() {
        let list = schema(
            1,
            SchemaKind::List {
                element: prim(Primitive::U64),
            },
        );
        let reg = Registry::new([list]);
        let mut arr = VArray::new();
        arr.push(Value::from(1u64));
        arr.push(Value::from(2u64));
        let value: Value = arr.into();
        let bytes = to_bytes(&value, SchemaId(1), &reg).unwrap();
        // u32 count, 4 bytes pad to 8, then two contiguous u64s.
        assert_eq!(bytes.len(), 4 + 4 + 16);
        rt(value, SchemaId(1), &reg);
    }

    #[test]
    fn enum_tuple_option_array() {
        // enum E { A, B(u32), C(u8, u8) }
        let e = schema(
            1,
            SchemaKind::Enum {
                name: "E".to_string(),
                variants: vec![
                    Variant {
                        name: "A".to_string(),
                        index: 0,
                        payload: VariantPayload::Unit,
                    },
                    Variant {
                        name: "B".to_string(),
                        index: 1,
                        payload: VariantPayload::Newtype(prim(Primitive::U32)),
                    },
                    Variant {
                        name: "C".to_string(),
                        index: 2,
                        payload: VariantPayload::Tuple(vec![prim(Primitive::U8), prim(Primitive::U8)]),
                    },
                ],
            },
        );
        let opt = schema(
            2,
            SchemaKind::Option {
                element: SchemaRef::concrete(SchemaId(1)),
            },
        );
        let arr = schema(
            3,
            SchemaKind::Array {
                element: prim(Primitive::U16),
                dimensions: vec![3],
            },
        );
        let reg = Registry::new([e.clone(), opt, arr]);

        // E::A
        let mut a = VObject::new();
        a.insert(VString::new("A"), Value::NULL);
        rt(a.into(), SchemaId(1), &reg);
        // E::B(42)
        let mut b = VObject::new();
        b.insert(VString::new("B"), Value::from(42u32));
        rt(b.into(), SchemaId(1), &reg);
        // E::C(1, 2)
        let mut cpay = VArray::new();
        cpay.push(Value::from(1u8));
        cpay.push(Value::from(2u8));
        let mut c = VObject::new();
        c.insert(VString::new("C"), Value::from(cpay));
        rt(c.into(), SchemaId(1), &reg);

        // Option<E> = Some(E::A), None
        let mut some_inner = VObject::new();
        some_inner.insert(VString::new("A"), Value::NULL);
        rt(some_inner.into(), SchemaId(2), &reg);
        rt(Value::NULL, SchemaId(2), &reg);

        // Array<u16, 3>
        let mut av = VArray::new();
        av.push(Value::from(10u16));
        av.push(Value::from(20u16));
        av.push(Value::from(30u16));
        rt(av.into(), SchemaId(3), &reg);
    }

    #[test]
    fn map_and_set_and_dynamic() {
        let map = schema(
            1,
            SchemaKind::Map {
                key: prim(Primitive::String),
                value: prim(Primitive::U32),
            },
        );
        let set = schema(
            2,
            SchemaKind::Set {
                element: prim(Primitive::U32),
            },
        );
        let dynamic = schema(3, SchemaKind::Dynamic);
        let reg = Registry::new([map, set, dynamic]);

        let mut m = VObject::new();
        m.insert(VString::new("a"), Value::from(1u32));
        m.insert(VString::new("b"), Value::from(2u32));
        rt(m.into(), SchemaId(1), &reg);

        let mut s = VArray::new();
        s.push(Value::from(1u32));
        s.push(Value::from(2u32));
        rt(s.into(), SchemaId(2), &reg);

        // Dynamic carries an arbitrary value self-describing.
        rt(Value::from("hello dynamic"), SchemaId(3), &reg);
    }

    #[test]
    fn unknown_schema_and_type_mismatch() {
        let reg = Registry::new([]);
        // u32 primitive is intrinsic; a bogus composite id is unknown.
        assert!(matches!(
            to_bytes(&Value::from(1u32), SchemaId(999), &reg),
            Err(CompactError::UnknownSchema(_))
        ));
        // a string where a u32 is expected
        assert!(matches!(
            to_bytes(&Value::from("x"), primitive_id(Primitive::U32), &reg),
            Err(CompactError::TypeMismatch { .. })
        ));
    }

    #[test]
    fn generics_resolve() {
        // Pair<A, B> = (A, B); Holder<T> = { pair: Pair<T, u32>, tag: string };
        // Root = { h: Holder<u8> } (concrete).
        let pair = Schema {
            id: SchemaId(10),
            type_params: vec!["A".to_string(), "B".to_string()],
            kind: SchemaKind::Tuple {
                elements: vec![SchemaRef::var("A"), SchemaRef::var("B")],
            },
        };
        let holder = Schema {
            id: SchemaId(11),
            type_params: vec!["T".to_string()],
            kind: SchemaKind::Struct {
                name: "Holder".to_string(),
                fields: vec![
                    Field {
                        name: "pair".to_string(),
                        schema: SchemaRef::generic(
                            SchemaId(10),
                            vec![SchemaRef::var("T"), prim(Primitive::U32)],
                        ),
                        required: true,
                    },
                    Field {
                        name: "tag".to_string(),
                        schema: prim(Primitive::String),
                        required: true,
                    },
                ],
            },
        };
        let root = schema(
            12,
            SchemaKind::Struct {
                name: "Root".to_string(),
                fields: vec![Field {
                    name: "h".to_string(),
                    schema: SchemaRef::generic(SchemaId(11), vec![prim(Primitive::U8)]),
                    required: true,
                }],
            },
        );
        let reg = Registry::new([pair, holder, root]);

        let mut pair_val = VArray::new();
        pair_val.push(Value::from(5u8));
        pair_val.push(Value::from(70_000u32));
        let mut holder_val = VObject::new();
        holder_val.insert(VString::new("pair"), Value::from(pair_val));
        holder_val.insert(VString::new("tag"), VString::new("hi"));
        let mut root_val = VObject::new();
        root_val.insert(VString::new("h"), Value::from(holder_val));

        rt(root_val.into(), SchemaId(12), &reg);

        // wrong arity is rejected
        let bad = schema(
            13,
            SchemaKind::Struct {
                name: "Bad".to_string(),
                fields: vec![Field {
                    name: "h".to_string(),
                    schema: SchemaRef::concrete(SchemaId(11)), // Holder needs 1 arg
                    required: true,
                }],
            },
        );
        let reg2 = Registry::new([
            Schema {
                id: SchemaId(11),
                type_params: vec!["T".to_string()],
                kind: SchemaKind::Option {
                    element: SchemaRef::var("T"),
                },
            },
            bad,
        ]);
        let mut bv = VObject::new();
        bv.insert(VString::new("h"), Value::NULL);
        assert!(matches!(
            to_bytes(&bv.into(), SchemaId(13), &reg2),
            Err(CompactError::GenericArity { .. })
        ));
    }

    #[test]
    fn extended_primitives() {
        use facet_value::{VDateTime, VQName, VUuid};
        let reg = Registry::new([]);
        rt(
            VUuid::from_u128(0x0123_4567_89ab_cdef_fedc_ba98_7654_3210).into(),
            primitive_id(Primitive::Uuid),
            &reg,
        );
        rt(
            VQName::new(VString::new("http://ns"), VString::new("el")).into(),
            primitive_id(Primitive::QName),
            &reg,
        );
        rt(
            VDateTime::new_offset(2026, 5, 29, 7, 32, 0, 0, 330).into(),
            primitive_id(Primitive::DateTime),
            &reg,
        );
        rt(
            VDateTime::new_local_date(2026, 5, 29).into(),
            primitive_id(Primitive::DateTime),
            &reg,
        );
    }
}
