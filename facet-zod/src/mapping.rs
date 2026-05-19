//! Intermediate Zod type representation and conversion from Facet [`Shape`](facet_core::Shape)s.

use std::collections::{HashMap, HashSet};

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
    /// `z.intersection(a, b)` — used for internally-tagged newtype variants.
    Intersection(Box<ZodType>, Box<ZodType>),
    /// `z.enum([...])` — list of string literal variant names.
    Enum(Vec<String>),
    /// `T.optional()`
    Optional(Box<ZodType>),
    /// `T.nullable()`
    Nullable(Box<ZodType>),
    /// `T.nullish()`
    Nullish(Box<ZodType>),
    /// Reference to an already-declared named schema (`FooSchema`).
    Ref(String),
    /// `z.lazy(() => FooSchema)` — used for forward references and to break recursive cycles.
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
    /// Whether the field key may be omitted (a non-`Option` field with a default).
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

/// Resolution context for a single named schema being emitted.
///
/// Nested named types are resolved against [`Ctx::registry`]: a type already
/// declared (in [`Ctx::emitted`]) becomes a plain [`ZodType::Ref`]; anything
/// not yet declared — including the schema currently being emitted — becomes a
/// [`ZodType::Lazy`] so the generated TypeScript is free of temporal-dead-zone
/// hazards regardless of declaration order or cycles.
pub struct Ctx<'a> {
    /// Generator configuration.
    pub config: &'a Config,
    /// All named shapes that get their own top-level declaration.
    pub registry: &'a HashMap<ConstTypeId, &'static Shape>,
    /// Named shapes whose declaration has already been written above.
    pub emitted: &'a HashSet<ConstTypeId>,
    /// The shape currently being emitted as a root (its body is expanded inline).
    pub root: ConstTypeId,
}

/// Expand a registered named shape's *body* (struct/enum/newtype), resolving any
/// nested named types through [`shape_to_zod`].
pub fn shape_to_zod_root(shape: &'static Shape, ctx: &Ctx) -> ZodType {
    map_shape(shape, ctx)
}

/// Convert a Facet [`Shape`] to a [`ZodType`].
///
/// A shape that has its own top-level declaration resolves to a reference
/// ([`ZodType::Ref`] if already declared, otherwise [`ZodType::Lazy`]) instead
/// of being inlined.
pub fn shape_to_zod(shape: &'static Shape, ctx: &Ctx) -> ZodType {
    if ctx.registry.contains_key(&shape.id) {
        let name = schema_name(shape);
        return if shape.id != ctx.root && ctx.emitted.contains(&shape.id) {
            ZodType::Ref(name)
        } else {
            ZodType::Lazy(name)
        };
    }
    map_shape(shape, ctx)
}

fn map_shape(shape: &'static Shape, ctx: &Ctx) -> ZodType {
    // A transparent wrapper (`#[facet(transparent)]` / `#[repr(transparent)]`)
    // serializes as its inner value, so its schema is the inner schema.
    if shape.is_transparent()
        && let Some(inner) = shape.inner
    {
        return shape_to_zod(inner, ctx);
    }

    match &shape.def {
        Def::Option(opt) => {
            let inner = shape_to_zod(opt.t, ctx);
            wrap_optional(inner, ctx.config)
        }
        Def::List(list) => {
            let elem = shape_to_zod(list.t, ctx);
            ZodType::Array(Box::new(elem))
        }
        Def::Set(set) => {
            let elem = shape_to_zod(set.t, ctx);
            ZodType::Array(Box::new(elem))
        }
        Def::Map(map) => {
            // JSON object keys are always strings: facet-json stringifies
            // numeric/boolean map keys (e.g. `{"1":...}`).
            let k = record_key(shape_to_zod(map.k, ctx));
            let v = shape_to_zod(map.v, ctx);
            ZodType::Record(Box::new(k), Box::new(v))
        }
        Def::Array(arr) => {
            let elem = shape_to_zod(arr.t, ctx);
            ZodType::Tuple(vec![elem; arr.n])
        }
        Def::Slice(slice) => {
            let elem = shape_to_zod(slice.t, ctx);
            ZodType::Array(Box::new(elem))
        }
        Def::Result(res) => {
            // facet-json serializes `Result` as a normal externally-tagged
            // enum: `{"Ok": T}` / `{"Err": E}`.
            ZodType::Union(vec![
                ZodType::Object(vec![ZodField {
                    name: "Ok".into(),
                    ty: shape_to_zod(res.t, ctx),
                    optional: false,
                    doc: None,
                }]),
                ZodType::Object(vec![ZodField {
                    name: "Err".into(),
                    ty: shape_to_zod(res.e, ctx),
                    optional: false,
                    doc: None,
                }]),
            ])
        }
        Def::Pointer(ptr) => {
            if let Some(pointee) = ptr.pointee {
                shape_to_zod(pointee, ctx)
            } else {
                ZodType::Unknown
            }
        }
        Def::Scalar => primitive_to_zod(shape, ctx.config),
        Def::Undefined | Def::DynamicValue(_) | Def::NdArray(_) => map_by_type(shape, ctx),
        _ => map_by_type(shape, ctx),
    }
}

fn map_by_type(shape: &'static Shape, ctx: &Ctx) -> ZodType {
    match &shape.ty {
        Type::User(UserType::Struct(st)) => struct_to_zod(st, ctx),
        Type::User(UserType::Enum(et)) => enum_to_zod(et, shape, ctx),
        Type::Primitive(_) => primitive_to_zod(shape, ctx.config),
        Type::Sequence(SequenceType::Array(arr)) => {
            let elem = shape_to_zod(arr.t, ctx);
            ZodType::Tuple(vec![elem; arr.n])
        }
        Type::Sequence(SequenceType::Slice(slice)) => {
            let elem = shape_to_zod(slice.t, ctx);
            ZodType::Array(Box::new(elem))
        }
        Type::Pointer(PointerType::Reference(vp) | PointerType::Raw(vp)) => {
            shape_to_zod(vp.target, ctx)
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

fn struct_to_zod(st: &StructType, ctx: &Ctx) -> ZodType {
    // Transparent newtypes are handled in `map_shape`; a plain tuple struct
    // serializes as a JSON array, e.g. `Wrapper(String)` -> `["w"]`.
    match st.kind {
        StructKind::TupleStruct | StructKind::Tuple => {
            let elems = st
                .fields
                .iter()
                .map(|f| shape_to_zod(f.shape.get(), ctx))
                .collect();
            ZodType::Tuple(elems)
        }
        StructKind::Unit => ZodType::Object(vec![]),
        StructKind::Struct => ZodType::Object(struct_fields(st, ctx)),
    }
}

fn struct_fields(st: &StructType, ctx: &Ctx) -> Vec<ZodField> {
    st.fields
        .iter()
        .filter(|f| !f.should_skip_serializing_unconditional())
        .map(|f| field_to_zod(f, ctx))
        .collect()
}

/// JSON object keys are always strings; numeric/boolean Rust map keys are
/// stringified by facet-json, so widen them to `z.string()`.
fn record_key(key: ZodType) -> ZodType {
    match key {
        ZodType::Number { .. } | ZodType::BigInt | ZodType::Boolean => ZodType::String,
        other => other,
    }
}

fn field_to_zod(field: &'static Field, ctx: &Ctx) -> ZodField {
    let field_shape = field.shape.get();
    let name = field.rename.unwrap_or(field.name).to_string();
    let doc = if field.doc.is_empty() {
        None
    } else {
        Some(field.doc.join("\n"))
    };

    let is_option = matches!(field_shape.def, Def::Option(_));
    let has_default = field.has_default();
    let conditionally_skipped = field.skip_serializing_if.is_some();

    let ty = shape_to_zod(field_shape, ctx);

    // `Option<T>` already carries its optionality via `wrap_optional`. A
    // non-`Option` field with a default, or one with a `skip_serializing_if`
    // predicate, may be absent from the payload — Zod expresses that with
    // `.optional()`.
    ZodField {
        name,
        ty,
        optional: (has_default && !is_option) || conditionally_skipped,
        doc,
    }
}

/// The serialized name of a variant (post-rename).
fn variant_name(v: &Variant) -> &'static str {
    v.rename.unwrap_or(v.name)
}

/// The data payload of a variant, as facet-json serializes it (the value that
/// sits next to the tag, or stands alone when untagged). `None` for unit
/// variants. A single-field tuple variant carries its field bare; multi-field
/// tuple variants become a JSON array; struct variants become an object.
fn variant_payload(v: &Variant, ctx: &Ctx) -> Option<ZodType> {
    if v.data.fields.is_empty() {
        return None;
    }
    Some(match v.data.kind {
        StructKind::Struct => ZodType::Object(struct_fields(&v.data, ctx)),
        StructKind::TupleStruct | StructKind::Tuple if v.data.fields.len() == 1 => {
            shape_to_zod(v.data.fields[0].shape.get(), ctx)
        }
        StructKind::TupleStruct | StructKind::Tuple => ZodType::Tuple(
            v.data
                .fields
                .iter()
                .map(|f| shape_to_zod(f.shape.get(), ctx))
                .collect(),
        ),
        StructKind::Unit => return None,
    })
}

fn tag_field(tag: &str, name: &str) -> ZodField {
    ZodField {
        name: tag.to_string(),
        ty: ZodType::Literal(name.to_string()),
        optional: false,
        doc: None,
    }
}

fn enum_to_zod(et: &EnumType, shape: &'static Shape, ctx: &Ctx) -> ZodType {
    // Untagged: each variant serializes as its bare payload (unit -> the
    // variant-name string literal).
    if shape.is_untagged() {
        let members = et
            .variants
            .iter()
            .map(|v| {
                variant_payload(v, ctx)
                    .unwrap_or_else(|| ZodType::Literal(variant_name(v).to_string()))
            })
            .collect();
        return ZodType::Union(members);
    }

    match (shape.tag, shape.content) {
        // Adjacently tagged: `{ [tag]: "Variant", [content]: payload }`.
        (Some(tag), Some(content)) => {
            let members = et
                .variants
                .iter()
                .map(|v| {
                    let mut fields = vec![tag_field(tag, variant_name(v))];
                    if let Some(payload) = variant_payload(v, ctx) {
                        fields.push(ZodField {
                            name: content.to_string(),
                            ty: payload,
                            optional: false,
                            doc: None,
                        });
                    }
                    ZodType::Object(fields)
                })
                .collect();
            ZodType::Union(members)
        }
        // Internally tagged: tag merged into the variant object.
        (Some(tag), None) => {
            let members = et
                .variants
                .iter()
                .map(|v| internal_member(v, tag, ctx))
                .collect();
            ZodType::Union(members)
        }
        // Externally tagged (default): unit -> "Variant", data -> `{ Variant: payload }`.
        _ => {
            if et.variants.iter().all(|v| v.data.fields.is_empty()) {
                let names = et
                    .variants
                    .iter()
                    .map(|v| variant_name(v).to_string())
                    .collect();
                return ZodType::Enum(names);
            }
            let members = et
                .variants
                .iter()
                .map(|v| match variant_payload(v, ctx) {
                    None => ZodType::Literal(variant_name(v).to_string()),
                    Some(payload) => ZodType::Object(vec![ZodField {
                        name: variant_name(v).to_string(),
                        ty: payload,
                        optional: false,
                        doc: None,
                    }]),
                })
                .collect();
            ZodType::Union(members)
        }
    }
}

fn internal_member(v: &Variant, tag: &str, ctx: &Ctx) -> ZodType {
    let tag = tag_field(tag, variant_name(v));
    if v.data.fields.is_empty() {
        return ZodType::Object(vec![tag]);
    }
    match v.data.kind {
        StructKind::Struct => {
            let mut fields = vec![tag];
            fields.extend(struct_fields(&v.data, ctx));
            ZodType::Object(fields)
        }
        // Newtype variant: the inner value's object fields are flattened
        // alongside the tag (facet-json: `{"type":"V","a":1}`).
        StructKind::TupleStruct | StructKind::Tuple if v.data.fields.len() == 1 => {
            ZodType::Intersection(
                Box::new(ZodType::Object(vec![tag])),
                Box::new(shape_to_zod(v.data.fields[0].shape.get(), ctx)),
            )
        }
        // facet-json refuses internally-tagged multi-field tuple variants.
        _ => ZodType::Never,
    }
}

fn wrap_optional(inner: ZodType, config: &Config) -> ZodType {
    match config.optional_mode {
        crate::config::OptionalMode::Optional => ZodType::Optional(Box::new(inner)),
        crate::config::OptionalMode::Nullable => ZodType::Nullable(Box::new(inner)),
        crate::config::OptionalMode::Nullish => ZodType::Nullish(Box::new(inner)),
    }
}

/// Derive the TypeScript schema name for a given Facet [`Shape`].
///
/// Generic types are disambiguated by their concrete type arguments (e.g.
/// `Foo<Bar>` → `FooBar`, `Foo<Baz>` → `FooBaz`) so distinct instantiations do
/// not collide.
pub fn schema_name(shape: &Shape) -> String {
    let base = shape.type_identifier.to_string();
    if shape.type_params.is_empty() {
        base
    } else {
        let params: Vec<String> = shape
            .type_params
            .iter()
            .map(|tp| schema_name(tp.shape))
            .collect();
        format!("{}{}", base, params.join(""))
    }
}
