#![deny(unsafe_code)]

use facet::Facet;
use facet_core::Shape;
use std::collections::HashMap;

/// Compute a TypeId from a Shape by hashing its canonical byte encoding with blake3.
// r[impl schema.type-id]
// r[impl schema.type-id.deterministic]
// r[impl schema.type-id.structural]
pub fn type_id_of(shape: &'static Shape) -> TypeId {
    let bytes = roam_hash::encode_shape_bytes(shape);
    let hash = blake3::hash(&bytes);
    let mut id = [0u8; 16];
    id.copy_from_slice(&hash.as_bytes()[..16]);
    TypeId(id)
}

/// A 16-byte identifier for a type.
#[derive(Facet, Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct TypeId(pub [u8; 16]);

/// The root schema type describing a single type.
#[derive(Facet, Clone, Debug)]
pub struct Schema {
    pub type_id: TypeId,
    pub kind: SchemaKind,
}

/// The structural kind of a type.
#[derive(Facet, Clone, Debug)]
#[repr(u8)]
pub enum SchemaKind {
    Struct { fields: Vec<FieldSchema> },
    Enum { variants: Vec<VariantSchema> },
    Tuple { elements: Vec<TypeId> },
    List { element: TypeId },
    Map { key: TypeId, value: TypeId },
    Set { element: TypeId },
    Array { element: TypeId, length: u64 },
    Option { element: TypeId },
    Primitive { primitive_type: PrimitiveType },
}

/// Describes a single field in a struct or struct variant.
#[derive(Facet, Clone, Debug)]
pub struct FieldSchema {
    pub name: String,
    pub type_id: TypeId,
    pub required: bool,
}

/// Describes a single variant in an enum.
#[derive(Facet, Clone, Debug)]
pub struct VariantSchema {
    pub name: String,
    pub index: u32,
    pub payload: VariantPayload,
}

/// The payload of an enum variant.
#[derive(Facet, Clone, Debug)]
#[repr(u8)]
pub enum VariantPayload {
    Unit,
    Newtype { type_id: TypeId },
    Struct { fields: Vec<FieldSchema> },
}

/// Primitive types supported by the wire format.
#[derive(Facet, Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum PrimitiveType {
    Bool,
    U8,
    U16,
    U32,
    U64,
    U128,
    I8,
    I16,
    I32,
    I64,
    I128,
    F32,
    F64,
    Char,
    String,
    Unit,
    Bytes,
}

/// Lookup table mapping TypeId → Schema, used for resolving type references
/// during deserialization with translation plans.
pub type SchemaRegistry = HashMap<TypeId, Schema>;

/// Build a SchemaRegistry from a list of schemas.
pub fn build_registry(schemas: &[Schema]) -> SchemaRegistry {
    schemas.iter().map(|s| (s.type_id, s.clone())).collect()
}

/// A batch of schemas sent over the wire.
#[derive(Facet, Clone, Debug)]
pub struct SchemaMessage {
    pub schemas: Vec<Schema>,
}

/// Build a CBOR-encoded schema batch from a list of schemas.
// r[impl schema.format.self-contained]
// r[impl schema.format.batch]
// r[impl schema.principles.cbor]
pub fn build_schema_message(schemas: &[Schema]) -> Vec<u8> {
    facet_cbor::to_vec(&schemas).expect("schema CBOR serialization should not fail")
}

/// Parse a CBOR-encoded schema batch.
// r[impl schema.format.batch]
// r[impl schema.principles.cbor]
pub fn parse_schema_message(bytes: &[u8]) -> Result<Vec<Schema>, facet_cbor::CborError> {
    facet_cbor::from_slice(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use facet::Facet;

    // r[verify schema.type-id]
    // r[verify schema.type-id.deterministic]
    #[test]
    fn type_id_is_stable() {
        let id1 = type_id_of(<u32 as Facet>::SHAPE);
        let id2 = type_id_of(<u32 as Facet>::SHAPE);
        assert_eq!(id1, id2);
    }

    // r[verify schema.type-id.structural]
    #[test]
    fn type_id_differs_for_different_types() {
        let id_u32 = type_id_of(<u32 as Facet>::SHAPE);
        let id_u64 = type_id_of(<u64 as Facet>::SHAPE);
        assert_ne!(id_u32, id_u64);
    }

    // r[verify schema.principles.cbor]
    // r[verify schema.format.self-contained]
    // r[verify schema.format.batch]
    #[test]
    fn cbor_round_trip() {
        // Simple schema round-trip without needing extract_schemas
        let schema = Schema {
            type_id: type_id_of(<u32 as Facet>::SHAPE),
            kind: SchemaKind::Primitive {
                primitive_type: PrimitiveType::U32,
            },
        };
        let bytes = build_schema_message(&[schema.clone()]);
        let parsed = parse_schema_message(&bytes).expect("should parse CBOR");
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].type_id, schema.type_id);
    }
}
