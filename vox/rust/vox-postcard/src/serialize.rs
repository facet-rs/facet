use facet::Facet;
use facet_core::{Def, ScalarType, StructKind, Type, UserType};
use facet_reflect::Peek;
use facet_value::{Value, ValueType};
use std::sync::OnceLock;

use crate::encode;
use crate::error::SerializeError;

pub type RuntimeEncodeHook =
    fn(facet::PtrConst, &'static facet::Shape) -> Result<Option<Vec<u8>>, SerializeError>;

static RUNTIME_ENCODE_HOOK: OnceLock<RuntimeEncodeHook> = OnceLock::new();

pub fn set_runtime_encode_hook(hook: RuntimeEncodeHook) {
    let _ = RUNTIME_ENCODE_HOOK.set(hook);
}

fn try_runtime_encode(
    ptr: facet::PtrConst,
    shape: &'static facet::Shape,
) -> Result<Option<Vec<u8>>, SerializeError> {
    if let Some(hook) = RUNTIME_ENCODE_HOOK.get() {
        hook(ptr, shape)
    } else {
        Ok(None)
    }
}

/// Handle to a reserved u32le size field that can be patched after serialization.
#[derive(Debug, Clone, Copy)]
pub struct SizeField(pub(crate) usize);

/// Trait for writing structural bytes during serialization.
pub trait Writer {
    /// Write a single byte.
    fn write_byte(&mut self, byte: u8);

    /// Write structural/metadata bytes (always copied).
    fn write_bytes(&mut self, bytes: &[u8]);

    /// Total bytes written so far.
    fn bytes_written(&self) -> usize;

    /// Reserve 4 bytes for a u32le size field, returning a handle to patch later.
    fn reserve_size_field(&mut self) -> SizeField;

    /// Patch a previously reserved size field with the actual value.
    fn write_size_field(&mut self, handle: SizeField, value: u32);
}

impl Writer for Vec<u8> {
    fn write_byte(&mut self, byte: u8) {
        self.push(byte);
    }

    fn write_bytes(&mut self, bytes: &[u8]) {
        self.extend_from_slice(bytes);
    }

    fn bytes_written(&self) -> usize {
        self.len()
    }

    fn reserve_size_field(&mut self) -> SizeField {
        let offset = self.len();
        self.extend_from_slice(&[0u8; 4]);
        SizeField(offset)
    }

    fn write_size_field(&mut self, handle: SizeField, value: u32) {
        self[handle.0..handle.0 + 4].copy_from_slice(&value.to_le_bytes());
    }
}

/// Extends `Writer` with zero-copy support for payload bytes borrowed from
/// the source value.
///
/// `ScatterBuilder<'a>` keeps references; `CopyWriter` copies them.
pub(crate) trait PostcardWriter<'a>: Writer {
    /// Write payload bytes that are borrowed from the source value for lifetime `'a`.
    fn write_referenced_bytes(&mut self, bytes: &'a [u8]);
}

/// Wraps any `Writer` and copies referenced bytes instead of keeping references.
pub(crate) struct CopyWriter<'w, W: Writer + ?Sized> {
    inner: &'w mut W,
}

impl<'w, W: Writer + ?Sized> CopyWriter<'w, W> {
    pub(crate) fn new(inner: &'w mut W) -> Self {
        Self { inner }
    }
}

impl<W: Writer + ?Sized> Writer for CopyWriter<'_, W> {
    fn write_byte(&mut self, byte: u8) {
        self.inner.write_byte(byte);
    }

    fn write_bytes(&mut self, bytes: &[u8]) {
        self.inner.write_bytes(bytes);
    }

    fn bytes_written(&self) -> usize {
        self.inner.bytes_written()
    }

    fn reserve_size_field(&mut self) -> SizeField {
        self.inner.reserve_size_field()
    }

    fn write_size_field(&mut self, handle: SizeField, value: u32) {
        self.inner.write_size_field(handle, value);
    }
}

impl<'a, W: Writer + ?Sized> PostcardWriter<'a> for CopyWriter<'_, W> {
    fn write_referenced_bytes(&mut self, bytes: &'a [u8]) {
        self.inner.write_bytes(bytes);
    }
}

/// Serialize any `Facet` type to postcard bytes.
///
/// Serialization always uses the local type definition — no translation plan.
/// The receiver adapts to the sender's layout, not the other way around.
// r[impl schema.translation.serialization-unchanged]
pub fn to_vec<'a, T: Facet<'a>>(value: &T) -> Result<Vec<u8>, SerializeError> {
    let peek = Peek::new(value);
    let mut out = Vec::new();
    serialize_peek(peek, &mut CopyWriter::new(&mut out))?;
    Ok(out)
}

/// Serialize a dynamically-shaped value to postcard bytes.
///
/// The caller must ensure `ptr` points to a valid value matching `shape`.
pub fn to_vec_dynamic(
    ptr: facet::PtrConst,
    shape: &'static facet::Shape,
) -> Result<Vec<u8>, SerializeError> {
    #[allow(unsafe_code)]
    let peek = unsafe { Peek::unchecked_new(ptr, shape) };
    let mut out = Vec::new();
    serialize_peek(peek, &mut CopyWriter::new(&mut out))?;
    Ok(out)
}

/// Serialize a `Peek` value to postcard, appending to the writer.
pub(crate) fn serialize_peek<'a>(
    peek: Peek<'a, '_>,
    out: &mut impl PostcardWriter<'a>,
) -> Result<(), SerializeError> {
    serialize_peek_inner(peek, out)
}

fn serialize_peek_inner<'a>(
    peek: Peek<'a, '_>,
    out: &mut impl PostcardWriter<'a>,
) -> Result<(), SerializeError> {
    let peek = peek.innermost_peek();
    fn re(e: impl std::fmt::Display) -> SerializeError {
        SerializeError::ReflectError(e.to_string())
    }

    // Handle proxy types (e.g. Rx<T>, Tx<T> with #[facet(proxy = ())])
    if let Some(proxy_def) = peek.shape().proxy {
        let proxy_shape = proxy_def.shape;
        let proxy_layout = proxy_shape
            .layout
            .sized_layout()
            .map_err(|_| SerializeError::ReflectError("proxy type must be sized".into()))?;

        let proxy_uninit = facet_core::alloc_for_layout(proxy_layout);
        #[allow(unsafe_code)]
        let proxy_ptr = unsafe { (proxy_def.convert_out)(peek.data(), proxy_uninit) }
            .map_err(SerializeError::ReflectError)?;
        #[allow(unsafe_code)]
        let proxy_peek = unsafe { Peek::unchecked_new(proxy_ptr.as_const(), proxy_shape) };

        let result = serialize_peek_inner(proxy_peek, out);

        #[allow(unsafe_code)]
        unsafe {
            let _ = proxy_shape.call_drop_in_place(proxy_ptr);
            facet_core::dealloc_for_layout(proxy_ptr, proxy_layout);
        }

        return result;
    }

    // r[impl zerocopy.framing.value.opaque]
    // r[impl zerocopy.framing.value.opaque.length-prefix]
    // Handle opaque adapters (e.g. Payload). Length-prefixed with u32le.
    if let Some(adapter) = peek.shape().opaque_adapter {
        #[allow(unsafe_code)]
        let mapped = unsafe { (adapter.serialize)(peek.data()) };
        // Check if this is already-encoded postcard bytes (passthrough)
        #[allow(unsafe_code)]
        if let Some(bytes) =
            unsafe { crate::raw::try_decode_passthrough_bytes(mapped.ptr, mapped.shape) }
        {
            // Passthrough: already-encoded postcard bytes, u32le length-prefixed.
            // The payload bytes are borrowed from the source value — zero-copy.
            out.write_bytes(&(bytes.len() as u32).to_le_bytes());
            out.write_referenced_bytes(bytes);
            return Ok(());
        }
        // Non-passthrough: reserve u32le prefix, serialize directly, patch length.
        #[allow(unsafe_code)]
        let mapped_peek = unsafe { Peek::unchecked_new(mapped.ptr, mapped.shape) };
        if let Some(bytes) = try_runtime_encode(mapped.ptr, mapped.shape)? {
            out.write_bytes(&(bytes.len() as u32).to_le_bytes());
            out.write_bytes(&bytes);
            return Ok(());
        }
        let size_field = out.reserve_size_field();
        let before = out.bytes_written();
        serialize_peek_inner(mapped_peek, out)?;
        let len = out.bytes_written() - before;
        out.write_size_field(size_field, len as u32);
        return Ok(());
    }

    if let Some(scalar_type) = peek.scalar_type() {
        return serialize_scalar(peek, scalar_type, out);
    }

    // Def-based types before user types (Option<T> is both Def::Option and UserType::Enum,
    // Result<T,E> is Def::Result with UserType::Opaque)
    match peek.shape().def {
        Def::Option(_) => {
            let opt = peek.into_option().map_err(re)?;
            return match opt.value() {
                Some(inner) => {
                    out.write_byte(0x01);
                    serialize_peek(inner, out)
                }
                None => {
                    out.write_byte(0x00);
                    Ok(())
                }
            };
        }
        Def::Result(_) => {
            let res = peek.into_result().map_err(re)?;
            return if let Some(ok_inner) = res.ok() {
                encode::write_varint(out, 0);
                serialize_peek(ok_inner, out)
            } else if let Some(err_inner) = res.err() {
                encode::write_varint(out, 1);
                serialize_peek(err_inner, out)
            } else {
                Err(SerializeError::ReflectError(
                    "Result is neither Ok nor Err".into(),
                ))
            };
        }
        Def::List(list_def) => {
            if list_def.t().is_type::<u8>() {
                // Vec<u8> → varint len + raw bytes
                let list = peek.into_list().map_err(re)?;
                if let Some(bytes) = peek.as_bytes() {
                    encode::write_varint(out, bytes.len() as u64);
                    out.write_referenced_bytes(bytes);
                } else {
                    let len = list.len();
                    let mut bytes = Vec::with_capacity(len);
                    for i in 0..len {
                        let elem = list
                            .get(i)
                            .ok_or_else(|| SerializeError::ReflectError("list index OOB".into()))?;
                        let byte = elem.get::<u8>().map_err(re)?;
                        bytes.push(*byte);
                    }
                    encode::write_varint(out, bytes.len() as u64);
                    out.write_bytes(&bytes);
                }
            } else {
                let list = peek.into_list().map_err(re)?;
                let len = list.len();
                encode::write_varint(out, len as u64);
                for elem in list.iter() {
                    serialize_peek(elem, out)?;
                }
            }
            return Ok(());
        }
        Def::Array(_) => {
            // Fixed-size array: NO length prefix
            let list_like = peek.into_list_like().map_err(re)?;
            for elem in list_like.iter() {
                serialize_peek(elem, out)?;
            }
            return Ok(());
        }
        Def::Slice(slice_def) => {
            let list_like = peek.into_list_like().map_err(re)?;
            if slice_def.t().is_type::<u8>() {
                if let Some(bytes) = list_like.as_bytes() {
                    encode::write_varint(out, bytes.len() as u64);
                    out.write_referenced_bytes(bytes);
                } else {
                    let len = list_like.len();
                    let mut bytes = Vec::with_capacity(len);
                    for elem in list_like.iter() {
                        let byte = elem.get::<u8>().map_err(re)?;
                        bytes.push(*byte);
                    }
                    encode::write_varint(out, bytes.len() as u64);
                    out.write_bytes(&bytes);
                }
            } else {
                let len = list_like.len();
                encode::write_varint(out, len as u64);
                for elem in list_like.iter() {
                    serialize_peek(elem, out)?;
                }
            }
            return Ok(());
        }
        Def::Map(_) => {
            let map = peek.into_map().map_err(re)?;
            encode::write_varint(out, map.len() as u64);
            for (key, value) in map.iter() {
                serialize_peek(key, out)?;
                serialize_peek(value, out)?;
            }
            return Ok(());
        }
        Def::Set(_) => {
            let set = peek.into_set().map_err(re)?;
            encode::write_varint(out, set.len() as u64);
            for elem in set.iter() {
                serialize_peek(elem, out)?;
            }
            return Ok(());
        }
        Def::Pointer(_) => {
            let ptr = peek.into_pointer().map_err(re)?;
            return match ptr.borrow_inner() {
                Some(inner) => serialize_peek(inner, out),
                None => Err(SerializeError::UnsupportedType("null pointer".into())),
            };
        }
        // `Def::DynamicValue` — `facet_value::Value`. The postcard wire
        // format mirrors `facet-postcard`'s tagged scheme: one tag byte
        // (0..9) per `facet_format::DynamicValueTag` + variant payload
        // (recurses for arrays/objects).
        Def::DynamicValue(_) => {
            let value = peek.get::<Value>().map_err(re)?;
            return serialize_dynamic_value(value, out);
        }
        _ => {}
    }

    // User types: struct/enum
    match peek.shape().ty {
        Type::User(UserType::Struct(struct_type)) => match struct_type.kind {
            StructKind::Struct | StructKind::TupleStruct | StructKind::Tuple => {
                // All struct kinds: fields in order, no delimiters, no count prefix
                let ps = peek.into_struct().map_err(re)?;
                for i in 0..ps.field_count() {
                    let field_peek = ps.field(i).map_err(re)?;
                    serialize_peek_inner(field_peek, out)?;
                }
                Ok(())
            }
            StructKind::Unit => Ok(()),
        },
        Type::User(UserType::Enum(_)) => {
            let pe = peek.into_enum().map_err(re)?;
            let variant_index = pe.variant_index().map_err(re)?;
            let variant = pe.active_variant().map_err(re)?;

            encode::write_varint(out, variant_index as u64);

            match variant.data.kind {
                StructKind::Unit => {}
                StructKind::TupleStruct | StructKind::Tuple | StructKind::Struct => {
                    for i in 0..variant.data.fields.len() {
                        let field_peek = pe.field(i).map_err(re)?.ok_or_else(|| {
                            SerializeError::ReflectError("missing variant field".into())
                        })?;
                        serialize_peek_inner(field_peek, out)?;
                    }
                }
            }
            Ok(())
        }
        _ => Err(SerializeError::UnsupportedType(format!("{}", peek.shape()))),
    }
}

/// Encode a `facet_value::Value` using the same tagged postcard scheme as
/// `facet-postcard`. The tag bytes match `facet_format::DynamicValueTag`:
/// 0=Null 1=Bool 2=I64 3=U64 4=F64 5=String 6=Bytes 7=Array 8=Object 9=DateTime.
fn serialize_dynamic_value(value: &Value, out: &mut impl Writer) -> Result<(), SerializeError> {
    match value.value_type() {
        ValueType::Null => {
            out.write_byte(0);
        }
        ValueType::Bool => {
            out.write_byte(1);
            let b = value.as_bool().ok_or_else(|| {
                SerializeError::ReflectError("Value claims Bool but as_bool() returned None".into())
            })?;
            out.write_byte(if b { 1 } else { 0 });
        }
        ValueType::Number => {
            let n = value.as_number().ok_or_else(|| {
                SerializeError::ReflectError(
                    "Value claims Number but as_number() returned None".into(),
                )
            })?;
            if n.is_integer() {
                if let Some(i) = n.to_i64() {
                    out.write_byte(2);
                    encode::write_varint_signed(out, i);
                } else if let Some(u) = n.to_u64() {
                    out.write_byte(3);
                    encode::write_varint(out, u);
                } else {
                    // Integer too large for either i64 or u64 — fall through
                    // to a lossy f64 representation (mirrors facet-postcard).
                    out.write_byte(4);
                    out.write_bytes(&n.to_f64_lossy().to_le_bytes());
                }
            } else {
                out.write_byte(4);
                let f = n.to_f64().unwrap_or_else(|| n.to_f64_lossy());
                out.write_bytes(&f.to_le_bytes());
            }
        }
        ValueType::String => {
            out.write_byte(5);
            let s = value
                .as_string()
                .ok_or_else(|| {
                    SerializeError::ReflectError(
                        "Value claims String but as_string() returned None".into(),
                    )
                })?
                .as_str();
            encode::write_varint(out, s.len() as u64);
            out.write_bytes(s.as_bytes());
        }
        ValueType::Bytes => {
            out.write_byte(6);
            let b = value
                .as_bytes()
                .ok_or_else(|| {
                    SerializeError::ReflectError(
                        "Value claims Bytes but as_bytes() returned None".into(),
                    )
                })?
                .as_slice();
            encode::write_varint(out, b.len() as u64);
            out.write_bytes(b);
        }
        ValueType::Array => {
            out.write_byte(7);
            let arr = value.as_array().ok_or_else(|| {
                SerializeError::ReflectError(
                    "Value claims Array but as_array() returned None".into(),
                )
            })?;
            encode::write_varint(out, arr.len() as u64);
            for item in arr {
                serialize_dynamic_value(item, out)?;
            }
        }
        ValueType::Object => {
            out.write_byte(8);
            let obj = value.as_object().ok_or_else(|| {
                SerializeError::ReflectError(
                    "Value claims Object but as_object() returned None".into(),
                )
            })?;
            encode::write_varint(out, obj.len() as u64);
            for (k, v) in obj.iter() {
                let s = k.as_str();
                encode::write_varint(out, s.len() as u64);
                out.write_bytes(s.as_bytes());
                serialize_dynamic_value(v, out)?;
            }
        }
        ValueType::DateTime | ValueType::QName | ValueType::Uuid => {
            return Err(SerializeError::UnsupportedType(format!(
                "facet_value::Value variant {:?} not yet implemented in vox-postcard \
                 (Null/Bool/Number/String/Bytes/Array/Object work)",
                value.value_type()
            )));
        }
    }
    Ok(())
}

fn serialize_scalar<'a>(
    peek: Peek<'a, '_>,
    scalar_type: ScalarType,
    out: &mut impl PostcardWriter<'a>,
) -> Result<(), SerializeError> {
    let re = |e: facet_reflect::ReflectError| SerializeError::ReflectError(e.to_string());
    match scalar_type {
        ScalarType::Unit => {}
        ScalarType::Bool => {
            let v = *peek.get::<bool>().map_err(re)?;
            out.write_byte(if v { 0x01 } else { 0x00 });
        }
        ScalarType::Char => {
            let v = *peek.get::<char>().map_err(re)?;
            let mut buf = [0u8; 4];
            let s = v.encode_utf8(&mut buf);
            encode::write_varint(out, s.len() as u64);
            out.write_bytes(s.as_bytes());
        }
        ScalarType::U8 => {
            let v = *peek.get::<u8>().map_err(re)?;
            out.write_byte(v);
        }
        ScalarType::U16 => {
            let v = *peek.get::<u16>().map_err(re)?;
            encode::write_varint(out, v as u64);
        }
        ScalarType::U32 => {
            let v = *peek.get::<u32>().map_err(re)?;
            encode::write_varint(out, v as u64);
        }
        ScalarType::U64 => {
            let v = *peek.get::<u64>().map_err(re)?;
            encode::write_varint(out, v);
        }
        ScalarType::U128 => {
            let v = *peek.get::<u128>().map_err(re)?;
            encode::write_varint_u128(out, v);
        }
        ScalarType::USize => {
            let v = *peek.get::<usize>().map_err(re)?;
            encode::write_varint(out, v as u64);
        }
        ScalarType::I8 => {
            let v = *peek.get::<i8>().map_err(re)?;
            out.write_byte(v as u8);
        }
        ScalarType::I16 => {
            let v = *peek.get::<i16>().map_err(re)?;
            encode::write_varint_signed(out, v as i64);
        }
        ScalarType::I32 => {
            let v = *peek.get::<i32>().map_err(re)?;
            encode::write_varint_signed(out, v as i64);
        }
        ScalarType::I64 => {
            let v = *peek.get::<i64>().map_err(re)?;
            encode::write_varint_signed(out, v);
        }
        ScalarType::I128 => {
            let v = *peek.get::<i128>().map_err(re)?;
            encode::write_varint_signed_i128(out, v);
        }
        ScalarType::ISize => {
            let v = *peek.get::<isize>().map_err(re)?;
            encode::write_varint_signed(out, v as i64);
        }
        ScalarType::F32 => {
            let v = *peek.get::<f32>().map_err(re)?;
            out.write_bytes(&v.to_le_bytes());
        }
        ScalarType::F64 => {
            let v = *peek.get::<f64>().map_err(re)?;
            out.write_bytes(&v.to_le_bytes());
        }
        ScalarType::Str | ScalarType::String | ScalarType::CowStr => {
            let s = peek
                .as_str()
                .ok_or_else(|| SerializeError::ReflectError("failed to extract string".into()))?;
            encode::write_varint(out, s.len() as u64);
            out.write_referenced_bytes(s.as_bytes());
        }
        _ => {
            return Err(SerializeError::UnsupportedType(format!(
                "scalar type {scalar_type:?}"
            )));
        }
    }
    Ok(())
}
