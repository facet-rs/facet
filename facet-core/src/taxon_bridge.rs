//! Projection from facet [`Shape`]s to taxon's abstract schema vocabulary.
//!
//! This bridge emits a batch of [`taxon::Schema`] values whose ids, and all
//! in-batch [`taxon::SchemaRef::Concrete`] references, are provisional dense
//! keys allocated with [`taxon::SchemaId::from_raw`]. Callers that need stable
//! content ids must pass the whole batch to [`taxon::resolve_ids`]; this module
//! deliberately does not resolve ids while walking facet metadata.

use alloc::{
    format,
    string::{String, ToString},
    vec,
    vec::Vec,
};

use taxon::{Field, Kind, Primitive, Schema, SchemaId, SchemaRef, Variant, VariantPayload};

use crate::{
    ConstTypeId, DeclId, Def, Field as FacetField, PointerType, ScalarType, Shape, StructKind,
    Type, UserType,
};

/// Build the complete provisional taxon schema batch reachable from `shape`.
///
/// The root schema is always `schemas[0]`. Every schema id in the returned
/// batch is a caller-local provisional key; feed the whole batch to
/// [`taxon::resolve_ids`] before storing, comparing, or sending ids outside the
/// current process.
#[must_use]
pub fn schemas_of(shape: &'static Shape) -> Vec<Schema> {
    let mut builder = Builder::default();
    builder.ref_of(shape);
    builder.schemas
}

/// Build only the root provisional taxon schema for `shape`.
///
/// This is a convenience for non-recursive inspection. For any schema that can
/// reference other schemas, prefer [`schemas_of`] so callers keep the complete
/// batch needed by [`taxon::resolve_ids`].
#[must_use]
pub fn schema_of(shape: &'static Shape) -> Schema {
    let mut schemas = schemas_of(shape);
    schemas.remove(0)
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct ShapeKey {
    id: ConstTypeId,
    decl_id: DeclId,
}

impl ShapeKey {
    fn of(shape: &'static Shape) -> Self {
        Self {
            id: shape.id,
            decl_id: shape.decl_id,
        }
    }
}

#[derive(Default)]
struct Builder {
    schemas: Vec<Schema>,
    seen: Vec<(ShapeKey, SchemaId)>,
}

impl Builder {
    fn ref_of(&mut self, shape: &'static Shape) -> SchemaRef {
        if let Some(pointee) = transparent_pointee(shape) {
            return self.ref_of(pointee);
        }
        let id = self.intern(shape);
        SchemaRef::concrete(id)
    }

    fn intern(&mut self, shape: &'static Shape) -> SchemaId {
        let key = ShapeKey::of(shape);
        if let Some((_, id)) = self.seen.iter().find(|(seen, _)| *seen == key) {
            return *id;
        }

        let id = SchemaId::from_raw(self.schemas.len() as u64);
        self.seen.push((key, id));
        self.schemas.push(Schema {
            id,
            type_params: Vec::new(),
            kind: Kind::Dynamic,
        });

        let index = self.schemas.len() - 1;
        self.schemas[index].kind = self.kind_of(shape);
        id
    }

    fn kind_of(&mut self, shape: &'static Shape) -> Kind {
        if let Some(primitive) = primitive_of(shape) {
            return Kind::Primitive(primitive);
        }

        match shape.def {
            Def::Map(map) => Kind::Map {
                key: self.ref_of(map.k()),
                value: self.ref_of(map.v()),
            },
            Def::Set(set) => Kind::Set {
                element: self.ref_of(set.t()),
            },
            Def::List(list) => Kind::List {
                element: self.ref_of(list.t()),
            },
            Def::Array(array) => Kind::Array {
                element: self.ref_of(array.t()),
                dimensions: vec![array.n as u64],
            },
            Def::NdArray(array) => Kind::Tensor {
                element: self.ref_of(array.t()),
                rank: None,
            },
            Def::Slice(slice) => Kind::List {
                element: self.ref_of(slice.t()),
            },
            Def::Option(option) => Kind::Option {
                element: self.ref_of(option.t()),
            },
            Def::Result(result) => Kind::Enum {
                name: shape.type_identifier.to_string(),
                variants: vec![
                    Variant {
                        name: "Ok".to_string(),
                        index: 0,
                        payload: VariantPayload::Newtype(self.ref_of(result.t())),
                    },
                    Variant {
                        name: "Err".to_string(),
                        index: 1,
                        payload: VariantPayload::Newtype(self.ref_of(result.e())),
                    },
                ],
            },
            Def::Pointer(pointer) => match pointer.pointee() {
                Some(pointee) => self.kind_of(pointee),
                None => external(shape, "rust-pointer-opaque"),
            },
            Def::DynamicValue(_) => Kind::Dynamic,
            Def::Scalar | Def::Undefined => self.kind_from_type(shape),
        }
    }

    fn kind_from_type(&mut self, shape: &'static Shape) -> Kind {
        match shape.ty {
            Type::Primitive(_) => external(shape, "rust-primitive-unmapped"),
            Type::Sequence(sequence) => match sequence {
                crate::SequenceType::Array(array) => Kind::Array {
                    element: self.ref_of(array.t),
                    dimensions: vec![array.n as u64],
                },
                crate::SequenceType::Slice(slice) => Kind::List {
                    element: self.ref_of(slice.t),
                },
            },
            Type::User(user) => match user {
                UserType::Struct(struct_type) => self.struct_kind(shape, struct_type),
                UserType::Enum(enum_type) => Kind::Enum {
                    name: shape.type_identifier.to_string(),
                    variants: self.enum_variants(enum_type),
                },
                UserType::Union(_) => external(shape, "rust-union"),
                UserType::Opaque => external(shape, "rust-opaque"),
            },
            Type::Pointer(pointer) => match pointer {
                PointerType::Reference(reference) | PointerType::Raw(reference) => {
                    self.kind_of(reference.target())
                }
                PointerType::Function(_) => external(shape, "rust-fn-pointer"),
            },
            Type::Undefined => external(shape, "rust-undefined"),
        }
    }

    fn struct_kind(&mut self, shape: &'static Shape, struct_type: crate::StructType) -> Kind {
        match struct_type.kind {
            StructKind::Tuple | StructKind::TupleStruct => {
                let mut elements = Vec::with_capacity(struct_type.fields.len());
                for field in struct_type.fields {
                    elements.push(self.ref_of(field.shape()));
                }
                Kind::Tuple { elements }
            }
            StructKind::Unit | StructKind::Struct => {
                let mut fields = Vec::with_capacity(struct_type.fields.len());
                for field in struct_type.fields {
                    fields.push(self.field(field));
                }
                Kind::Struct {
                    name: shape.type_identifier.to_string(),
                    fields,
                }
            }
        }
    }

    fn enum_variants(&mut self, enum_type: crate::EnumType) -> Vec<Variant> {
        let mut variants = Vec::with_capacity(enum_type.variants.len());
        for (index, variant) in enum_type.variants.iter().enumerate() {
            variants.push(Variant {
                name: variant.effective_name().to_string(),
                index: index as u32,
                payload: self.variant_payload(variant),
            });
        }
        variants
    }

    fn variant_payload(&mut self, variant: &'static crate::Variant) -> VariantPayload {
        let fields = variant.data.fields;
        if fields.is_empty() {
            return VariantPayload::Unit;
        }

        match variant.data.kind {
            StructKind::Struct => {
                let mut payload_fields = Vec::with_capacity(fields.len());
                for field in fields {
                    payload_fields.push(self.field(field));
                }
                VariantPayload::Struct(payload_fields)
            }
            StructKind::Tuple | StructKind::TupleStruct if fields.len() == 1 => {
                VariantPayload::Newtype(self.ref_of(fields[0].shape()))
            }
            StructKind::Tuple | StructKind::TupleStruct => {
                let mut elements = Vec::with_capacity(fields.len());
                for field in fields {
                    elements.push(self.ref_of(field.shape()));
                }
                VariantPayload::Tuple(elements)
            }
            StructKind::Unit => VariantPayload::Unit,
        }
    }

    fn field(&mut self, field: &'static FacetField) -> Field {
        Field {
            name: field.effective_name().to_string(),
            schema: self.ref_of(field.shape()),
            required: field.default.is_none(),
        }
    }
}

fn transparent_pointee(shape: &'static Shape) -> Option<&'static Shape> {
    match shape.def {
        Def::Pointer(pointer) => pointer.pointee(),
        _ => match shape.ty {
            Type::Pointer(PointerType::Reference(reference) | PointerType::Raw(reference)) => {
                Some(reference.target())
            }
            _ => None,
        },
    }
}

fn primitive_of(shape: &'static Shape) -> Option<Primitive> {
    Some(match shape.scalar_type()? {
        ScalarType::Unit => Primitive::Unit,
        ScalarType::Bool => Primitive::Bool,
        ScalarType::Char => Primitive::Char,
        ScalarType::Str => Primitive::String,
        #[cfg(feature = "alloc")]
        ScalarType::String | ScalarType::CowStr => Primitive::String,
        ScalarType::F32 => Primitive::F32,
        ScalarType::F64 => Primitive::F64,
        ScalarType::U8 => Primitive::U8,
        ScalarType::U16 => Primitive::U16,
        ScalarType::U32 => Primitive::U32,
        ScalarType::U64 => Primitive::U64,
        ScalarType::U128 => Primitive::U128,
        ScalarType::USize => Primitive::U64,
        ScalarType::I8 => Primitive::I8,
        ScalarType::I16 => Primitive::I16,
        ScalarType::I32 => Primitive::I32,
        ScalarType::I64 => Primitive::I64,
        ScalarType::I128 => Primitive::I128,
        ScalarType::ISize => Primitive::I64,
        ScalarType::ConstTypeId => return None,
        #[cfg(feature = "net")]
        ScalarType::SocketAddr
        | ScalarType::IpAddr
        | ScalarType::Ipv4Addr
        | ScalarType::Ipv6Addr => return None,
    })
}

fn external(shape: &'static Shape, kind: &str) -> Kind {
    Kind::External {
        kind: format!("{kind}:{}", rust_name(shape)),
        metadata: None,
    }
}

fn rust_name(shape: &'static Shape) -> String {
    match shape.module_path {
        Some(module_path) => format!("{module_path}::{}", shape.type_identifier),
        None => shape.type_identifier.to_string(),
    }
}
