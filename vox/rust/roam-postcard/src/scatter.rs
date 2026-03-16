//! ScatterPlan: zero-copy serialization that separates structural bytes
//! from borrowed payload references.

use facet_core::ScalarType;
use facet_reflect::Peek;

use crate::encode;
use crate::error::SerializeError;

/// A segment of the serialized output.
#[derive(Debug)]
pub enum Segment<'a> {
    /// Structural bytes stored in the staging buffer.
    Staged { offset: usize, len: usize },
    /// Bytes borrowed directly from the source value's memory (zero-copy).
    Reference { bytes: &'a [u8] },
}

/// A plan for writing serialized output with minimal copying.
///
/// Structural metadata (varints, discriminants) lives in `staging`.
/// Payload data (strings, byte arrays) is referenced from the source value.
pub struct ScatterPlan<'a> {
    staging: Vec<u8>,
    segments: Vec<Segment<'a>>,
    total_size: usize,
}

impl<'a> ScatterPlan<'a> {
    pub fn total_size(&self) -> usize {
        self.total_size
    }

    pub fn staging(&self) -> &[u8] {
        &self.staging
    }

    pub fn segments(&self) -> &[Segment<'a>] {
        &self.segments
    }

    /// Write the full serialized output into `dest`.
    /// `dest` must be at least `total_size()` bytes.
    pub fn write_into(&self, dest: &mut [u8]) {
        let mut cursor = 0;
        for segment in &self.segments {
            match segment {
                Segment::Staged { offset, len } => {
                    dest[cursor..cursor + len]
                        .copy_from_slice(&self.staging[*offset..*offset + len]);
                    cursor += len;
                }
                Segment::Reference { bytes } => {
                    dest[cursor..cursor + bytes.len()].copy_from_slice(bytes);
                    cursor += bytes.len();
                }
            }
        }
        debug_assert_eq!(cursor, self.total_size);
    }
}

struct ScatterBuilder<'a> {
    staging: Vec<u8>,
    segments: Vec<Segment<'a>>,
    total_size: usize,
}

impl<'a> ScatterBuilder<'a> {
    fn new() -> Self {
        Self {
            staging: Vec::new(),
            segments: Vec::new(),
            total_size: 0,
        }
    }

    fn finish(self) -> ScatterPlan<'a> {
        ScatterPlan {
            staging: self.staging,
            segments: self.segments,
            total_size: self.total_size,
        }
    }

    /// Write structural bytes to staging and add a segment.
    fn write_bytes(&mut self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        let offset = self.staging.len();
        self.staging.extend_from_slice(bytes);
        let len = bytes.len();

        // Merge with previous staged segment if contiguous
        if let Some(Segment::Staged {
            offset: prev_offset,
            len: prev_len,
        }) = self.segments.last_mut()
        {
            if *prev_offset + *prev_len == offset {
                *prev_len += len;
                self.total_size += len;
                return;
            }
        }

        self.segments.push(Segment::Staged { offset, len });
        self.total_size += len;
    }

    /// Add a reference to bytes borrowed from the source value (zero-copy).
    fn write_referenced(&mut self, bytes: &'a [u8]) {
        if bytes.is_empty() {
            return;
        }
        self.segments.push(Segment::Reference { bytes });
        self.total_size += bytes.len();
    }
}

/// Build a scatter plan from a Peek value.
pub fn peek_to_scatter_plan<'input, 'facet>(
    peek: Peek<'input, 'facet>,
) -> Result<ScatterPlan<'input>, SerializeError> {
    let mut builder = ScatterBuilder::new();
    scatter_peek(peek, &mut builder, false)?;
    Ok(builder.finish())
}

fn scatter_peek<'input, 'facet>(
    peek: Peek<'input, 'facet>,
    builder: &mut ScatterBuilder<'input>,
    is_trailing: bool,
) -> Result<(), SerializeError> {
    use facet_core::{Def, StructKind, Type, UserType};

    let peek = peek.innermost_peek();
    fn re(e: impl std::fmt::Display) -> SerializeError {
        SerializeError::ReflectError(e.to_string())
    }

    // Opaque adapters
    if let Some(adapter) = peek.shape().opaque_adapter {
        #[allow(unsafe_code)]
        let mapped = unsafe { (adapter.serialize)(peek.data()) };
        #[allow(unsafe_code)]
        if let Some(bytes) =
            unsafe { crate::raw::try_decode_passthrough_bytes(mapped.ptr, mapped.shape) }
        {
            if is_trailing {
                // Trailing opaque fields omit outer length framing.
                builder.write_referenced(bytes);
            } else {
                // Non-trailing opaque fields get postcard byte-sequence framing.
                let mut len_buf = Vec::new();
                encode::write_varint(&mut len_buf, bytes.len() as u64);
                builder.write_bytes(&len_buf);
                builder.write_referenced(bytes);
            }
            return Ok(());
        }
        // Non-passthrough: scatter the mapped value.
        #[allow(unsafe_code)]
        let mapped_peek = unsafe { Peek::unchecked_new(mapped.ptr, mapped.shape) };
        if is_trailing {
            // Trailing: scatter inline (no outer length framing).
            return scatter_peek(mapped_peek, builder, false);
        } else {
            // Non-trailing: wrap in length prefix.
            let mut tmp = Vec::new();
            crate::serialize::serialize_peek(mapped_peek, &mut tmp)?;
            let mut len_buf = Vec::new();
            encode::write_varint(&mut len_buf, tmp.len() as u64);
            builder.write_bytes(&len_buf);
            builder.write_bytes(&tmp);
            return Ok(());
        }
    }

    if let Some(scalar_type) = peek.scalar_type() {
        return scatter_scalar(peek, scalar_type, builder);
    }

    match peek.shape().def {
        Def::Option(_) => {
            let opt = peek.into_option().map_err(re)?;
            return match opt.value() {
                Some(inner) => {
                    builder.write_bytes(&[0x01]);
                    scatter_peek(inner, builder, false)
                }
                None => {
                    builder.write_bytes(&[0x00]);
                    Ok(())
                }
            };
        }
        Def::List(list_def) => {
            if list_def.t().is_type::<u8>() {
                let list = peek.into_list().map_err(re)?;
                // Try to get contiguous bytes for zero-copy
                if let Some(bytes) = peek.as_bytes() {
                    let mut len_buf = Vec::new();
                    encode::write_varint(&mut len_buf, bytes.len() as u64);
                    builder.write_bytes(&len_buf);
                    builder.write_referenced(bytes);
                } else {
                    let len = list.len();
                    let mut buf = Vec::with_capacity(len);
                    for i in 0..len {
                        let elem = list
                            .get(i)
                            .ok_or_else(|| SerializeError::ReflectError("list index OOB".into()))?;
                        let byte = elem.get::<u8>().map_err(re)?;
                        buf.push(*byte);
                    }
                    let mut len_buf = Vec::new();
                    encode::write_varint(&mut len_buf, buf.len() as u64);
                    builder.write_bytes(&len_buf);
                    builder.write_bytes(&buf);
                }
            } else {
                let list = peek.into_list().map_err(re)?;
                let len = list.len();
                let mut len_buf = Vec::new();
                encode::write_varint(&mut len_buf, len as u64);
                builder.write_bytes(&len_buf);
                for elem in list.iter() {
                    scatter_peek(elem, builder, false)?;
                }
            }
            return Ok(());
        }
        Def::Array(_) => {
            let list_like = peek.into_list_like().map_err(re)?;
            for elem in list_like.iter() {
                scatter_peek(elem, builder, false)?;
            }
            return Ok(());
        }
        Def::Slice(_) => {
            let list_like = peek.into_list_like().map_err(re)?;
            let len = list_like.len();
            let mut len_buf = Vec::new();
            encode::write_varint(&mut len_buf, len as u64);
            builder.write_bytes(&len_buf);
            for elem in list_like.iter() {
                scatter_peek(elem, builder, false)?;
            }
            return Ok(());
        }
        Def::Map(_) => {
            let map = peek.into_map().map_err(re)?;
            let mut len_buf = Vec::new();
            encode::write_varint(&mut len_buf, map.len() as u64);
            builder.write_bytes(&len_buf);
            for (key, value) in map.iter() {
                scatter_peek(key, builder, false)?;
                scatter_peek(value, builder, false)?;
            }
            return Ok(());
        }
        Def::Set(_) => {
            let set = peek.into_set().map_err(re)?;
            let mut len_buf = Vec::new();
            encode::write_varint(&mut len_buf, set.len() as u64);
            builder.write_bytes(&len_buf);
            for elem in set.iter() {
                scatter_peek(elem, builder, false)?;
            }
            return Ok(());
        }
        Def::Pointer(_) => {
            let ptr = peek.into_pointer().map_err(re)?;
            return match ptr.borrow_inner() {
                Some(inner) => scatter_peek(inner, builder, false),
                None => Err(SerializeError::UnsupportedType("null pointer".into())),
            };
        }
        _ => {}
    }

    match peek.shape().ty {
        Type::User(UserType::Struct(struct_type)) => match struct_type.kind {
            StructKind::Struct | StructKind::TupleStruct | StructKind::Tuple => {
                let ps = peek.into_struct().map_err(re)?;
                for i in 0..ps.field_count() {
                    let field_peek = ps.field(i).map_err(re)?;
                    let trailing = struct_type.fields[i].has_builtin_attr("trailing");
                    scatter_peek(field_peek, builder, trailing)?;
                }
                Ok(())
            }
            StructKind::Unit => Ok(()),
        },
        Type::User(UserType::Enum(_)) => {
            let pe = peek.into_enum().map_err(re)?;
            let variant_index = pe.variant_index().map_err(re)?;
            let variant = pe.active_variant().map_err(re)?;

            let mut disc_buf = Vec::new();
            encode::write_varint(&mut disc_buf, variant_index as u64);
            builder.write_bytes(&disc_buf);

            match variant.data.kind {
                StructKind::Unit => {}
                StructKind::TupleStruct | StructKind::Tuple | StructKind::Struct => {
                    for i in 0..variant.data.fields.len() {
                        let field_peek = pe.field(i).map_err(re)?.ok_or_else(|| {
                            SerializeError::ReflectError("missing variant field".into())
                        })?;
                        let trailing = variant.data.fields[i].has_builtin_attr("trailing");
                        scatter_peek(field_peek, builder, trailing)?;
                    }
                }
            }
            Ok(())
        }
        _ => Err(SerializeError::UnsupportedType(format!("{}", peek.shape()))),
    }
}

fn scatter_scalar<'input, 'facet>(
    peek: Peek<'input, 'facet>,
    scalar_type: ScalarType,
    builder: &mut ScatterBuilder<'input>,
) -> Result<(), SerializeError> {
    let re = |e: facet_reflect::ReflectError| SerializeError::ReflectError(e.to_string());
    match scalar_type {
        ScalarType::Unit => {}
        ScalarType::Bool => {
            let v = *peek.get::<bool>().map_err(re)?;
            builder.write_bytes(&[if v { 0x01 } else { 0x00 }]);
        }
        ScalarType::Char => {
            let v = *peek.get::<char>().map_err(re)?;
            let mut buf = [0u8; 4];
            let s = v.encode_utf8(&mut buf);
            let mut len_buf = Vec::new();
            encode::write_varint(&mut len_buf, s.len() as u64);
            builder.write_bytes(&len_buf);
            builder.write_bytes(s.as_bytes());
        }
        ScalarType::U8 => {
            let v = *peek.get::<u8>().map_err(re)?;
            builder.write_bytes(&[v]);
        }
        ScalarType::U16 => {
            let v = *peek.get::<u16>().map_err(re)?;
            let mut buf = Vec::new();
            encode::write_varint(&mut buf, v as u64);
            builder.write_bytes(&buf);
        }
        ScalarType::U32 => {
            let v = *peek.get::<u32>().map_err(re)?;
            let mut buf = Vec::new();
            encode::write_varint(&mut buf, v as u64);
            builder.write_bytes(&buf);
        }
        ScalarType::U64 => {
            let v = *peek.get::<u64>().map_err(re)?;
            let mut buf = Vec::new();
            encode::write_varint(&mut buf, v);
            builder.write_bytes(&buf);
        }
        ScalarType::U128 => {
            let v = *peek.get::<u128>().map_err(re)?;
            let mut buf = Vec::new();
            encode::write_varint_u128(&mut buf, v);
            builder.write_bytes(&buf);
        }
        ScalarType::USize => {
            let v = *peek.get::<usize>().map_err(re)?;
            let mut buf = Vec::new();
            encode::write_varint(&mut buf, v as u64);
            builder.write_bytes(&buf);
        }
        ScalarType::I8 => {
            let v = *peek.get::<i8>().map_err(re)?;
            builder.write_bytes(&[v as u8]);
        }
        ScalarType::I16 => {
            let v = *peek.get::<i16>().map_err(re)?;
            let mut buf = Vec::new();
            encode::write_varint_signed(&mut buf, v as i64);
            builder.write_bytes(&buf);
        }
        ScalarType::I32 => {
            let v = *peek.get::<i32>().map_err(re)?;
            let mut buf = Vec::new();
            encode::write_varint_signed(&mut buf, v as i64);
            builder.write_bytes(&buf);
        }
        ScalarType::I64 => {
            let v = *peek.get::<i64>().map_err(re)?;
            let mut buf = Vec::new();
            encode::write_varint_signed(&mut buf, v);
            builder.write_bytes(&buf);
        }
        ScalarType::I128 => {
            let v = *peek.get::<i128>().map_err(re)?;
            let mut buf = Vec::new();
            encode::write_varint_signed_i128(&mut buf, v);
            builder.write_bytes(&buf);
        }
        ScalarType::ISize => {
            let v = *peek.get::<isize>().map_err(re)?;
            let mut buf = Vec::new();
            encode::write_varint_signed(&mut buf, v as i64);
            builder.write_bytes(&buf);
        }
        ScalarType::F32 => {
            let v = *peek.get::<f32>().map_err(re)?;
            builder.write_bytes(&v.to_le_bytes());
        }
        ScalarType::F64 => {
            let v = *peek.get::<f64>().map_err(re)?;
            builder.write_bytes(&v.to_le_bytes());
        }
        ScalarType::Str | ScalarType::String | ScalarType::CowStr => {
            let s = peek
                .as_str()
                .ok_or_else(|| SerializeError::ReflectError("failed to extract string".into()))?;
            let mut len_buf = Vec::new();
            encode::write_varint(&mut len_buf, s.len() as u64);
            builder.write_bytes(&len_buf);
            // String content is referenced from source for zero-copy
            #[allow(unsafe_code)]
            let bytes: &'input [u8] = unsafe { std::slice::from_raw_parts(s.as_ptr(), s.len()) };
            builder.write_referenced(bytes);
        }
        _ => {
            return Err(SerializeError::UnsupportedType(format!(
                "scalar type {scalar_type:?}"
            )));
        }
    }
    Ok(())
}
