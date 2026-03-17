#![deny(unsafe_code)]

use facet::Facet;
use facet_core::Shape;
use std::collections::HashMap;

/// Compute a TypeSchemaId from a Shape by hashing its canonical byte encoding with blake3.
// r[impl schema.type-id]
// r[impl schema.type-id.deterministic]
// r[impl schema.type-id.structural]
pub fn type_schema_id_of(shape: &'static Shape) -> TypeSchemaId {
    let bytes = roam_hash::encode_shape_bytes(shape);
    let hash = blake3::hash(&bytes);
    let mut id = [0u8; 16];
    id.copy_from_slice(&hash.as_bytes()[..16]);
    TypeSchemaId(id)
}

/// A 16-byte identifier for a type.
#[derive(Facet, Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct TypeSchemaId(pub [u8; 16]);

/// The root schema type describing a single type.
#[derive(Facet, Clone, Debug)]
pub struct Schema {
    pub type_id: TypeSchemaId,
    pub kind: SchemaKind,
}

/// The structural kind of a type.
#[derive(Facet, Clone, Debug)]
#[repr(u8)]
pub enum SchemaKind {
    Struct {
        fields: Vec<FieldSchema>,
    },
    Enum {
        variants: Vec<VariantSchema>,
    },
    Tuple {
        elements: Vec<TypeSchemaId>,
    },
    List {
        element: TypeSchemaId,
    },
    Map {
        key: TypeSchemaId,
        value: TypeSchemaId,
    },
    Set {
        element: TypeSchemaId,
    },
    Array {
        element: TypeSchemaId,
        length: u64,
    },
    Option {
        element: TypeSchemaId,
    },
    Primitive {
        primitive_type: PrimitiveType,
    },
}

/// Describes a single field in a struct or struct variant.
#[derive(Facet, Clone, Debug)]
pub struct FieldSchema {
    pub name: String,
    pub type_id: TypeSchemaId,
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
    Newtype { type_id: TypeSchemaId },
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
pub type SchemaRegistry = HashMap<TypeSchemaId, Schema>;

/// Build a SchemaRegistry from a list of schemas.
pub fn build_registry(schemas: &[Schema]) -> SchemaRegistry {
    schemas.iter().map(|s| (s.type_id, s.clone())).collect()
}

/// Binds a method (by its MethodId as u64) to the root TypeSchemaId of the
/// type being sent for that method. Sent once per method per direction.
#[derive(Facet, Clone, Debug)]
pub struct MethodSchemaBinding {
    /// The method ID (MethodId is a u64 newtype).
    pub method_id: u64,
    /// Root TypeSchemaId for this method's args or return type.
    pub root_type_schema_id: TypeSchemaId,
}

/// CBOR-encoded payload inside a schema wire message.
/// A struct so new fields can be added without breaking the wire format.
#[derive(Facet, Clone, Debug)]
pub struct SchemaMessagePayload {
    pub schemas: Vec<Schema>,
    #[facet(default)]
    pub method_bindings: Vec<MethodSchemaBinding>,
}

/// Build a CBOR-encoded schema message.
// r[impl schema.format.self-contained]
// r[impl schema.format.batch]
// r[impl schema.principles.cbor]
pub fn build_schema_message(
    schemas: &[Schema],
    method_bindings: &[MethodSchemaBinding],
) -> Vec<u8> {
    let payload = SchemaMessagePayload {
        schemas: schemas.to_vec(),
        method_bindings: method_bindings.to_vec(),
    };
    facet_cbor::to_vec(&payload).expect("schema CBOR serialization should not fail")
}

/// Parse a CBOR-encoded schema message.
// r[impl schema.format.batch]
// r[impl schema.principles.cbor]
pub fn parse_schema_message(bytes: &[u8]) -> Result<SchemaMessagePayload, facet_cbor::CborError> {
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
        let id1 = type_schema_id_of(<u32 as Facet>::SHAPE);
        let id2 = type_schema_id_of(<u32 as Facet>::SHAPE);
        assert_eq!(id1, id2);
    }

    // r[verify schema.type-id.structural]
    #[test]
    fn type_id_differs_for_different_types() {
        let id_u32 = type_schema_id_of(<u32 as Facet>::SHAPE);
        let id_u64 = type_schema_id_of(<u64 as Facet>::SHAPE);
        assert_ne!(id_u32, id_u64);
    }

    // r[verify schema.principles.cbor]
    // r[verify schema.format.self-contained]
    // r[verify schema.format.batch]
    #[test]
    fn cbor_round_trip() {
        // Simple schema round-trip without needing extract_schemas
        let schema = Schema {
            type_id: type_schema_id_of(<u32 as Facet>::SHAPE),
            kind: SchemaKind::Primitive {
                primitive_type: PrimitiveType::U32,
            },
        };
        let bytes = build_schema_message(std::slice::from_ref(&schema), &[]);
        let payload = parse_schema_message(&bytes).expect("should parse CBOR");
        assert_eq!(payload.schemas.len(), 1);
        assert_eq!(payload.schemas[0].type_id, schema.type_id);
    }
}
