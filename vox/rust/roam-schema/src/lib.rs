#![deny(unsafe_code)]

use facet::Facet;
use std::collections::HashMap;

/// An opaque type identifier, unique within a connection half.
///
/// Type IDs are assigned by the sender (typically as incrementing integers)
/// and are used so schemas can reference each other and to track which types
/// have already been sent. They are not stable across connections.
// r[impl schema.type-id]
#[derive(Facet, Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct TypeSchemaId(pub u32);

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

/// Lookup table mapping TypeSchemaId → Schema, used for resolving type
/// references during deserialization with translation plans.
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
    /// Whether this binding is for args (caller → callee) or response (callee → caller).
    pub direction: BindingDirection,
}

/// Whether a method schema binding describes args or the response type.
#[derive(Facet, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum BindingDirection {
    /// The sender will send data of this type as method arguments.
    Args,
    /// The sender will send data of this type as the method response.
    Response,
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

    // r[verify schema.type-id]
    #[test]
    fn type_ids_are_just_u32() {
        let id = TypeSchemaId(42);
        assert_eq!(id.0, 42);
        assert_eq!(id, TypeSchemaId(42));
        assert_ne!(id, TypeSchemaId(43));
    }

    // r[verify schema.principles.cbor]
    // r[verify schema.format.self-contained]
    // r[verify schema.format.batch]
    #[test]
    fn cbor_round_trip() {
        let schema = Schema {
            type_id: TypeSchemaId(1),
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
