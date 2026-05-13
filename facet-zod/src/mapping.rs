//! Intermediate Zod type representation and conversion from Facet [`Shape`](facet_core::Shape)s.

use facet_core::Shape;
use facet_core::*;

use crate::config::{BigIntMode, Config};

/// Intermediate representation of a Zod type, before emission to source text.
#[derive(Debug, Clone)]
pub enum ZodType {
    /// `z.string()`
    String,
    /// `z.number()` (with optional `.int()` constraint).
    Number {
        /// Whether this number is constrained to integers.
        int: bool,
    },
    /// `z.bigint()`
    BigInt,
    /// `z.boolean()`
    Boolean,
    /// `z.object({ ... })` with named fields.
    Object(Vec<ZodField>),
    /// `z.array(T)`
    Array(Box<ZodType>),
    /// `z.tuple([...])`
    Tuple(Vec<ZodType>),
    /// `z.record(K, V)`
    Record(Box<ZodType>, Box<ZodType>),
    /// `z.union([...])`
    Union(Vec<ZodType>),
    /// `z.enum([...])` — list of string literal variant names.
    Enum(Vec<String>),
    /// `T.optional()`
    Optional(Box<ZodType>),
    /// `T.nullable()`
    Nullable(Box<ZodType>),
    /// `T.nullish()`
    Nullish(Box<ZodType>),
    /// Reference to a named schema (`FooSchema`).
    Ref(String),
    /// `z.lazy(() => FooSchema)` — used to break recursive cycles.
    Lazy(String),
    /// `z.literal(...)` — string or boolean literal.
    Literal(String),
    /// `z.undefined()`
    Undefined,
    /// `z.unknown()`
    Unknown,
    /// `z.never()`
    Never,
}

/// A named field on a Zod object.
#[derive(Debug, Clone)]
pub struct ZodField {
    /// The field's serialized name (post-rename).
    pub name: String,
    /// The field's type.
    pub ty: ZodType,
    /// Whether the field is optional (Rust `Option` or has a default).
    pub optional: bool,
    /// Optional doc-comment text to emit above the field.
    pub doc: Option<String>,
}

/// A top-level named Zod schema, ready for emission.
#[derive(Debug, Clone)]
pub struct NamedSchema {
    /// The TypeScript identifier (e.g. `User`); the const is `${name}Schema`.
    pub name: String,
    /// The schema's type.
    pub ty: ZodType,
    /// Optional doc-comment text to emit above the schema.
    pub doc: Option<String>,
}

/// Convert a Facet [`Shape`] to a [`ZodType`], inserting [`ZodType::Lazy`] for
/// types already on the `visiting` stack to break cycles.
pub fn shape_to_zod(
    shape: &'static Shape,
    config: &Config,
    visiting: &mut Vec<ConstTypeId>,
) -> ZodType {
    let id = shape.id;

    if visiting.contains(&id) {
        return ZodType::Lazy(schema_name(shape));
    }

    visiting.push(id);
    let result = shape_to_zod_inner(shape, config, visiting);
    visiting.pop();
    result
}

fn shape_to_zod_inner(
    shape: &'static Shape,
    config: &Config,
    visiting: &mut Vec<ConstTypeId>,
) -> ZodType {
    match &shape.def {
        Def::Option(opt) => {
            let inner = shape_to_zod(opt.t, config, visiting);
            wrap_optional(inner, config)
        }
        Def::List(list) => {
            let elem = shape_to_zod(list.t, config, visiting);
            ZodType::Array(Box::new(elem))
        }
        Def::Set(set) => {
            let elem = shape_to_zod(set.t, config, visiting);
            ZodType::Array(Box::new(elem))
        }
        Def::Map(map) => {
            let k = shape_to_zod(map.k, config, visiting);
            let v = shape_to_zod(map.v, config, visiting);
            ZodType::Record(Box::new(k), Box::new(v))
        }
        Def::Array(arr) => {
            let elem = shape_to_zod(arr.t, config, visiting);
            ZodType::Tuple(vec![elem; arr.n])
        }
        Def::Slice(slice) => {
            let elem = shape_to_zod(slice.t, config, visiting);
            ZodType::Array(Box::new(elem))
        }
        Def::Result(res) => {
            let ok = shape_to_zod(res.t, config, visiting);
            let err = shape_to_zod(res.e, config, visiting);
            ZodType::Union(vec![
                ZodType::Object(vec![
                    ZodField {
                        name: "ok".into(),
                        ty: ZodType::Literal("true".into()),
                        optional: false,
                        doc: None,
                    },
                    ZodField {
                        name: "value".into(),
                        ty: ok,
                        optional: false,
                        doc: None,
                    },
                ]),
                ZodType::Object(vec![
                    ZodField {
                        name: "ok".into(),
                        ty: ZodType::Literal("false".into()),
                        optional: false,
                        doc: None,
                    },
                    ZodField {
                        name: "error".into(),
                        ty: err,
                        optional: false,
                        doc: None,
                    },
                ]),
            ])
        }
        Def::Pointer(ptr) => {
            if let Some(pointee) = ptr.pointee {
                shape_to_zod(pointee, config, visiting)
            } else {
                ZodType::Unknown
            }
        }
        Def::Scalar => primitive_to_zod(shape, config),
        Def::Undefined | Def::DynamicValue(_) | Def::NdArray(_) => {
            map_by_type(shape, config, visiting)
        }
        _ => map_by_type(shape, config, visiting),
    }
}

fn map_by_type(shape: &'static Shape, config: &Config, visiting: &mut Vec<ConstTypeId>) -> ZodType {
    match &shape.ty {
        Type::User(UserType::Struct(st)) => struct_to_zod(st, shape, config, visiting),
        Type::User(UserType::Enum(et)) => enum_to_zod(et, shape, config, visiting),
        Type::Primitive(_) => primitive_to_zod(shape, config),
        Type::Sequence(SequenceType::Array(arr)) => {
            let elem = shape_to_zod(arr.t, config, visiting);
            ZodType::Tuple(vec![elem; arr.n])
        }
        Type::Sequence(SequenceType::Slice(slice)) => {
            let elem = shape_to_zod(slice.t, config, visiting);
            ZodType::Array(Box::new(elem))
        }
        Type::Pointer(PointerType::Reference(vp) | PointerType::Raw(vp)) => {
            shape_to_zod(vp.target, config, visiting)
        }
        Type::Pointer(PointerType::Function(_)) => ZodType::Never,
        _ => ZodType::Unknown,
    }
}

fn primitive_to_zod(shape: &'static Shape, config: &Config) -> ZodType {
    match &shape.ty {
        Type::Primitive(PrimitiveType::Boolean) => ZodType::Boolean,
        Type::Primitive(PrimitiveType::Textual(_)) => ZodType::String,
        Type::Primitive(PrimitiveType::Numeric(num)) => numeric_to_zod(num, shape, config),
        Type::Primitive(PrimitiveType::Never) => ZodType::Never,
        _ => {
            if shape.type_identifier == "String" || shape.type_identifier == "str" {
                ZodType::String
            } else {
                ZodType::Unknown
            }
        }
    }
}

fn numeric_to_zod(num: &NumericType, shape: &'static Shape, config: &Config) -> ZodType {
    match num {
        NumericType::Float => ZodType::Number { int: false },
        NumericType::Integer { .. } => {
            let is_large = match shape.layout {
                ShapeLayout::Sized(layout) => layout.size() >= 8,
                ShapeLayout::Unsized => false,
            };
            if is_large && matches!(config.bigint_mode, BigIntMode::From64Bit) {
                ZodType::BigInt
            } else {
                ZodType::Number { int: true }
            }
        }
    }
}

fn struct_to_zod(
    st: &StructType,
    shape: &'static Shape,
    config: &Config,
    visiting: &mut Vec<ConstTypeId>,
) -> ZodType {
    match st.kind {
        StructKind::TupleStruct if st.fields.len() == 1 => {
            if let Some(inner) = shape.inner {
                return shape_to_zod(inner, config, visiting);
            }
            let field_shape = st.fields[0].shape.get();
            shape_to_zod(field_shape, config, visiting)
        }
        StructKind::TupleStruct | StructKind::Tuple => {
            let elems = st
                .fields
                .iter()
                .map(|f| shape_to_zod(f.shape.get(), config, visiting))
                .collect();
            ZodType::Tuple(elems)
        }
        StructKind::Unit => ZodType::Object(vec![]),
        StructKind::Struct => {
            let fields = st
                .fields
                .iter()
                .filter(|f| {
                    !f.flags
                        .contains(FieldFlags::SKIP | FieldFlags::SKIP_SERIALIZING)
                })
                .map(|f| field_to_zod(f, config, visiting))
                .collect();
            ZodType::Object(fields)
        }
    }
}

fn field_to_zod(
    field: &'static Field,
    config: &Config,
    visiting: &mut Vec<ConstTypeId>,
) -> ZodField {
    let field_shape = field.shape.get();
    let name = field.rename.unwrap_or(field.name).to_string();
    let doc = if field.doc.is_empty() {
        None
    } else {
        Some(field.doc.join("\n"))
    };

    let is_option = matches!(field_shape.def, Def::Option(_));
    let has_default = field.has_default();

    let ty = shape_to_zod(field_shape, config, visiting);

    ZodField {
        name,
        ty,
        optional: is_option || has_default,
        doc,
    }
}

fn enum_to_zod(
    et: &EnumType,
    _shape: &'static Shape,
    config: &Config,
    visiting: &mut Vec<ConstTypeId>,
) -> ZodType {
    let all_unit = et.variants.iter().all(|v| v.data.fields.is_empty());

    if all_unit {
        let names = et
            .variants
            .iter()
            .map(|v| v.rename.unwrap_or(v.name).to_string())
            .collect();
        return ZodType::Enum(names);
    }

    let members: Vec<ZodType> = et
        .variants
        .iter()
        .map(|v| {
            let variant_name = v.rename.unwrap_or(v.name);
            if v.data.fields.is_empty() {
                ZodType::Object(vec![ZodField {
                    name: variant_name.to_string(),
                    ty: ZodType::Literal("true".into()),
                    optional: false,
                    doc: None,
                }])
            } else {
                let inner = struct_to_zod(&v.data, v.data.fields[0].shape.get(), config, visiting);
                ZodType::Object(vec![ZodField {
                    name: variant_name.to_string(),
                    ty: inner,
                    optional: false,
                    doc: None,
                }])
            }
        })
        .collect();

    ZodType::Union(members)
}

fn wrap_optional(inner: ZodType, config: &Config) -> ZodType {
    match config.optional_mode {
        crate::config::OptionalMode::Optional => ZodType::Optional(Box::new(inner)),
        crate::config::OptionalMode::Nullable => ZodType::Nullable(Box::new(inner)),
        crate::config::OptionalMode::Nullish => ZodType::Nullish(Box::new(inner)),
    }
}

/// Derive the TypeScript schema name for a given Facet [`Shape`].
pub fn schema_name(shape: &Shape) -> String {
    let base = shape.type_identifier.to_string();
    if shape.type_params.is_empty() {
        base
    } else {
        let params: Vec<String> = shape
            .type_params
            .iter()
            .map(|tp| tp.name.to_string())
            .collect();
        format!("{}{}", base, params.join(""))
    }
}
