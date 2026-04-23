use crate::error::DeserializeError;
use vox_schema::{PrimitiveType, SchemaKind, SchemaRegistry};

pub struct Cursor<'a> {
    input: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    pub fn new(input: &'a [u8]) -> Self {
        Self { input, pos: 0 }
    }

    /// Create a cursor over `input` starting at `pos`.
    pub fn new_at(input: &'a [u8], pos: usize) -> Self {
        Self { input, pos }
    }

    /// Advance the cursor to `new_pos` without reading bytes.
    pub fn advance_to(&mut self, new_pos: usize) {
        debug_assert!(new_pos <= self.input.len());
        self.pos = new_pos;
    }

    pub fn pos(&self) -> usize {
        self.pos
    }

    pub fn remaining(&self) -> usize {
        self.input.len() - self.pos
    }

    pub fn is_empty(&self) -> bool {
        self.pos >= self.input.len()
    }

    pub fn read_byte(&mut self) -> Result<u8, DeserializeError> {
        if self.pos >= self.input.len() {
            return Err(DeserializeError::UnexpectedEof { pos: self.pos });
        }
        let b = self.input[self.pos];
        self.pos += 1;
        Ok(b)
    }

    pub fn read_bytes(&mut self, n: usize) -> Result<&'a [u8], DeserializeError> {
        if self.pos + n > self.input.len() {
            return Err(DeserializeError::UnexpectedEof { pos: self.pos });
        }
        let slice = &self.input[self.pos..self.pos + n];
        self.pos += n;
        Ok(slice)
    }

    pub fn read_varint(&mut self) -> Result<u64, DeserializeError> {
        let start = self.pos;
        let mut result: u64 = 0;
        let mut shift: u32 = 0;
        loop {
            let byte = self.read_byte()?;
            result |= ((byte & 0x7F) as u64) << shift;
            if byte & 0x80 == 0 {
                return Ok(result);
            }
            shift += 7;
            if shift >= 64 {
                return Err(DeserializeError::VarintOverflow { pos: start });
            }
        }
    }

    pub fn read_signed_varint(&mut self) -> Result<i64, DeserializeError> {
        let zigzag = self.read_varint()?;
        Ok(((zigzag >> 1) as i64) ^ (-((zigzag & 1) as i64)))
    }

    pub fn read_varint_u128(&mut self) -> Result<u128, DeserializeError> {
        let start = self.pos;
        let mut result: u128 = 0;
        let mut shift: u32 = 0;
        loop {
            let byte = self.read_byte()?;
            result |= ((byte & 0x7F) as u128) << shift;
            if byte & 0x80 == 0 {
                return Ok(result);
            }
            shift += 7;
            if shift >= 128 {
                return Err(DeserializeError::VarintOverflow { pos: start });
            }
        }
    }

    pub fn read_signed_varint_i128(&mut self) -> Result<i128, DeserializeError> {
        let zigzag = self.read_varint_u128()?;
        Ok(((zigzag >> 1) as i128) ^ (-((zigzag & 1) as i128)))
    }

    pub fn read_str(&mut self) -> Result<&'a str, DeserializeError> {
        let len = self.read_varint()? as usize;
        let bytes = self.read_bytes(len)?;
        std::str::from_utf8(bytes).map_err(|_| DeserializeError::InvalidUtf8 {
            pos: self.pos - len,
        })
    }

    pub fn read_byte_slice(&mut self) -> Result<&'a [u8], DeserializeError> {
        let len = self.read_varint()? as usize;
        self.read_bytes(len)
    }

    pub fn read_u32le(&mut self) -> Result<u32, DeserializeError> {
        let bytes = self.read_bytes(4)?;
        Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    /// Read a u32le-length-prefixed byte slice (used for opaque values).
    pub fn read_opaque_bytes(&mut self) -> Result<&'a [u8], DeserializeError> {
        let len = self.read_u32le()? as usize;
        self.read_bytes(len)
    }
}

/// Advance the cursor past one postcard-encoded value described by `kind`,
/// without materializing it.
pub fn skip_value(
    cursor: &mut Cursor<'_>,
    kind: &SchemaKind,
    registry: &SchemaRegistry,
) -> Result<(), DeserializeError> {
    match kind {
        SchemaKind::Primitive { primitive_type } => skip_primitive(cursor, *primitive_type),
        SchemaKind::Struct { fields, .. } => {
            for field in fields {
                let field_kind = resolve_kind(&field.type_ref, registry)?;
                skip_value(cursor, &field_kind, registry)?;
            }
            Ok(())
        }
        SchemaKind::Enum { variants, .. } => {
            let disc = cursor.read_varint()? as usize;
            let variant = variants
                .get(disc)
                .ok_or(DeserializeError::InvalidEnumDiscriminant {
                    pos: cursor.pos(),
                    index: disc as u64,
                    variant_count: variants.len(),
                })?;
            match &variant.payload {
                vox_schema::VariantPayload::Unit => Ok(()),
                vox_schema::VariantPayload::Newtype { type_ref } => {
                    let inner_kind = resolve_kind(type_ref, registry)?;
                    skip_value(cursor, &inner_kind, registry)
                }
                vox_schema::VariantPayload::Tuple { types } => {
                    for type_ref in types {
                        let inner_kind = resolve_kind(type_ref, registry)?;
                        skip_value(cursor, &inner_kind, registry)?;
                    }
                    Ok(())
                }
                vox_schema::VariantPayload::Struct { fields } => {
                    for field in fields {
                        let field_kind = resolve_kind(&field.type_ref, registry)?;
                        skip_value(cursor, &field_kind, registry)?;
                    }
                    Ok(())
                }
            }
        }
        SchemaKind::Tuple { elements } => {
            for elem_ref in elements {
                let elem_kind = resolve_kind(elem_ref, registry)?;
                skip_value(cursor, &elem_kind, registry)?;
            }
            Ok(())
        }
        SchemaKind::List { element } => {
            let count = cursor.read_varint()? as usize;
            let elem_kind = resolve_kind(element, registry)?;
            for _ in 0..count {
                skip_value(cursor, &elem_kind, registry)?;
            }
            Ok(())
        }
        SchemaKind::Map { key, value } => {
            let count = cursor.read_varint()? as usize;
            let key_kind = resolve_kind(key, registry)?;
            let val_kind = resolve_kind(value, registry)?;
            for _ in 0..count {
                skip_value(cursor, &key_kind, registry)?;
                skip_value(cursor, &val_kind, registry)?;
            }
            Ok(())
        }
        SchemaKind::Array { element, length } => {
            let elem_kind = resolve_kind(element, registry)?;
            for _ in 0..*length {
                skip_value(cursor, &elem_kind, registry)?;
            }
            Ok(())
        }
        SchemaKind::Option { element } => {
            let tag = cursor.read_byte()?;
            match tag {
                0x00 => Ok(()),
                0x01 => {
                    let inner_kind = resolve_kind(element, registry)?;
                    skip_value(cursor, &inner_kind, registry)
                }
                other => Err(DeserializeError::InvalidOptionTag {
                    pos: cursor.pos() - 1,
                    got: other,
                }),
            }
        }
        // Channels are serialized as () on the wire — nothing to skip.
        SchemaKind::Channel { .. } => Ok(()),
    }
}

fn skip_primitive(cursor: &mut Cursor<'_>, prim: PrimitiveType) -> Result<(), DeserializeError> {
    match prim {
        PrimitiveType::Unit => Ok(()),
        PrimitiveType::Never => Err(DeserializeError::protocol(
            "received bytes for never primitive",
        )),
        PrimitiveType::Bool | PrimitiveType::U8 | PrimitiveType::I8 => {
            cursor.read_byte()?;
            Ok(())
        }
        PrimitiveType::U16
        | PrimitiveType::U32
        | PrimitiveType::U64
        | PrimitiveType::I16
        | PrimitiveType::I32
        | PrimitiveType::I64 => {
            cursor.read_varint()?;
            Ok(())
        }
        PrimitiveType::U128 | PrimitiveType::I128 => {
            cursor.read_varint_u128()?;
            Ok(())
        }
        PrimitiveType::F32 => {
            cursor.read_bytes(4)?;
            Ok(())
        }
        PrimitiveType::F64 => {
            cursor.read_bytes(8)?;
            Ok(())
        }
        PrimitiveType::Char | PrimitiveType::String | PrimitiveType::Bytes => {
            let len = cursor.read_varint()? as usize;
            cursor.read_bytes(len)?;
            Ok(())
        }
        PrimitiveType::Payload => {
            // Payload uses a u32 LE length prefix, not a varint
            let len_bytes = cursor.read_bytes(4)?;
            let len = u32::from_le_bytes(len_bytes.try_into().unwrap()) as usize;
            cursor.read_bytes(len)?;
            Ok(())
        }
    }
}

fn resolve_kind(
    type_ref: &vox_schema::TypeRef,
    registry: &SchemaRegistry,
) -> Result<vox_schema::SchemaKind, DeserializeError> {
    type_ref.resolve_kind(registry).ok_or_else(|| {
        DeserializeError::Custom(format!("schema not found for type_ref {type_ref:?}"))
    })
}
