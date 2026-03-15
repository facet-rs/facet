#![deny(unsafe_code)]

use facet::Facet;
use facet_core::{Def, ScalarType, Shape, StructKind, Type, UserType};
use roam_types::{is_rx, is_tx};
use std::collections::HashSet;

/// Compute a TypeId from a Shape by hashing its canonical byte encoding with blake3.
pub fn type_id_of(shape: &'static Shape) -> TypeId {
    let bytes = roam_hash::encode_shape_bytes(shape);
    let hash = blake3::hash(&bytes);
    let mut id = [0u8; 16];
    id.copy_from_slice(&hash.as_bytes()[..16]);
    TypeId(id)
}

/// Extract all schemas for a type and its transitive dependencies.
///
/// Returns schemas in dependency order: dependencies appear before dependents.
/// The root type's schema is last.
pub fn extract_schemas(shape: &'static Shape) -> Vec<Schema> {
    let mut ctx = ExtractCtx {
        schemas: Vec::new(),
        seen: HashSet::new(),
        stack: Vec::new(),
    };
    ctx.extract(shape);
    ctx.schemas
}

struct ExtractCtx {
    schemas: Vec<Schema>,
    /// Shapes already fully processed (by pointer identity).
    seen: HashSet<usize>,
    /// Stack for cycle detection (by pointer identity).
    stack: Vec<usize>,
}

impl ExtractCtx {
    /// Extract a schema for the given shape, returning its TypeId.
    /// Recursively extracts dependencies first.
    fn extract(&mut self, shape: &'static Shape) -> TypeId {
        // Channel types: extract the element type, skip the channel wrapper.
        if is_tx(shape) || is_rx(shape) {
            if let Some(inner) = shape.type_params.first() {
                return self.extract(inner.shape);
            }
        }

        // Transparent wrappers: follow inner.
        if shape.is_transparent() {
            if let Some(inner) = shape.inner {
                return self.extract(inner);
            }
        }

        let type_id = type_id_of(shape);
        let ptr = shape as *const Shape as usize;

        // Already fully processed — just return its id.
        if self.seen.contains(&ptr) {
            return type_id;
        }

        // Cycle detection: if on the stack, return the id without re-entering.
        if self.stack.contains(&ptr) {
            return type_id;
        }

        // Scalars
        if let Some(scalar) = shape.scalar_type() {
            if self.seen.insert(ptr) {
                self.schemas.push(Schema {
                    type_id,
                    kind: SchemaKind::Primitive {
                        primitive_type: scalar_to_primitive(scalar),
                    },
                });
            }
            return type_id;
        }

        // Containers
        match shape.def {
            Def::List(list_def) => {
                if let Some(ScalarType::U8) = list_def.t().scalar_type() {
                    // Vec<u8> → Bytes
                    if self.seen.insert(ptr) {
                        self.schemas.push(Schema {
                            type_id,
                            kind: SchemaKind::Primitive {
                                primitive_type: PrimitiveType::Bytes,
                            },
                        });
                    }
                } else {
                    let elem_id = self.extract(list_def.t());
                    if self.seen.insert(ptr) {
                        self.schemas.push(Schema {
                            type_id,
                            kind: SchemaKind::List { element: elem_id },
                        });
                    }
                }
                return type_id;
            }
            Def::Array(array_def) => {
                let elem_id = self.extract(array_def.t());
                if self.seen.insert(ptr) {
                    self.schemas.push(Schema {
                        type_id,
                        kind: SchemaKind::Array {
                            element: elem_id,
                            length: array_def.n as u64,
                        },
                    });
                }
                return type_id;
            }
            Def::Slice(slice_def) => {
                let elem_id = self.extract(slice_def.t());
                if self.seen.insert(ptr) {
                    self.schemas.push(Schema {
                        type_id,
                        kind: SchemaKind::List { element: elem_id },
                    });
                }
                return type_id;
            }
            Def::Map(map_def) => {
                let key_id = self.extract(map_def.k());
                let val_id = self.extract(map_def.v());
                if self.seen.insert(ptr) {
                    self.schemas.push(Schema {
                        type_id,
                        kind: SchemaKind::Map {
                            key: key_id,
                            value: val_id,
                        },
                    });
                }
                return type_id;
            }
            Def::Set(set_def) => {
                let elem_id = self.extract(set_def.t());
                if self.seen.insert(ptr) {
                    self.schemas.push(Schema {
                        type_id,
                        kind: SchemaKind::Set { element: elem_id },
                    });
                }
                return type_id;
            }
            Def::Option(opt_def) => {
                let elem_id = self.extract(opt_def.t());
                if self.seen.insert(ptr) {
                    self.schemas.push(Schema {
                        type_id,
                        kind: SchemaKind::Option { element: elem_id },
                    });
                }
                return type_id;
            }
            Def::Pointer(ptr_def) => {
                if let Some(pointee) = ptr_def.pointee {
                    return self.extract(pointee);
                }
            }
            _ => {}
        }

        // User-defined types: push onto stack for cycle detection.
        self.stack.push(ptr);

        let kind = match shape.ty {
            Type::User(UserType::Struct(struct_type)) => match struct_type.kind {
                StructKind::Unit => SchemaKind::Primitive {
                    primitive_type: PrimitiveType::Unit,
                },
                StructKind::TupleStruct | StructKind::Tuple => {
                    let elements: Vec<TypeId> = struct_type
                        .fields
                        .iter()
                        .map(|f| self.extract(f.shape()))
                        .collect();
                    SchemaKind::Tuple { elements }
                }
                StructKind::Struct => {
                    let fields: Vec<FieldSchema> = struct_type
                        .fields
                        .iter()
                        .map(|f| FieldSchema {
                            name: f.name.to_string(),
                            type_id: self.extract(f.shape()),
                            required: true,
                        })
                        .collect();
                    SchemaKind::Struct { fields }
                }
            },
            Type::User(UserType::Enum(enum_type)) => {
                let variants: Vec<VariantSchema> = enum_type
                    .variants
                    .iter()
                    .enumerate()
                    .map(|(i, v)| {
                        let payload = match v.data.kind {
                            StructKind::Unit => VariantPayload::Unit,
                            StructKind::TupleStruct | StructKind::Tuple => {
                                if v.data.fields.len() == 1 {
                                    VariantPayload::Newtype {
                                        type_id: self.extract(v.data.fields[0].shape()),
                                    }
                                } else {
                                    let fields: Vec<FieldSchema> = v
                                        .data
                                        .fields
                                        .iter()
                                        .enumerate()
                                        .map(|(j, f)| FieldSchema {
                                            name: j.to_string(),
                                            type_id: self.extract(f.shape()),
                                            required: true,
                                        })
                                        .collect();
                                    VariantPayload::Struct { fields }
                                }
                            }
                            StructKind::Struct => {
                                let fields: Vec<FieldSchema> = v
                                    .data
                                    .fields
                                    .iter()
                                    .map(|f| FieldSchema {
                                        name: f.name.to_string(),
                                        type_id: self.extract(f.shape()),
                                        required: true,
                                    })
                                    .collect();
                                VariantPayload::Struct { fields }
                            }
                        };
                        VariantSchema {
                            name: v.name.to_string(),
                            index: i as u32,
                            payload,
                        }
                    })
                    .collect();
                SchemaKind::Enum { variants }
            }
            Type::Pointer(_) => {
                // Follow pointer type params
                if let Some(inner) = shape.type_params.first() {
                    self.stack.pop();
                    return self.extract(inner.shape);
                }
                SchemaKind::Primitive {
                    primitive_type: PrimitiveType::Unit,
                }
            }
            _ => SchemaKind::Primitive {
                primitive_type: PrimitiveType::Unit,
            },
        };

        self.stack.pop();

        if self.seen.insert(ptr) {
            self.schemas.push(Schema { type_id, kind });
        }

        type_id
    }
}

fn scalar_to_primitive(scalar: ScalarType) -> PrimitiveType {
    match scalar {
        ScalarType::Unit => PrimitiveType::Unit,
        ScalarType::Bool => PrimitiveType::Bool,
        ScalarType::Char => PrimitiveType::Char,
        ScalarType::Str | ScalarType::String | ScalarType::CowStr => PrimitiveType::String,
        ScalarType::F32 => PrimitiveType::F32,
        ScalarType::F64 => PrimitiveType::F64,
        ScalarType::U8 => PrimitiveType::U8,
        ScalarType::U16 => PrimitiveType::U16,
        ScalarType::U32 => PrimitiveType::U32,
        ScalarType::U64 => PrimitiveType::U64,
        ScalarType::U128 => PrimitiveType::U128,
        ScalarType::USize => PrimitiveType::U64,
        ScalarType::I8 => PrimitiveType::I8,
        ScalarType::I16 => PrimitiveType::I16,
        ScalarType::I32 => PrimitiveType::I32,
        ScalarType::I64 => PrimitiveType::I64,
        ScalarType::I128 => PrimitiveType::I128,
        ScalarType::ISize => PrimitiveType::I64,
        ScalarType::ConstTypeId => PrimitiveType::U64,
        _ => PrimitiveType::Unit,
    }
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

/// A batch of schemas sent over the wire.
#[derive(Facet, Clone, Debug)]
pub struct SchemaMessage {
    pub schemas: Vec<Schema>,
}
