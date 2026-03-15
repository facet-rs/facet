#![deny(unsafe_code)]

use facet::Facet;

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

/// A batch of schemas sent over the wire.
#[derive(Facet, Clone, Debug)]
pub struct SchemaMessage {
    pub schemas: Vec<Schema>,
}
