//! Canonical schema model and wire payload types for vox.
//!
//! This crate contains the transport-independent schema data model, content
//! hashing, and CBOR schema payload helpers shared by vox runtimes and codecs.

use facet::{Facet, OpaqueSerialize, PtrConst};
use facet_core::Shape;
use std::collections::HashMap;

// ============================================================================
// Schema data types
// ============================================================================

/// A content hash that uniquely identifies a type's postcard-level structure.
///
/// Computed via blake3, truncated to 64 bits. The same type always produces
/// the same TypeSchemaId regardless of connection, session, process, or
/// language.
// r[impl schema.type-id]
#[derive(Facet, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
#[facet(transparent)]
pub struct SchemaHash(pub u64);

/// Temporary index assigned during schema extraction to handle cycles in
/// recursive types. Completely unrelated to type parameters — this is purely
/// a bookkeeping index for the extraction/hashing pipeline.
#[derive(Facet, Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct CycleSchemaIndex(u64);

impl CycleSchemaIndex {
    /// The starting index for a fresh extraction pass.
    pub fn first() -> Self {
        Self(1)
    }

    /// Return the current index and advance to the next one.
    pub fn next_index(&mut self) -> Self {
        let current = *self;
        self.0 += 1;
        current
    }
}

/// The name of a generic type parameter (e.g. `"T"`, `"K"`, `"V"`).
///
/// Used in two places:
/// - `Schema::type_params` — declares the parameter names for a generic type
/// - `TypeRef::Var` — references a parameter by name at usage sites
///
/// Cannot be constructed outside this module — the only legitimate source
/// is facet's `TypeParam::name`.
#[derive(Facet, Clone, PartialEq, Eq, Hash, Debug)]
#[facet(transparent)]
pub struct TypeParamName(pub String);

impl TypeParamName {
    /// Get the type parameter name as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A reference to a type in a schema. Either a concrete type (with optional
/// type arguments for generics) or a type variable bound by the enclosing
/// generic's `type_params`.
///
/// Generic over the ID type: `TypeSchemaId` for final schemas,
/// `MixedId` during extraction.
#[derive(Facet, Clone, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
#[facet(tag = "tag", rename_all = "snake_case")]
pub enum TypeRef<Id = SchemaHash> {
    /// A concrete type, possibly generic.
    Concrete {
        type_id: Id,
        /// Type arguments for generic types. Empty for non-generic types.
        args: Vec<TypeRef<Id>>,
    },
    /// A reference to a type parameter of the enclosing generic type,
    /// by name (e.g. `TypeParamName("T")`).
    Var { name: TypeParamName },
}

impl<Id> TypeRef<Id> {
    /// Shorthand for a non-generic concrete type reference.
    pub fn concrete(type_id: Id) -> Self {
        TypeRef::Concrete {
            type_id,
            args: Vec::new(),
        }
    }

    /// Shorthand for a generic concrete type reference with type arguments.
    pub fn generic(type_id: Id, args: Vec<TypeRef<Id>>) -> Self {
        TypeRef::Concrete { type_id, args }
    }

    /// Collect all concrete IDs reachable from this TypeRef (depth-first).
    pub fn collect_ids(&self, out: &mut Vec<Id>)
    where
        Id: Copy,
    {
        match self {
            TypeRef::Concrete { type_id, args } => {
                out.push(*type_id);
                for arg in args {
                    arg.collect_ids(out);
                }
            }
            TypeRef::Var { .. } => {}
        }
    }

    /// Return the concrete type ID if this is a non-generic `Concrete` variant, panicking otherwise.
    pub fn expect_concrete_id(&self) -> &Id {
        match self {
            TypeRef::Concrete { type_id, args } if args.is_empty() => type_id,
            TypeRef::Concrete { .. } => panic!("TypeRef::expect_concrete_id: has type args"),
            TypeRef::Var { .. } => panic!("TypeRef::expect_concrete_id: is a type variable"),
        }
    }

    /// Map a `TypeRef<Id>` to `TypeRef<OtherId>` by applying `f` to every concrete ID.
    pub fn map<OtherId, F: Fn(Id) -> OtherId + Copy>(self, f: F) -> TypeRef<OtherId> {
        match self {
            TypeRef::Concrete { type_id, args } => TypeRef::Concrete {
                type_id: f(type_id),
                args: args.into_iter().map(|a| a.map(f)).collect(),
            },
            TypeRef::Var { name } => TypeRef::Var { name },
        }
    }

    /// Fallible version of `map` — applies `f` to every concrete ID, propagating errors.
    pub fn try_map<OtherId, E, F: Fn(Id) -> Result<OtherId, E> + Copy>(
        self,
        f: &F,
    ) -> Result<TypeRef<OtherId>, E> {
        match self {
            TypeRef::Concrete { type_id, args } => Ok(TypeRef::Concrete {
                type_id: f(type_id)?,
                args: args
                    .into_iter()
                    .map(|a| a.try_map(f))
                    .collect::<Result<_, _>>()?,
            }),
            TypeRef::Var { name } => Ok(TypeRef::Var { name }),
        }
    }
}

impl TypeRef {
    /// Look up the schema for this TypeRef in the registry and return
    /// the schema's kind with all type variables substituted.
    ///
    /// For non-generic types (`Concrete { args: [] }`), returns the kind as-is.
    /// For generic types, substitutes `Var` references with the concrete
    /// type arguments from this TypeRef.
    ///
    /// Returns `None` if the schema is not in the registry or this is a `Var`.
    pub fn resolve_kind(&self, registry: &SchemaRegistry) -> Option<SchemaKind> {
        match self {
            TypeRef::Var { .. } => None,
            TypeRef::Concrete { type_id, args } => {
                let schema = registry.get(type_id)?;
                if args.is_empty() {
                    return Some(schema.kind.clone());
                }
                // Build substitution map: type param name → concrete TypeRef
                let subst: HashMap<&TypeParamName, &TypeRef> =
                    schema.type_params.iter().zip(args.iter()).collect();
                let kind = schema
                    .kind
                    .clone()
                    .try_map_type_refs(&mut |tr| -> Result<TypeRef, std::convert::Infallible> {
                        Ok(match tr {
                            TypeRef::Var { ref name } => match subst.get(name) {
                                Some(concrete) => (*concrete).clone(),
                                None => tr,
                            },
                            other => other,
                        })
                    })
                    .unwrap(); // infallible
                Some(kind)
            }
        }
    }
}

/// During extraction, IDs can be either already-finalized content hashes
/// or temporary indices that will be resolved during finalization.
#[derive(Facet, Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[repr(u8)]
pub enum MixedId {
    /// A final content hash (from a previously extracted type).
    Final(SchemaHash),
    /// A temporary index assigned during the current extraction pass.
    /// Used only for cycle detection/resolution during hashing.
    Temp(CycleSchemaIndex),
}

/// The root schema type, generic over the ID representation.
#[derive(Facet, Clone, Debug)]
pub struct Schema<Id = SchemaHash> {
    /// A unique identifier for this schema (hash of its contents)
    pub id: Id,

    /// Type parameter names for generic types. Empty for non-generic types.
    #[facet(default)]
    pub type_params: Vec<TypeParamName>,

    /// The inner description of the schema, if it's a struct, an enum, etc.
    pub kind: SchemaKind<Id>,
}

impl Schema {
    /// Returns the type name for nominal types (struct/enum), or `None` for
    /// structural types (tuple, list, map, etc.).
    pub fn name(&self) -> Option<&str> {
        match &self.kind {
            SchemaKind::Struct { name, .. } | SchemaKind::Enum { name, .. } => Some(name.as_str()),
            _ => None,
        }
    }
}

/// The structural kind of a type, generic over the ID representation.
#[derive(Facet, Clone, Debug)]
#[repr(u8)]
#[facet(tag = "tag", rename_all = "snake_case")]
pub enum SchemaKind<Id = SchemaHash> {
    Struct {
        /// The type name (e.g. "Point"). Used for matching across schema
        /// versions and for diagnostics. MUST NOT be empty.
        name: String,
        fields: Vec<FieldSchema<Id>>,
    },
    Enum {
        /// The type name (e.g. "Color"). Used for matching across schema
        /// versions and for diagnostics. MUST NOT be empty.
        name: String,
        variants: Vec<VariantSchema<Id>>,
    },
    Tuple {
        elements: Vec<TypeRef<Id>>,
    },
    List {
        element: TypeRef<Id>,
    },
    Map {
        key: TypeRef<Id>,
        value: TypeRef<Id>,
    },
    Array {
        element: TypeRef<Id>,
        length: u64,
    },
    Option {
        element: TypeRef<Id>,
    },
    Channel {
        direction: ChannelDirection,
        element: TypeRef<Id>,
    },
    Primitive {
        primitive_type: PrimitiveType,
    },
}

impl<Id> SchemaKind<Id> {
    /// Visit every TypeRef in this schema kind.
    pub fn for_each_type_ref(&self, f: &mut impl FnMut(&TypeRef<Id>)) {
        match self {
            Self::Primitive { .. } => {}
            Self::Struct { fields, .. } => {
                for field in fields {
                    field.for_each_type_ref(f);
                }
            }
            Self::Enum { variants, .. } => {
                for variant in variants {
                    variant.for_each_type_ref(f);
                }
            }
            Self::Tuple { elements } => {
                for elem in elements {
                    f(elem);
                }
            }
            Self::List { element }
            | Self::Option { element }
            | Self::Array { element, .. }
            | Self::Channel { element, .. } => f(element),
            Self::Map { key, value } => {
                f(key);
                f(value);
            }
        }
    }

    /// Transform every TypeRef in this schema kind, with fallible mapping.
    pub fn try_map_type_refs<OtherId, E>(
        self,
        f: &mut impl FnMut(TypeRef<Id>) -> Result<TypeRef<OtherId>, E>,
    ) -> Result<SchemaKind<OtherId>, E> {
        Ok(match self {
            Self::Primitive { primitive_type } => SchemaKind::Primitive { primitive_type },
            Self::Struct { name, fields } => SchemaKind::Struct {
                name,
                fields: fields
                    .into_iter()
                    .map(|field| field.try_map_type_ref(f))
                    .collect::<Result<_, _>>()?,
            },
            Self::Enum { name, variants } => SchemaKind::Enum {
                name,
                variants: variants
                    .into_iter()
                    .map(|v| v.try_map_type_refs(f))
                    .collect::<Result<_, _>>()?,
            },
            Self::Tuple { elements } => SchemaKind::Tuple {
                elements: elements.into_iter().map(f).collect::<Result<_, _>>()?,
            },
            Self::List { element } => SchemaKind::List {
                element: f(element)?,
            },
            Self::Map { key, value } => SchemaKind::Map {
                key: f(key)?,
                value: f(value)?,
            },
            Self::Array { element, length } => SchemaKind::Array {
                element: f(element)?,
                length,
            },
            Self::Option { element } => SchemaKind::Option {
                element: f(element)?,
            },
            Self::Channel { direction, element } => SchemaKind::Channel {
                direction,
                element: f(element)?,
            },
        })
    }
}

impl<Id> FieldSchema<Id> {
    /// Visit the TypeRef in this field.
    pub fn for_each_type_ref(&self, f: &mut impl FnMut(&TypeRef<Id>)) {
        f(&self.type_ref);
    }

    /// Transform the TypeRef in this field.
    pub fn try_map_type_ref<OtherId, E>(
        self,
        f: &mut impl FnMut(TypeRef<Id>) -> Result<TypeRef<OtherId>, E>,
    ) -> Result<FieldSchema<OtherId>, E> {
        Ok(FieldSchema {
            name: self.name,
            type_ref: f(self.type_ref)?,
            required: self.required,
        })
    }
}

impl<Id> VariantSchema<Id> {
    /// Visit every TypeRef in this variant.
    pub fn for_each_type_ref(&self, f: &mut impl FnMut(&TypeRef<Id>)) {
        self.payload.for_each_type_ref(f);
    }

    /// Transform every TypeRef in this variant.
    pub fn try_map_type_refs<OtherId, E>(
        self,
        f: &mut impl FnMut(TypeRef<Id>) -> Result<TypeRef<OtherId>, E>,
    ) -> Result<VariantSchema<OtherId>, E> {
        Ok(VariantSchema {
            name: self.name,
            index: self.index,
            payload: self.payload.try_map_type_refs(f)?,
        })
    }
}

impl<Id> VariantPayload<Id> {
    /// Visit every TypeRef in this payload.
    pub fn for_each_type_ref(&self, f: &mut impl FnMut(&TypeRef<Id>)) {
        match self {
            Self::Unit => {}
            Self::Newtype { type_ref } => f(type_ref),
            Self::Tuple { types } => {
                for t in types {
                    f(t);
                }
            }
            Self::Struct { fields } => {
                for field in fields {
                    field.for_each_type_ref(f);
                }
            }
        }
    }

    /// Transform every TypeRef in this payload.
    pub fn try_map_type_refs<OtherId, E>(
        self,
        f: &mut impl FnMut(TypeRef<Id>) -> Result<TypeRef<OtherId>, E>,
    ) -> Result<VariantPayload<OtherId>, E> {
        Ok(match self {
            Self::Unit => VariantPayload::Unit,
            Self::Newtype { type_ref } => VariantPayload::Newtype {
                type_ref: f(type_ref)?,
            },
            Self::Tuple { types } => VariantPayload::Tuple {
                types: types.into_iter().map(f).collect::<Result<_, _>>()?,
            },
            Self::Struct { fields } => VariantPayload::Struct {
                fields: fields
                    .into_iter()
                    .map(|field| field.try_map_type_ref(f))
                    .collect::<Result<_, _>>()?,
            },
        })
    }
}

/// The direction of a channel type.
#[derive(Facet, Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
#[facet(tag = "tag", rename_all = "snake_case")]
pub enum ChannelDirection {
    /// A sending channel (`Tx<T>`).
    Tx,
    /// A receiving channel (`Rx<T>`).
    Rx,
}

/// Type aliases for schemas during extraction (mixed temp/final IDs).
pub type MixedSchema = Schema<MixedId>;
pub type MixedSchemaKind = SchemaKind<MixedId>;

/// Describes a single field in a struct or struct variant.
#[derive(Facet, Clone, Debug)]
pub struct FieldSchema<Id = SchemaHash> {
    pub name: String,
    pub type_ref: TypeRef<Id>,
    pub required: bool,
}

/// Describes a single variant in an enum.
#[derive(Facet, Clone, Debug)]
pub struct VariantSchema<Id = SchemaHash> {
    pub name: String,
    pub index: u32,
    pub payload: VariantPayload<Id>,
}

/// The payload of an enum variant.
#[derive(Facet, Clone, Debug)]
#[repr(u8)]
#[facet(tag = "tag", rename_all = "snake_case")]
pub enum VariantPayload<Id = SchemaHash> {
    Unit,
    Newtype { type_ref: TypeRef<Id> },
    Tuple { types: Vec<TypeRef<Id>> },
    Struct { fields: Vec<FieldSchema<Id>> },
}

/// Primitive types supported by the wire format.
#[derive(Facet, Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
#[facet(tag = "tag", rename_all = "snake_case")]
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
    Never,
    Bytes,
    /// An opaque payload — a length-prefixed byte sequence whose
    /// length prefix is a little-endian u32 (not a varint like other
    /// postcard sequences).
    Payload,
}

// ============================================================================
// Content hashing — r[schema.type-id.hash]
// ============================================================================

impl PrimitiveType {
    /// The tag string used for hashing this primitive type.
    fn hash_tag(self) -> &'static str {
        match self {
            PrimitiveType::Bool => "bool",
            PrimitiveType::U8 => "u8",
            PrimitiveType::U16 => "u16",
            PrimitiveType::U32 => "u32",
            PrimitiveType::U64 => "u64",
            PrimitiveType::U128 => "u128",
            PrimitiveType::I8 => "i8",
            PrimitiveType::I16 => "i16",
            PrimitiveType::I32 => "i32",
            PrimitiveType::I64 => "i64",
            PrimitiveType::I128 => "i128",
            PrimitiveType::F32 => "f32",
            PrimitiveType::F64 => "f64",
            PrimitiveType::Char => "char",
            PrimitiveType::String => "string",
            PrimitiveType::Unit => "unit",
            PrimitiveType::Never => "never",
            PrimitiveType::Bytes => "bytes",
            PrimitiveType::Payload => "payload",
        }
    }
}

/// Context for computing content hashes of schemas.
///
/// Generic over the ID type so it works with both `MixedId` (during extraction)
/// and `TypeSchemaId` (for already-finalized schemas).
struct SchemaHasher<'a, Id: Copy> {
    hasher: blake3::Hasher,
    resolve: &'a dyn Fn(Id) -> SchemaHash,
}

impl<'a, Id: Copy> SchemaHasher<'a, Id> {
    fn new(resolve: &'a dyn Fn(Id) -> SchemaHash) -> Self {
        Self {
            hasher: blake3::Hasher::new(),
            resolve,
        }
    }

    fn feed_string(&mut self, s: &str) {
        self.hasher.update(&(s.len() as u32).to_le_bytes());
        self.hasher.update(s.as_bytes());
    }

    fn feed_type_ref(&mut self, tr: &TypeRef<Id>) {
        match tr {
            TypeRef::Concrete { type_id, args } => {
                self.feed_string("concrete");
                let resolved = (self.resolve)(*type_id);
                self.hasher.update(&resolved.0.to_le_bytes());
                if !args.is_empty() {
                    self.feed_string("args");
                    for arg in args {
                        self.feed_type_ref(arg);
                    }
                }
            }
            TypeRef::Var { name } => {
                self.feed_string("var");
                self.feed_string(&name.0);
            }
        }
    }

    // r[impl schema.type-id.hash.primitives]
    // r[impl schema.type-id.hash.struct]
    // r[impl schema.type-id.hash.enum]
    // r[impl schema.type-id.hash.container]
    // r[impl schema.type-id.hash.tuple]
    fn feed_schema(&mut self, kind: &SchemaKind<Id>, type_params: &[TypeParamName]) {
        match kind {
            SchemaKind::Primitive { primitive_type } => {
                self.feed_string(primitive_type.hash_tag());
            }
            SchemaKind::Struct { name, fields } => {
                self.feed_string("struct");
                self.feed_string(name);
                self.hasher
                    .update(&(type_params.len() as u32).to_le_bytes());
                for tp in type_params {
                    self.feed_string(&tp.0);
                }
                for field in fields {
                    self.feed_string(&field.name);
                    self.feed_type_ref(&field.type_ref);
                }
            }
            SchemaKind::Enum { name, variants } => {
                self.feed_string("enum");
                self.feed_string(name);
                self.hasher
                    .update(&(type_params.len() as u32).to_le_bytes());
                for tp in type_params {
                    self.feed_string(&tp.0);
                }
                for variant in variants {
                    self.feed_string(&variant.name);
                    self.hasher.update(&variant.index.to_le_bytes());
                    match &variant.payload {
                        VariantPayload::Unit => {
                            self.feed_string("unit");
                        }
                        VariantPayload::Newtype { type_ref } => {
                            self.feed_string("newtype");
                            self.feed_type_ref(type_ref);
                        }
                        VariantPayload::Tuple { types } => {
                            self.feed_string("tuple");
                            for tr in types {
                                self.feed_type_ref(tr);
                            }
                        }
                        VariantPayload::Struct { fields } => {
                            self.feed_string("struct");
                            for field in fields {
                                self.feed_string(&field.name);
                                self.feed_type_ref(&field.type_ref);
                            }
                        }
                    }
                }
            }
            SchemaKind::Tuple { elements } => {
                self.feed_string("tuple");
                for elem in elements {
                    self.feed_type_ref(elem);
                }
            }
            SchemaKind::List { element } => {
                self.feed_string("list");
                self.feed_type_ref(element);
            }
            SchemaKind::Map { key, value } => {
                self.feed_string("map");
                self.feed_type_ref(key);
                self.feed_type_ref(value);
            }
            SchemaKind::Array { element, length } => {
                self.feed_string("array");
                self.feed_type_ref(element);
                self.hasher.update(&length.to_le_bytes());
            }
            SchemaKind::Option { element } => {
                self.feed_string("option");
                self.feed_type_ref(element);
            }
            SchemaKind::Channel { direction, element } => {
                self.feed_string("channel");
                self.feed_string(match direction {
                    ChannelDirection::Tx => "send",
                    ChannelDirection::Rx => "recv",
                });
                self.feed_type_ref(element);
            }
        }
    }

    fn finalize(self) -> SchemaHash {
        let hash = self.hasher.finalize();
        let bytes: [u8; 8] = hash.as_bytes()[0..8].try_into().expect("slice len");
        SchemaHash(u64::from_le_bytes(bytes))
    }
}

/// Compute the content hash of a schema, given a resolver for child type IDs.
pub fn compute_content_hash<Id: Copy>(
    kind: &SchemaKind<Id>,
    type_params: &[TypeParamName],
    resolve: &dyn Fn(Id) -> SchemaHash,
) -> SchemaHash {
    let mut hasher = SchemaHasher::new(resolve);
    hasher.feed_schema(kind, type_params);
    hasher.finalize()
}

/// Collect all TypeSchemaIds directly referenced by a SchemaKind.
pub fn schema_child_ids(kind: &SchemaKind) -> Vec<SchemaHash> {
    let mut refs = Vec::new();
    kind.for_each_type_ref(&mut |tr| tr.collect_ids(&mut refs));
    refs
}

/// CBOR-encoded schema payload (schemas + method bindings).
///
/// Newtype over `Vec<u8>` so the type system distinguishes raw bytes from
/// CBOR-encoded schema data. Empty when no new schemas need to be sent.
#[derive(Facet, Clone, Debug, Default)]
#[repr(transparent)]
#[facet(transparent)]
pub struct CborPayload(pub Vec<u8>);

impl CborPayload {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Lookup table mapping TypeSchemaId → Schema, used for resolving type
/// references during deserialization with translation plans.
pub type SchemaRegistry = HashMap<SchemaHash, Schema>;

/// Build a SchemaRegistry from a list of schemas.
pub fn build_registry(schemas: &[Schema]) -> SchemaRegistry {
    schemas.iter().map(|s| (s.id, s.clone())).collect()
}

/// Anything that can look up schemas by their content hash.
///
/// Implemented by SchemaRegistry (HashMap), the operation store, etc.
/// Used by the send tracker to source schemas without caring where they
/// come from.
pub trait SchemaSource {
    fn get_schema(&self, id: SchemaHash) -> Option<Schema>;
}

impl SchemaSource for SchemaRegistry {
    fn get_schema(&self, id: SchemaHash) -> Option<Schema> {
        self.get(&id).cloned()
    }
}

/// Whether a method schema binding describes args or the response type.
#[derive(Facet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
#[facet(tag = "tag", rename_all = "snake_case")]
pub enum BindingDirection {
    /// The sender will send data of this type as method arguments.
    Args,

    /// The sender will send data of this type as the method response.
    Response,
}

/// CBOR-encoded payload inside a schema wire message.
/// A struct so new fields can be added without breaking the wire format.
#[derive(Facet, Clone, Debug)]
pub struct SchemaPayload {
    /// All schemas we're sending over. Sending the schema twice is a
    /// protocol error: peers must bail out.
    pub schemas: Vec<Schema>,

    /// Hash of the schema of the type corresponding to this method.
    /// When attached to `RequestCall`, this is the args tuple, e.g.
    /// for `add(a: u32, b: u32) -> u64` it's `(u32, u32)`.
    /// For `RequestResponse` it's `u64` in thea bove example.
    pub root: TypeRef,
}

impl SchemaPayload {
    /// CBOR-encode this prepared message for embedding in RequestCall/RequestResponse.
    pub fn to_cbor(&self) -> CborPayload {
        CborPayload(facet_cbor::to_vec(self).expect("schema CBOR serialization should not fail"))
    }

    /// Parse a CBOR-encoded schema message from bytes.
    pub fn from_cbor(bytes: &[u8]) -> Result<SchemaPayload, facet_cbor::CborError> {
        facet_cbor::from_slice(bytes)
    }
}

/// Transparent wrapper around borrowed bytes that are already postcard-encoded.
/// Used as a sentinel type for passthrough detection in serializers.
#[repr(transparent)]
pub struct RawPostcardBorrowed<'a>(pub &'a [u8]);

/// Sentinel shape for borrowed passthrough bytes. Serializers check against
/// this to distinguish pre-encoded bytes from regular `&[u8]`/`Vec<u8>` values.
pub static RAW_POSTCARD_BORROWED_SHAPE: Shape =
    Shape::builder_for_sized::<RawPostcardBorrowed<'static>>("RawPostcardBorrowed").build();

/// Create an `OpaqueSerialize` for already-encoded postcard bytes.
/// The serializer detects the sentinel shape and writes bytes directly (passthrough).
pub fn opaque_encoded_borrowed(bytes: &&[u8]) -> OpaqueSerialize {
    OpaqueSerialize {
        ptr: PtrConst::new((bytes as *const &[u8]).cast::<RawPostcardBorrowed<'_>>()),
        shape: &RAW_POSTCARD_BORROWED_SHAPE,
    }
}
