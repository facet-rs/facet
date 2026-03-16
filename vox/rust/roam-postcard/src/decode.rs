use crate::error::DeserializeError;
use roam_schema::{PrimitiveType, SchemaKind, SchemaRegistry, TypeId};

pub struct Cursor<'a> {
    input: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    pub fn new(input: &'a [u8]) -> Self {
        Self { input, pos: 0 }
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
        SchemaKind::Struct { fields } => {
            for field in fields {
                let field_kind = lookup_kind(&field.type_id, registry)?;
                skip_value(cursor, field_kind, registry)?;
            }
            Ok(())
        }
        SchemaKind::Enum { variants } => {
            let disc = cursor.read_varint()? as usize;
            let variant = variants
                .get(disc)
                .ok_or(DeserializeError::InvalidEnumDiscriminant {
                    pos: cursor.pos(),
                    index: disc as u64,
                    variant_count: variants.len(),
                })?;
            match &variant.payload {
                roam_schema::VariantPayload::Unit => Ok(()),
                roam_schema::VariantPayload::Newtype { type_id } => {
                    let inner_kind = lookup_kind(type_id, registry)?;
                    skip_value(cursor, inner_kind, registry)
                }
                roam_schema::VariantPayload::Struct { fields } => {
                    for field in fields {
                        let field_kind = lookup_kind(&field.type_id, registry)?;
                        skip_value(cursor, field_kind, registry)?;
                    }
                    Ok(())
                }
            }
        }
        SchemaKind::Tuple { elements } => {
            for elem_id in elements {
                let elem_kind = lookup_kind(elem_id, registry)?;
                skip_value(cursor, elem_kind, registry)?;
            }
            Ok(())
        }
        SchemaKind::List { element } | SchemaKind::Set { element } => {
            let count = cursor.read_varint()? as usize;
            let elem_kind = lookup_kind(element, registry)?;
            for _ in 0..count {
                skip_value(cursor, elem_kind, registry)?;
            }
            Ok(())
        }
        SchemaKind::Map { key, value } => {
            let count = cursor.read_varint()? as usize;
            let key_kind = lookup_kind(key, registry)?;
            let val_kind = lookup_kind(value, registry)?;
            for _ in 0..count {
                skip_value(cursor, key_kind, registry)?;
                skip_value(cursor, val_kind, registry)?;
            }
            Ok(())
        }
        SchemaKind::Array { element, length } => {
            let elem_kind = lookup_kind(element, registry)?;
            for _ in 0..*length {
                skip_value(cursor, elem_kind, registry)?;
            }
            Ok(())
        }
        SchemaKind::Option { element } => {
            let tag = cursor.read_byte()?;
            match tag {
                0x00 => Ok(()),
                0x01 => {
                    let inner_kind = lookup_kind(element, registry)?;
                    skip_value(cursor, inner_kind, registry)
                }
                other => Err(DeserializeError::InvalidOptionTag {
                    pos: cursor.pos() - 1,
                    got: other,
                }),
            }
        }
    }
}

fn skip_primitive(cursor: &mut Cursor<'_>, prim: PrimitiveType) -> Result<(), DeserializeError> {
    match prim {
        PrimitiveType::Unit => Ok(()),
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
    }
}

fn lookup_kind<'a>(
    type_id: &TypeId,
    registry: &'a SchemaRegistry,
) -> Result<&'a SchemaKind, DeserializeError> {
    registry.get(type_id).map(|s| &s.kind).ok_or_else(|| {
        DeserializeError::Custom(format!("schema not found for type_id {type_id:?}"))
    })
}
