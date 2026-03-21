//! Schema types, extraction, and tracking for roam wire protocol.
//!
//! This module contains:
//! - Schema data types (TypeSchemaId, Schema, SchemaKind, etc.)
//! - CBOR serialization for schema messages
//! - Schema extraction from facet Shape graphs
//! - SchemaSendTracker for outbound dedup (owned by SessionCore)
//! - SchemaRecvTracker for inbound storage (shared via Arc)

use facet::Facet;
use facet_core::{DeclId, Def, ScalarType, Shape, StructKind, Type, UserType};
use indexmap::IndexMap;
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use crate::{MethodId, RequestCall, RequestResponse, is_rx, is_tx};

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
    fn first() -> Self {
        Self(1)
    }

    /// Return the current index and advance to the next one.
    fn next(&mut self) -> Self {
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
pub struct TypeParamName(String);

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
pub(crate) type MixedSchema = Schema<MixedId>;
pub(crate) type MixedSchemaKind = SchemaKind<MixedId>;

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

// ============================================================================
// Schema extraction
// ============================================================================

/// Errors that can occur during schema extraction.
#[derive(Debug)]
pub enum SchemaExtractError {
    /// Encountered a type that schema extraction doesn't know how to handle.
    UnhandledType { type_desc: String },

    /// A pointer type had no type_params to follow.
    PointerWithoutTypeParams { shape_desc: String },

    /// A temporary ID was not resolved during finalization.
    UnresolvedTempId { temp_id: CycleSchemaIndex },

    /// A DeclId was expected in the assigned map but wasn't found.
    MissingAssignment { context: String },
}

impl std::fmt::Display for SchemaExtractError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnhandledType { type_desc } => {
                write!(f, "schema extraction: unhandled type: {type_desc}")
            }
            Self::PointerWithoutTypeParams { shape_desc } => {
                write!(
                    f,
                    "schema extraction: Pointer type without type_params: {shape_desc}"
                )
            }
            Self::UnresolvedTempId { temp_id } => {
                write!(
                    f,
                    "schema extraction: unresolved temp ID {temp_id:?} during finalization"
                )
            }
            Self::MissingAssignment { context } => {
                write!(f, "schema extraction: missing DeclId assignment: {context}")
            }
        }
    }
}

/// A value for which a schema can be attached
pub trait Schematic {
    fn direction(&self) -> BindingDirection;
    fn attach_schemas(&mut self, schemas: CborPayload);
}

impl<'payload> Schematic for RequestCall<'payload> {
    fn direction(&self) -> BindingDirection {
        BindingDirection::Args
    }

    fn attach_schemas(&mut self, schemas: CborPayload) {
        self.schemas = schemas;
    }
}

impl<'payload> Schematic for RequestResponse<'payload> {
    fn direction(&self) -> BindingDirection {
        BindingDirection::Response
    }

    fn attach_schemas(&mut self, schemas: CborPayload) {
        self.schemas = schemas;
    }
}

impl std::error::Error for SchemaExtractError {}

// ============================================================================
// SchemaSendTracker — outbound dedup, owned by SessionCore (no Arc, no Mutex)
// ============================================================================

/// Tracks which schemas have been sent on the current connection.
///
/// Plain struct — owned by `SessionCore` behind the same Mutex as the
/// conduit tx. Reset on reconnection.
// r[impl schema.tracking.sent]
// r[impl schema.type-id.per-connection]
pub struct SchemaSendTracker {
    /// Per-method, per-direction: the CborPayload that was sent. Keyed by
    /// (method_id, direction). If present, schemas were already sent.
    sent_bindings: HashSet<(MethodId, BindingDirection)>,

    /// SchemaHashes already sent on this connection.
    sent_schemas: HashSet<SchemaHash>,

    /// All extracted schemas, kept for the operation store to pull from.
    registry: SchemaRegistry,
}

impl SchemaSendTracker {
    pub fn new() -> Self {
        SchemaSendTracker {
            registry: HashMap::new(),
            sent_bindings: HashSet::new(),
            sent_schemas: HashSet::new(),
        }
    }

    /// Reset connection-scoped state — call on reconnection.
    /// The registry is preserved (schemas don't change across connections).
    pub fn reset(&mut self) {
        self.sent_bindings.clear();
        self.sent_schemas.clear();
    }

    /// Borrow the schema registry. Used by the operation store to pull
    /// schemas it hasn't stored yet.
    pub fn registry(&self) -> &SchemaRegistry {
        &self.registry
    }

    /// Prepare schemas for a method call/response, returning a CBOR payload
    /// to inline in the request/response. Returns empty payload if schemas
    /// were already sent for this shape.
    ///
    // r[impl schema.tracking.transitive]
    // r[impl schema.exchange.idempotent]
    // r[impl schema.principles.once-per-type]
    // r[impl schema.principles.sender-driven]
    // r[impl schema.principles.no-roundtrips]
    pub fn attach_schemas_for_shape_if_needed(
        &mut self,
        method_id: MethodId,
        shape: &'static Shape,
        schematic: &mut impl Schematic,
    ) -> Result<CborPayload, SchemaExtractError> {
        let key = (method_id, schematic.direction());

        // Fast path: already sent for this method+direction.
        if self.sent_bindings.contains(&key) {
            let empty = CborPayload::default();
            schematic.attach_schemas(empty.clone());
            return Ok(empty);
        }

        // Slow path: extract, deduplicate, encode.
        let already_sent = self.sent_schemas.clone();

        // Extraction is intentionally fresh and pure here. Wire dedup happens
        // on content hashes, not DeclId caching, to avoid aliasing structural
        // roots like unary tuples across unrelated method shapes.
        let extracted = extract_schemas(shape)?;

        // Add all schemas to the persistent registry (for the operation store).
        for schema in &extracted.schemas {
            self.registry
                .entry(schema.id)
                .or_insert_with(|| schema.clone());
        }

        // Filter to only schemas not already sent on the wire.
        let unsent: Vec<Schema> = extracted
            .schemas
            .into_iter()
            .filter(|s| !already_sent.contains(&s.id))
            .collect();

        // Track sent schemas.
        for s in &unsent {
            self.sent_schemas.insert(s.id);
        }

        let schema_payload = SchemaPayload {
            schemas: unsent,
            root: extracted.root,
        };
        dlog!(
            "[schema] send binding: method={:?} direction={:?} root={:?} schema_count={}",
            method_id,
            schematic.direction(),
            schema_payload.root,
            schema_payload.schemas.len()
        );
        let cbor = schema_payload.to_cbor();
        schematic.attach_schemas(cbor.clone());
        self.sent_bindings.insert(key);
        Ok(cbor)
    }

    /// Prepare schemas for sending, sourcing them from a `SchemaSource`.
    ///
    /// Used for replay paths where we don't have a live value shape but do
    /// have the bound root `TypeRef` and a schema source.
    pub fn prepare_send(
        &mut self,
        method_id: MethodId,
        direction: BindingDirection,
        root_type: &TypeRef,
        source: &dyn SchemaSource,
    ) -> CborPayload {
        let key = (method_id, direction);
        if self.sent_bindings.contains(&key) {
            return CborPayload::default();
        }

        let already_sent = self.sent_schemas.clone();
        let mut all_schemas = Vec::new();
        let mut visited = HashSet::new();
        let mut queue = Vec::new();
        root_type.collect_ids(&mut queue);

        while let Some(id) = queue.pop() {
            if !visited.insert(id) {
                continue;
            }
            if let Some(schema) = source.get_schema(id) {
                for child_id in schema_child_ids(&schema.kind) {
                    queue.push(child_id);
                }
                all_schemas.push(schema);
            }
        }

        for schema in &all_schemas {
            self.registry
                .entry(schema.id)
                .or_insert_with(|| schema.clone());
        }

        let unsent: Vec<Schema> = all_schemas
            .into_iter()
            .filter(|schema| !already_sent.contains(&schema.id))
            .collect();

        for schema in &unsent {
            self.sent_schemas.insert(schema.id);
        }

        let schema_payload = SchemaPayload {
            schemas: unsent,
            root: root_type.clone(),
        };
        dlog!(
            "[schema] resend binding: method={:?} direction={:?} root={:?} schema_count={}",
            method_id,
            direction,
            schema_payload.root,
            schema_payload.schemas.len()
        );
        let cbor = schema_payload.to_cbor();
        self.sent_bindings.insert(key);
        cbor
    }

    /// Compatibility shim: schema extraction is now independent from
    /// connection-scoped send tracking.
    pub fn extract_schemas(
        &mut self,
        shape: &'static Shape,
    ) -> Result<ExtractedSchemas, SchemaExtractError> {
        self::extract_schemas(shape)
    }
}

impl Default for SchemaSendTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for SchemaSendTracker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SchemaSendTracker").finish_non_exhaustive()
    }
}

// ============================================================================
// SchemaRecvTracker — inbound storage, shared via Arc
// ============================================================================

/// Tracks schemas received from the remote peer on the current connection.
///
/// Uses interior mutability (Mutex) so it can be shared via `Arc` between the
/// session recv loop and in-flight handler tasks. Created fresh on each
/// connection — NOT reused across reconnections.
// r[impl schema.tracking.received]
// r[impl schema.type-id.per-connection]
pub struct SchemaRecvTracker {
    /// Type schemas received from the remote peer.
    received: Mutex<HashMap<SchemaHash, Schema>>,
    /// Args bindings received: method_id → root TypeRef for args.
    received_args_bindings: Mutex<HashMap<MethodId, TypeRef>>,
    /// Response bindings received: method_id → root TypeRef for response.
    received_response_bindings: Mutex<HashMap<MethodId, TypeRef>>,
}

/// Error returned when recording received schemas detects a protocol violation.
#[derive(Debug)]
pub struct DuplicateSchemaError {
    pub type_id: SchemaHash,
}

impl std::fmt::Display for DuplicateSchemaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "duplicate TypeSchemaId {:?} received on same connection — protocol error",
            self.type_id
        )
    }
}

impl std::error::Error for DuplicateSchemaError {}

impl SchemaRecvTracker {
    pub fn new() -> Self {
        SchemaRecvTracker {
            received: Mutex::new(HashMap::new()),
            received_args_bindings: Mutex::new(HashMap::new()),
            received_response_bindings: Mutex::new(HashMap::new()),
        }
    }

    /// Record a parsed schema message from the remote peer.
    ///
    /// Returns `Err` if a TypeSchemaId was already received — this is a
    /// protocol error (the send tracker didn't reset on reconnection).
    pub fn record_received(
        &self,
        method_id: MethodId,
        direction: BindingDirection,
        payload: SchemaPayload,
    ) -> Result<(), DuplicateSchemaError> {
        {
            let mut received = self.received.lock().unwrap();
            for schema in &payload.schemas {
                dlog!("[schema] record_received: id={:?}", schema.id);
            }
            for schema in payload.schemas {
                if let Some(existing) = received.get(&schema.id) {
                    dlog!(
                        "[schema] DUPLICATE: id={:?} existing={:?} new={:?}",
                        schema.id,
                        existing,
                        schema
                    );
                    return Err(DuplicateSchemaError { type_id: schema.id });
                }
                received.insert(schema.id, schema);
            }
        }
        let map = match direction {
            BindingDirection::Args => &self.received_args_bindings,
            BindingDirection::Response => &self.received_response_bindings,
        };
        dlog!(
            "[schema] record binding: method={:?} direction={:?} root={:?}",
            method_id,
            direction,
            payload.root
        );
        map.lock().unwrap().insert(method_id, payload.root);
        Ok(())
    }

    /// Look up the remote's root TypeRef for a method's args.
    pub fn get_remote_args_root(&self, method_id: MethodId) -> Option<TypeRef> {
        self.received_args_bindings
            .lock()
            .unwrap()
            .get(&method_id)
            .cloned()
    }

    /// Look up the remote's root TypeRef for a method's response.
    pub fn get_remote_response_root(&self, method_id: MethodId) -> Option<TypeRef> {
        self.received_response_bindings
            .lock()
            .unwrap()
            .get(&method_id)
            .cloned()
    }

    /// Look up a received schema by type ID.
    pub fn get_received(&self, type_id: &SchemaHash) -> Option<Schema> {
        self.received.lock().unwrap().get(type_id).cloned()
    }

    /// Get a snapshot of the received schema registry for building translation plans.
    pub fn received_registry(&self) -> SchemaRegistry {
        self.received.lock().unwrap().clone()
    }
}

impl Default for SchemaRecvTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl SchemaSource for SchemaRecvTracker {
    fn get_schema(&self, id: SchemaHash) -> Option<Schema> {
        self.get_received(&id)
    }
}

impl std::fmt::Debug for SchemaRecvTracker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SchemaRecvTracker").finish_non_exhaustive()
    }
}

/// Result of schema extraction: the schemas and the root TypeRef.
pub struct ExtractedSchemas {
    /// All schemas in dependency order (dependencies before dependents).
    pub schemas: Vec<Schema>,

    /// The root TypeRef — may be generic (e.g. `Concrete { id: result_id, args: [i64, MathError] }`).
    pub root: TypeRef,
}

/// Extract schemas without a tracker (uses a temporary counter).
/// Useful for tests and one-off schema extraction.
pub fn extract_schemas(shape: &'static Shape) -> Result<ExtractedSchemas, SchemaExtractError> {
    let mut ctx = ExtractCtx {
        next_id: CycleSchemaIndex::first(),
        schemas: IndexMap::new(),
        assigned: HashMap::new(),
        seen: HashSet::new(),
    };
    let root_mixed_ref = ctx.extract(shape)?;
    let schemas: Vec<MixedSchema> = ctx.schemas.into_values().collect();
    let (finalized, temp_to_final) = finalize_content_hashes(schemas)?;

    let resolve = |mid: MixedId| -> SchemaHash {
        match mid {
            MixedId::Final(tid) => tid,
            MixedId::Temp(t) => temp_to_final.get(&t).copied().unwrap_or(SchemaHash(0)),
        }
    };
    let root_type_ref = root_mixed_ref.map(resolve);

    Ok(ExtractedSchemas {
        schemas: finalized,
        root: root_type_ref,
    })
}

/// Replace temporary incrementing IDs with blake3 content hashes.
///
/// Schemas must be in dependency order (dependencies before dependents).
/// For non-recursive types, this is a simple bottom-up pass. For recursive
/// types, the 4-step algorithm from r[schema.hash.recursive] is used.
// r[impl schema.type-id.hash]
// r[impl schema.hash.recursive]
/// Resolve a MixedId to a TypeSchemaId for hashing purposes.
fn resolve_mixed(id: MixedId, temp_to_final: &HashMap<CycleSchemaIndex, SchemaHash>) -> SchemaHash {
    match id {
        MixedId::Final(tid) => tid,
        MixedId::Temp(t) => temp_to_final.get(&t).copied().unwrap_or(SchemaHash(0)),
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum ExtractKey {
    Decl(DeclId),
    AnonymousTupleArity(usize),
}

/// Convert a Vec<MixedSchema> (from extraction) into Vec<Schema> with
/// content-hashed TypeSchemaIds.
///
/// Schemas must be in dependency order (dependencies before dependents).
/// For non-recursive types, this is a simple bottom-up pass. For recursive
/// types, the 4-step algorithm from r[schema.hash.recursive] is used.
///
/// Returns the finalized schemas and a mapping from temp IDs to final IDs.
// r[impl schema.type-id.hash]
// r[impl schema.hash.recursive]
fn finalize_content_hashes(
    schemas: Vec<MixedSchema>,
) -> Result<(Vec<Schema>, HashMap<CycleSchemaIndex, SchemaHash>), SchemaExtractError> {
    // Only Temp entries need hashing. Build index of temp IDs.
    let temp_to_idx: HashMap<CycleSchemaIndex, usize> = schemas
        .iter()
        .enumerate()
        .filter_map(|(i, s)| match s.id {
            MixedId::Temp(t) => Some((t, i)),
            MixedId::Final(_) => None,
        })
        .collect();

    fn collect_refs(kind: &MixedSchemaKind) -> Vec<MixedId> {
        let mut refs = Vec::new();
        kind.for_each_type_ref(&mut |tr| tr.collect_ids(&mut refs));
        refs
    }

    // Detect recursive groups among temp schemas.
    let n = schemas.len();
    let mut in_recursive_group: Vec<bool> = vec![false; n];

    for (i, schema) in schemas.iter().enumerate() {
        if matches!(schema.id, MixedId::Final(_)) {
            continue; // Already finalized, skip.
        }
        for r in collect_refs(&schema.kind) {
            if let MixedId::Temp(t) = r
                && let Some(&ref_idx) = temp_to_idx.get(&t)
                && ref_idx >= i
            {
                in_recursive_group[i] = true;
                in_recursive_group[ref_idx] = true;
            }
        }
    }

    // Map from temp ID -> final content hash.
    let mut temp_to_final: HashMap<CycleSchemaIndex, SchemaHash> = HashMap::new();

    // Phase 1: Hash non-recursive temp types bottom-up.
    for (i, schema) in schemas.iter().enumerate() {
        if in_recursive_group[i] {
            continue;
        }
        if let MixedId::Temp(temp) = schema.id {
            let final_id = compute_content_hash(&schema.kind, &schema.type_params, &|mid| {
                resolve_mixed(mid, &temp_to_final)
            });
            temp_to_final.insert(temp, final_id);
        }
    }

    // Phase 2: Hash recursive groups using the 4-step algorithm.
    let mut i = 0;
    while i < n {
        if !in_recursive_group[i] {
            i += 1;
            continue;
        }

        let group_start = i;
        while i < n && in_recursive_group[i] {
            i += 1;
        }
        let group_end = i;

        // Collect the temp IDs in this group.
        let group_temp_ids: HashSet<CycleSchemaIndex> = schemas[group_start..group_end]
            .iter()
            .filter_map(|s| match s.id {
                MixedId::Temp(t) => Some(t),
                _ => None,
            })
            .collect();

        // Step 1: Preliminary hashes — intra-group refs become sentinel (0).
        let mut prelim_hashes: Vec<SchemaHash> = Vec::new();
        for schema in &schemas[group_start..group_end] {
            let prelim =
                compute_content_hash(&schema.kind, &schema.type_params, &|mid| match mid {
                    MixedId::Final(tid) => tid,
                    MixedId::Temp(t) => {
                        if group_temp_ids.contains(&t) {
                            SchemaHash(0) // sentinel
                        } else {
                            temp_to_final.get(&t).copied().unwrap_or(SchemaHash(0))
                        }
                    }
                });
            prelim_hashes.push(prelim);
        }

        // Step 3: Canonical ordering.
        let mut order: Vec<usize> = (0..prelim_hashes.len()).collect();
        order.sort_by_key(|&i| prelim_hashes[i].0);

        // Step 4: Final hashes.
        let mut group_hasher = blake3::Hasher::new();
        for &idx in &order {
            group_hasher.update(&prelim_hashes[idx].0.to_le_bytes());
        }
        let gh = group_hasher.finalize();
        let group_hash = u64::from_le_bytes(gh.as_bytes()[0..8].try_into().unwrap());

        for (position, &idx) in order.iter().enumerate() {
            let mut fh = blake3::Hasher::new();
            fh.update(&group_hash.to_le_bytes());
            fh.update(&(position as u64).to_le_bytes());
            let fo = fh.finalize();
            let final_hash =
                SchemaHash(u64::from_le_bytes(fo.as_bytes()[0..8].try_into().unwrap()));

            if let MixedId::Temp(t) = schemas[group_start + idx].id {
                temp_to_final.insert(t, final_hash);
            }
        }
    }

    // Phase 3: Convert MixedSchema -> Schema by resolving all MixedIds.
    let resolve = |mid: MixedId| -> Result<SchemaHash, SchemaExtractError> {
        match mid {
            MixedId::Final(tid) => Ok(tid),
            MixedId::Temp(t) => temp_to_final
                .get(&t)
                .copied()
                .ok_or(SchemaExtractError::UnresolvedTempId { temp_id: t }),
        }
    };

    let mut resolve_type_ref =
        |type_ref: TypeRef<MixedId>| -> Result<TypeRef<SchemaHash>, SchemaExtractError> {
            type_ref.try_map(&resolve)
        };

    let mut seen_ids = HashSet::new();
    let finalized: Vec<Schema> = schemas
        .into_iter()
        .map(|s| {
            let type_id = resolve(s.id)?;
            Ok(Schema {
                id: type_id,
                type_params: s.type_params,
                kind: s.kind.try_map_type_refs(&mut resolve_type_ref)?,
            })
        })
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .filter(|s| seen_ids.insert(s.id))
        .collect();

    Ok((finalized, temp_to_final))
}

struct ExtractCtx {
    /// Counter for assigning temp IDs
    next_id: CycleSchemaIndex,
    /// Schemas being built in this extraction pass, keyed by extraction identity.
    /// Insertion order is dependency order.
    schemas: IndexMap<ExtractKey, MixedSchema>,
    /// ExtractKey → MixedId for types we've started extracting (may not be
    /// fully built yet — needed for cycle references).
    assigned: HashMap<ExtractKey, MixedId>,
    /// Shapes we've started walking. If we encounter a shape already in
    /// this set, we're in a cycle.
    seen: HashSet<&'static Shape>,
}

impl ExtractCtx {
    /// Get or assign a MixedId for an extraction key.
    fn id_for_key(&mut self, key: ExtractKey) -> MixedId {
        if let Some(&id) = self.assigned.get(&key) {
            return id;
        }
        let id = MixedId::Temp(self.next_id.next());
        self.assigned.insert(key, id);
        id
    }

    /// Emit a schema for an extraction key (if not already emitted in this pass).
    fn emit_schema(&mut self, key: ExtractKey, schema: MixedSchema) {
        self.schemas.entry(key).or_insert(schema);
    }

    fn key_for_shape(&self, shape: &'static Shape) -> ExtractKey {
        match anonymous_tuple_arity(shape) {
            Some(arity) => ExtractKey::AnonymousTupleArity(arity),
            None => ExtractKey::Decl(shape.decl_id),
        }
    }

    /// Build a TypeRef for a field/element shape, substituting Var references
    /// for shapes that match a type parameter.
    fn type_ref_for_shape(
        &mut self,
        shape: &'static Shape,
        param_map: &HashMap<*const Shape, TypeParamName>,
    ) -> Result<TypeRef<MixedId>, SchemaExtractError> {
        let ptr = shape as *const Shape;
        if let Some(name) = param_map.get(&ptr) {
            // This shape is a type parameter — emit Var reference.
            // But we still need to extract the concrete type's schema.
            self.extract(shape)?;
            Ok(TypeRef::Var { name: name.clone() })
        } else {
            self.extract(shape)
        }
    }

    /// Extract a schema for the given shape, returning a TypeRef to it.
    /// Recursively extracts dependencies first.
    fn extract(&mut self, shape: &'static Shape) -> Result<TypeRef<MixedId>, SchemaExtractError> {
        // Channel types: emit a Channel schema with direction and element type.
        if is_tx(shape) || is_rx(shape) {
            let direction = if is_tx(shape) {
                ChannelDirection::Tx
            } else {
                ChannelDirection::Rx
            };
            if let Some(inner) = shape.type_params.first() {
                let elem_ref = self.extract(inner.shape)?;
                let key = self.key_for_shape(shape);
                let id = self.id_for_key(key);
                // For channels, the element in the schema body uses Var("T")
                // since channels are generic over their element type.
                let type_params = vec![TypeParamName("T".to_string())];
                self.emit_schema(
                    key,
                    MixedSchema {
                        id,
                        type_params,
                        kind: SchemaKind::Channel {
                            direction,
                            element: TypeRef::Var {
                                name: TypeParamName("T".to_string()),
                            },
                        },
                    },
                );
                self.seen.insert(shape);
                return Ok(TypeRef::Concrete {
                    type_id: id,
                    args: vec![elem_ref],
                });
            }
        }

        // Transparent wrappers: follow inner.
        if shape.is_transparent()
            && let Some(inner) = shape.inner
        {
            return self.extract(inner);
        }

        // Pointer types (Box, Arc, etc.): follow through to pointee.
        // Must be before id_for_decl to avoid orphaned temp IDs.
        if let Def::Pointer(ptr_def) = shape.def
            && let Some(pointee) = ptr_def.pointee
        {
            return self.extract(pointee);
        }

        let key = self.key_for_shape(shape);
        let id = self.id_for_key(key);

        // r[impl schema.format.recursive]
        // Cycle detection: if we've already started walking this shape,
        // return the assigned id without re-entering.
        if !self.seen.insert(shape) {
            // Already seen — either fully processed or a cycle.
            // Extract type args if generic (they may contain new types).
            let args = self.extract_instantiation_args(shape)?;
            return Ok(if args.is_empty() {
                TypeRef::concrete(id)
            } else {
                TypeRef::generic(id, args)
            });
        }

        // If we've already emitted a schema for this extraction key (in this pass),
        // we still need to extract type args for this particular instantiation.
        let already_emitted = self.schemas.contains_key(&key);
        if already_emitted {
            let args = self.extract_instantiation_args(shape)?;
            return Ok(if args.is_empty() {
                TypeRef::concrete(id)
            } else {
                TypeRef::generic(id, args)
            });
        }

        // Build a map from shape pointer → type param name for this type.
        // Used to emit Var references in the schema body.
        let param_map: HashMap<*const Shape, TypeParamName> = shape
            .type_params
            .iter()
            .map(|tp| (tp.shape as *const Shape, TypeParamName(tp.name.to_string())))
            .collect();
        let type_param_names: Vec<TypeParamName> = shape
            .type_params
            .iter()
            .map(|tp| TypeParamName(tp.name.to_string()))
            .collect();

        // r[impl schema.format.primitive]
        // Scalars
        if let Some(scalar) = shape.scalar_type() {
            self.emit_schema(
                key,
                MixedSchema {
                    id,
                    type_params: vec![],
                    kind: SchemaKind::Primitive {
                        primitive_type: scalar_to_primitive(scalar),
                    },
                },
            );
            return Ok(TypeRef::concrete(id));
        }

        // r[impl schema.format.container]
        // Containers
        match shape.def {
            Def::List(list_def) => {
                if let Some(ScalarType::U8) = list_def.t().scalar_type() {
                    self.emit_schema(
                        key,
                        MixedSchema {
                            id,
                            type_params: vec![],
                            kind: SchemaKind::Primitive {
                                primitive_type: PrimitiveType::Bytes,
                            },
                        },
                    );
                    return Ok(TypeRef::concrete(id));
                }
                let elem_ref = self.type_ref_for_shape(list_def.t(), &param_map)?;
                let args = self.extract_type_args(shape)?;
                self.emit_schema(
                    key,
                    MixedSchema {
                        id,
                        type_params: type_param_names,
                        kind: SchemaKind::List { element: elem_ref },
                    },
                );
                return Ok(if args.is_empty() {
                    TypeRef::concrete(id)
                } else {
                    TypeRef::generic(id, args)
                });
            }
            Def::Array(array_def) => {
                let elem_ref = self.type_ref_for_shape(array_def.t(), &param_map)?;
                let args = self.extract_type_args(shape)?;
                self.emit_schema(
                    key,
                    MixedSchema {
                        id,
                        type_params: type_param_names,
                        kind: SchemaKind::Array {
                            element: elem_ref,
                            length: array_def.n as u64,
                        },
                    },
                );
                return Ok(if args.is_empty() {
                    TypeRef::concrete(id)
                } else {
                    TypeRef::generic(id, args)
                });
            }
            Def::Slice(slice_def) => {
                if let Some(ScalarType::U8) = slice_def.t().scalar_type() {
                    self.emit_schema(
                        key,
                        MixedSchema {
                            id,
                            type_params: vec![],
                            kind: SchemaKind::Primitive {
                                primitive_type: PrimitiveType::Bytes,
                            },
                        },
                    );
                    return Ok(TypeRef::concrete(id));
                }
                let elem_ref = self.type_ref_for_shape(slice_def.t(), &param_map)?;
                let args = self.extract_type_args(shape)?;
                self.emit_schema(
                    key,
                    MixedSchema {
                        id,
                        type_params: type_param_names,
                        kind: SchemaKind::List { element: elem_ref },
                    },
                );
                return Ok(if args.is_empty() {
                    TypeRef::concrete(id)
                } else {
                    TypeRef::generic(id, args)
                });
            }
            Def::Map(map_def) => {
                let key_ref = self.type_ref_for_shape(map_def.k(), &param_map)?;
                let val_ref = self.type_ref_for_shape(map_def.v(), &param_map)?;
                let args = self.extract_type_args(shape)?;
                self.emit_schema(
                    key,
                    MixedSchema {
                        id,
                        type_params: type_param_names,
                        kind: SchemaKind::Map {
                            key: key_ref,
                            value: val_ref,
                        },
                    },
                );
                return Ok(if args.is_empty() {
                    TypeRef::concrete(id)
                } else {
                    TypeRef::generic(id, args)
                });
            }
            Def::Set(set_def) => {
                let elem_ref = self.type_ref_for_shape(set_def.t(), &param_map)?;
                let args = self.extract_type_args(shape)?;
                self.emit_schema(
                    key,
                    MixedSchema {
                        id,
                        type_params: type_param_names,
                        kind: SchemaKind::List { element: elem_ref },
                    },
                );
                return Ok(if args.is_empty() {
                    TypeRef::concrete(id)
                } else {
                    TypeRef::generic(id, args)
                });
            }
            Def::Option(opt_def) => {
                let elem_ref = self.type_ref_for_shape(opt_def.t(), &param_map)?;
                let args = self.extract_type_args(shape)?;
                self.emit_schema(
                    key,
                    MixedSchema {
                        id,
                        type_params: type_param_names,
                        kind: SchemaKind::Option { element: elem_ref },
                    },
                );
                return Ok(if args.is_empty() {
                    TypeRef::concrete(id)
                } else {
                    TypeRef::generic(id, args)
                });
            }
            Def::Result(result_def) => {
                let ok_ref = self.type_ref_for_shape(result_def.t(), &param_map)?;
                let err_ref = self.type_ref_for_shape(result_def.e(), &param_map)?;
                let args = self.extract_type_args(shape)?;
                self.emit_schema(
                    key,
                    MixedSchema {
                        id,
                        type_params: type_param_names,
                        kind: SchemaKind::Enum {
                            name: shape.type_identifier.to_string(),
                            variants: vec![
                                VariantSchema {
                                    name: "Ok".to_string(),
                                    index: 0,
                                    payload: VariantPayload::Newtype { type_ref: ok_ref },
                                },
                                VariantSchema {
                                    name: "Err".to_string(),
                                    index: 1,
                                    payload: VariantPayload::Newtype { type_ref: err_ref },
                                },
                            ],
                        },
                    },
                );
                return Ok(if args.is_empty() {
                    TypeRef::concrete(id)
                } else {
                    TypeRef::generic(id, args)
                });
            }
            _ => {}
        }

        // User-defined types.
        let kind = match shape.ty {
            // r[impl schema.format.struct]
            // r[impl schema.format.tuple]
            Type::User(UserType::Struct(struct_type)) => match struct_type.kind {
                StructKind::Unit => SchemaKind::Primitive {
                    primitive_type: PrimitiveType::Unit,
                },
                StructKind::TupleStruct | StructKind::Tuple => {
                    if let Some(arity) = anonymous_tuple_arity(shape) {
                        let args = self.extract_instantiation_args(shape)?;
                        let type_params = tuple_type_params(arity);
                        let elements = type_params
                            .iter()
                            .cloned()
                            .map(|name| TypeRef::Var { name })
                            .collect();
                        self.emit_schema(
                            key,
                            MixedSchema {
                                id,
                                type_params,
                                kind: SchemaKind::Tuple { elements },
                            },
                        );
                        return Ok(TypeRef::generic(id, args));
                    }
                    let mut elements = Vec::with_capacity(struct_type.fields.len());
                    for f in struct_type.fields {
                        elements.push(self.type_ref_for_shape(f.shape(), &param_map)?);
                    }
                    SchemaKind::Tuple { elements }
                }
                StructKind::Struct => {
                    let mut fields = Vec::with_capacity(struct_type.fields.len());
                    for f in struct_type.fields {
                        fields.push(FieldSchema {
                            name: f.name.to_string(),
                            type_ref: self.type_ref_for_shape(f.shape(), &param_map)?,
                            required: f.default.is_none(),
                        });
                    }
                    SchemaKind::Struct {
                        name: shape.type_identifier.to_string(),
                        fields,
                    }
                }
            },
            // r[impl schema.format.enum]
            Type::User(UserType::Enum(enum_type)) => {
                let mut variants = Vec::with_capacity(enum_type.variants.len());
                for (i, v) in enum_type.variants.iter().enumerate() {
                    let payload = match v.data.kind {
                        StructKind::Unit => VariantPayload::Unit,
                        StructKind::TupleStruct | StructKind::Tuple => {
                            if v.data.fields.len() == 1 {
                                VariantPayload::Newtype {
                                    type_ref: self
                                        .type_ref_for_shape(v.data.fields[0].shape(), &param_map)?,
                                }
                            } else {
                                let mut types = Vec::with_capacity(v.data.fields.len());
                                for f in v.data.fields {
                                    types.push(self.type_ref_for_shape(f.shape(), &param_map)?);
                                }
                                VariantPayload::Tuple { types }
                            }
                        }
                        StructKind::Struct => {
                            let mut fields = Vec::with_capacity(v.data.fields.len());
                            for f in v.data.fields {
                                fields.push(FieldSchema {
                                    name: f.name.to_string(),
                                    type_ref: self.type_ref_for_shape(f.shape(), &param_map)?,
                                    required: true,
                                });
                            }
                            VariantPayload::Struct { fields }
                        }
                    };
                    variants.push(VariantSchema {
                        name: v.name.to_string(),
                        index: i as u32,
                        payload,
                    });
                }
                SchemaKind::Enum {
                    name: shape.type_identifier.to_string(),
                    variants,
                }
            }
            Type::User(UserType::Opaque) => SchemaKind::Primitive {
                primitive_type: PrimitiveType::Payload,
            },
            other => {
                return Err(SchemaExtractError::UnhandledType {
                    type_desc: format!("{other:?} for shape {shape} (def={:?})", shape.def),
                });
            }
        };

        let args = self.extract_type_args(shape)?;
        self.emit_schema(
            key,
            MixedSchema {
                id,
                type_params: type_param_names,
                kind,
            },
        );

        Ok(if args.is_empty() {
            TypeRef::concrete(id)
        } else {
            TypeRef::generic(id, args)
        })
    }

    /// Extract the concrete type arguments for a generic shape.
    /// For `Vec<u32>`, this extracts u32 and returns `[TypeRef::concrete(u32_id)]`.
    /// For non-generic types, returns an empty vec.
    fn extract_type_args(
        &mut self,
        shape: &'static Shape,
    ) -> Result<Vec<TypeRef<MixedId>>, SchemaExtractError> {
        if shape.type_params.is_empty() {
            return Ok(vec![]);
        }
        let mut args = Vec::with_capacity(shape.type_params.len());
        for tp in shape.type_params {
            args.push(self.extract(tp.shape)?);
        }
        Ok(args)
    }

    /// Extract the concrete instantiation arguments for a shape.
    ///
    /// Most generic shapes get their args from facet `type_params`.
    /// Anonymous tuples are synthesized as generic families per arity,
    /// so their "type args" come from their element shapes.
    fn extract_instantiation_args(
        &mut self,
        shape: &'static Shape,
    ) -> Result<Vec<TypeRef<MixedId>>, SchemaExtractError> {
        if anonymous_tuple_arity(shape).is_some()
            && let Type::User(UserType::Struct(struct_type)) = shape.ty
        {
            let mut args = Vec::with_capacity(struct_type.fields.len());
            for field in struct_type.fields {
                args.push(self.extract(field.shape())?);
            }
            return Ok(args);
        }
        self.extract_type_args(shape)
    }
}

fn anonymous_tuple_arity(shape: &'static Shape) -> Option<usize> {
    match shape.ty {
        Type::User(UserType::Struct(struct_type))
            if struct_type.kind == StructKind::Tuple && shape.type_identifier.starts_with('(') =>
        {
            Some(struct_type.fields.len())
        }
        _ => None,
    }
}

fn tuple_type_params(arity: usize) -> Vec<TypeParamName> {
    (0..arity)
        .map(|index| TypeParamName(format!("T{index}")))
        .collect()
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

#[cfg(test)]
mod tests {
    use super::*;
    use facet::Facet;

    struct TestSchematic {
        direction: BindingDirection,
        shape: &'static Shape,
        attached: CborPayload,
    }

    impl TestSchematic {
        fn new(direction: BindingDirection, shape: &'static Shape) -> Self {
            Self {
                direction,
                shape,
                attached: CborPayload::default(),
            }
        }
    }

    impl Schematic for TestSchematic {
        fn direction(&self) -> BindingDirection {
            self.direction
        }

        fn attach_schemas(&mut self, schemas: CborPayload) {
            self.attached = schemas;
        }
    }

    // r[verify schema.type-id]
    #[test]
    fn type_ids_are_u64_content_hashes() {
        let id = SchemaHash(42);
        assert_eq!(id.0, 42);
        assert_eq!(id, SchemaHash(42));
        assert_ne!(id, SchemaHash(43));
    }

    // r[verify schema.principles.cbor]
    // r[verify schema.format.self-contained]
    #[test]
    fn cbor_round_trip() {
        let schema = Schema {
            id: SchemaHash(1),
            type_params: vec![],
            kind: SchemaKind::Primitive {
                primitive_type: PrimitiveType::U32,
            },
        };
        let bytes = SchemaPayload {
            schemas: vec![schema.clone()],
            root: TypeRef::concrete(schema.id),
        }
        .to_cbor();
        let payload = SchemaPayload::from_cbor(&bytes.0).expect("should parse CBOR");
        assert_eq!(payload.schemas.len(), 1);
        assert_eq!(payload.schemas[0].id, schema.id);
        assert_eq!(payload.root, TypeRef::concrete(schema.id));
    }

    // r[verify schema.format.primitive]
    #[test]
    fn primitive_u32() {
        let schemas = extract_schemas(<u32 as Facet>::SHAPE).unwrap().schemas;
        assert_eq!(schemas.len(), 1);
        assert!(matches!(
            schemas[0].kind,
            SchemaKind::Primitive {
                primitive_type: PrimitiveType::U32
            }
        ));
    }

    #[test]
    fn primitive_string() {
        let schemas = extract_schemas(<String as Facet>::SHAPE).unwrap().schemas;
        assert_eq!(schemas.len(), 1);
        assert!(matches!(
            schemas[0].kind,
            SchemaKind::Primitive {
                primitive_type: PrimitiveType::String
            }
        ));
    }

    #[test]
    fn primitive_bool() {
        let schemas = extract_schemas(<bool as Facet>::SHAPE).unwrap().schemas;
        assert_eq!(schemas.len(), 1);
        assert!(matches!(
            schemas[0].kind,
            SchemaKind::Primitive {
                primitive_type: PrimitiveType::Bool
            }
        ));
    }

    // r[verify schema.format.struct]
    #[test]
    fn simple_struct() {
        #[derive(Facet)]
        struct Point {
            x: f64,
            y: f64,
        }

        let schemas = extract_schemas(Point::SHAPE).unwrap().schemas;
        assert!(schemas.len() >= 2);

        let point_schema = schemas.last().unwrap();
        match &point_schema.kind {
            SchemaKind::Struct { name, fields } => {
                assert!(
                    name.contains("Point"),
                    "expected name to contain Point, got {name}"
                );
                assert_eq!(fields.len(), 2);
                assert_eq!(fields[0].name, "x");
                assert_eq!(fields[1].name, "y");
                assert!(fields[0].required);
                assert_eq!(fields[0].type_ref, fields[1].type_ref);
            }
            other => panic!("expected Struct, got {other:?}"),
        }
    }

    // r[verify schema.format.enum]
    #[test]
    fn simple_enum() {
        #[derive(Facet)]
        #[repr(u8)]
        enum Color {
            Red,
            Green,
            Blue,
        }

        let schemas = extract_schemas(Color::SHAPE).unwrap().schemas;
        let color_schema = schemas.last().unwrap();
        match &color_schema.kind {
            SchemaKind::Enum { variants, .. } => {
                assert_eq!(variants.len(), 3);
                assert_eq!(variants[0].name, "Red");
                assert_eq!(variants[1].name, "Green");
                assert_eq!(variants[2].name, "Blue");
                assert!(matches!(variants[0].payload, VariantPayload::Unit));
            }
            other => panic!("expected Enum, got {other:?}"),
        }
    }

    // r[verify schema.format.enum]
    #[test]
    fn enum_with_payloads() {
        #[derive(Facet)]
        #[repr(u8)]
        #[allow(dead_code)]
        enum Shape {
            Circle(f64),
            Rect { w: f64, h: f64 },
            Empty,
        }

        let schemas = extract_schemas(Shape::SHAPE).unwrap().schemas;
        let shape_schema = schemas.last().unwrap();
        match &shape_schema.kind {
            SchemaKind::Enum { variants, .. } => {
                assert_eq!(variants.len(), 3);
                assert!(matches!(
                    variants[0].payload,
                    VariantPayload::Newtype { .. }
                ));
                match &variants[1].payload {
                    VariantPayload::Struct { fields } => {
                        assert_eq!(fields.len(), 2);
                        assert_eq!(fields[0].name, "w");
                        assert_eq!(fields[1].name, "h");
                    }
                    other => panic!("expected Struct variant, got {other:?}"),
                }
                assert!(matches!(variants[2].payload, VariantPayload::Unit));
            }
            other => panic!("expected Enum, got {other:?}"),
        }
    }

    // r[verify schema.format.container]
    #[test]
    fn container_vec() {
        let schemas = extract_schemas(<Vec<u32> as Facet>::SHAPE).unwrap().schemas;
        assert_eq!(schemas.len(), 2);
        assert!(matches!(
            schemas[0].kind,
            SchemaKind::Primitive {
                primitive_type: PrimitiveType::U32
            }
        ));
        assert!(matches!(schemas[1].kind, SchemaKind::List { .. }));
    }

    // r[verify schema.format.container]
    #[test]
    fn container_option() {
        let schemas = extract_schemas(<Option<String> as Facet>::SHAPE)
            .unwrap()
            .schemas;
        assert_eq!(schemas.len(), 2);
        assert!(matches!(
            schemas[0].kind,
            SchemaKind::Primitive {
                primitive_type: PrimitiveType::String
            }
        ));
        assert!(matches!(schemas[1].kind, SchemaKind::Option { .. }));
    }

    // r[verify schema.format.recursive]
    #[test]
    fn recursive_type_terminates() {
        #[derive(Facet)]
        struct Node {
            value: u32,
            next: Option<Box<Node>>,
        }

        let schemas = extract_schemas(Node::SHAPE).unwrap().schemas;
        assert!(schemas.len() >= 2);

        let node_schema = schemas.last().unwrap();
        assert!(matches!(node_schema.kind, SchemaKind::Struct { .. }));
    }

    // r[verify schema.format.primitive]
    #[test]
    fn vec_u8_is_bytes() {
        let schemas = extract_schemas(<Vec<u8> as Facet>::SHAPE).unwrap().schemas;
        assert_eq!(schemas.len(), 1);
        assert!(matches!(
            schemas[0].kind,
            SchemaKind::Primitive {
                primitive_type: PrimitiveType::Bytes
            }
        ));
    }

    #[test]
    fn slice_u8_is_bytes() {
        let schemas = extract_schemas(<&[u8] as Facet>::SHAPE).unwrap().schemas;
        assert_eq!(schemas.len(), 1);
        assert!(matches!(
            schemas[0].kind,
            SchemaKind::Primitive {
                primitive_type: PrimitiveType::Bytes
            }
        ));
    }

    #[test]
    fn cbor_payload_is_bytes() {
        let schemas = extract_schemas(CborPayload::SHAPE).unwrap().schemas;
        assert_eq!(schemas.len(), 1);
        assert!(matches!(
            schemas[0].kind,
            SchemaKind::Primitive {
                primitive_type: PrimitiveType::Bytes
            }
        ));
    }

    // r[verify zerocopy.framing.value.opaque]
    #[test]
    fn opaque_payload_is_payload_primitive() {
        let schemas = extract_schemas(crate::Payload::<'static>::SHAPE)
            .unwrap()
            .schemas;
        assert_eq!(schemas.len(), 1);
        assert!(matches!(
            schemas[0].kind,
            SchemaKind::Primitive {
                primitive_type: PrimitiveType::Payload
            }
        ));
    }

    // r[verify schema.principles.once-per-type]
    #[test]
    fn deduplication_two_u32_fields() {
        #[derive(Facet)]
        struct TwoU32 {
            a: u32,
            b: u32,
        }

        let schemas = extract_schemas(TwoU32::SHAPE).unwrap().schemas;
        let u32_count = schemas
            .iter()
            .filter(|s| {
                matches!(
                    s.kind,
                    SchemaKind::Primitive {
                        primitive_type: PrimitiveType::U32
                    }
                )
            })
            .count();
        assert_eq!(u32_count, 1, "u32 schema should appear exactly once");
        assert_eq!(schemas.len(), 2);
    }

    // r[verify schema.format.container]
    #[test]
    fn container_map() {
        let schemas = extract_schemas(<std::collections::HashMap<String, u32> as Facet>::SHAPE)
            .unwrap()
            .schemas;
        let map_schema = schemas.last().unwrap();
        assert!(matches!(map_schema.kind, SchemaKind::Map { .. }));
    }

    // r[verify schema.format.container]
    #[test]
    fn container_array() {
        let schemas = extract_schemas(<[u32; 4] as Facet>::SHAPE).unwrap().schemas;
        let arr_schema = schemas.last().unwrap();
        match &arr_schema.kind {
            SchemaKind::Array { length, .. } => assert_eq!(*length, 4),
            other => panic!("expected Array, got {other:?}"),
        }
    }

    // r[verify schema.format.tuple]
    #[test]
    fn tuple_type() {
        let schemas = extract_schemas(<(u32, String) as Facet>::SHAPE)
            .unwrap()
            .schemas;
        let tuple_schema = schemas.last().unwrap();
        match &tuple_schema.kind {
            SchemaKind::Tuple { elements } => {
                assert_eq!(elements.len(), 2);
                assert_ne!(elements[0], elements[1]);
            }
            other => panic!("expected Tuple, got {other:?}"),
        }
    }

    // r[verify schema.format]
    #[test]
    fn extract_schemas_returns_all_kinds() {
        #[derive(Facet)]
        struct Mixed {
            count: u32,
            tags: Vec<String>,
            pair: (u8, u8),
        }

        let schemas = extract_schemas(Mixed::SHAPE).unwrap().schemas;
        assert!(schemas.len() >= 4);
    }

    // r[verify schema.principles.once-per-type]
    // r[verify schema.exchange.idempotent]
    #[test]
    fn tracker_prepare_send_returns_payload_then_empty() {
        let mut tracker = SchemaSendTracker::new();
        let method = MethodId(1);
        let mut schematic = TestSchematic::new(BindingDirection::Args, <u32 as Facet>::SHAPE);
        let first = tracker
            .attach_schemas_for_shape_if_needed(method, schematic.shape, &mut schematic)
            .unwrap();
        assert!(
            !first.is_empty(),
            "first prepare_send should return payload"
        );
        assert_eq!(schematic.attached.0, first.0);
        let second = tracker
            .attach_schemas_for_shape_if_needed(method, schematic.shape, &mut schematic)
            .unwrap();
        assert!(
            second.is_empty(),
            "second prepare_send for same method should return empty"
        );
        assert!(schematic.attached.is_empty());
    }

    // r[verify schema.tracking.transitive]
    // r[verify schema.tracking.sent]
    #[test]
    fn tracker_prepare_send_includes_transitive_deps() {
        #[derive(Facet)]
        struct Outer {
            inner: u32,
            name: String,
        }

        let mut tracker = SchemaSendTracker::new();
        let method = MethodId(1);
        let mut schematic = TestSchematic::new(BindingDirection::Args, Outer::SHAPE);
        let first = tracker
            .attach_schemas_for_shape_if_needed(method, schematic.shape, &mut schematic)
            .unwrap();
        assert!(!first.is_empty(), "should return schemas");
        let parsed = SchemaPayload::from_cbor(&first.0).expect("should parse CBOR");
        assert!(
            parsed.schemas.len() >= 3,
            "should include transitive deps, got {}",
            parsed.schemas.len()
        );

        // Same method again — nothing to send
        schematic.shape = <u32 as Facet>::SHAPE;
        let again = tracker
            .attach_schemas_for_shape_if_needed(method, schematic.shape, &mut schematic)
            .unwrap();
        assert!(
            again.is_empty(),
            "u32 was already sent as transitive dep, method already bound"
        );
    }

    // r[verify schema.tracking.received]
    #[test]
    fn tracker_record_and_get_received() {
        let tracker = SchemaRecvTracker::new();
        let schemas = extract_schemas(<u32 as Facet>::SHAPE).unwrap().schemas;
        let id = schemas[0].id;
        assert!(tracker.get_received(&id).is_none());
        tracker
            .record_received(
                MethodId(7),
                BindingDirection::Args,
                SchemaPayload {
                    schemas,
                    root: TypeRef::concrete(id),
                },
            )
            .expect("first record should succeed");
        assert!(tracker.get_received(&id).is_some());
        assert_eq!(
            tracker.get_remote_args_root(MethodId(7)),
            Some(TypeRef::concrete(id))
        );
    }

    // r[verify schema.type-id]
    // r[verify schema.type-id.hash]
    #[test]
    fn type_ids_are_content_hashes() {
        let mut tracker = SchemaSendTracker::new();
        let schemas = tracker
            .extract_schemas(<(u32, String) as Facet>::SHAPE)
            .unwrap()
            .schemas;
        assert!(schemas.len() >= 3);

        // Same type extracted again must produce the same content hash.
        let mut tracker2 = SchemaSendTracker::new();
        let schemas2 = tracker2
            .extract_schemas(<(u32, String) as Facet>::SHAPE)
            .unwrap()
            .schemas;
        assert_eq!(schemas.len(), schemas2.len());
        for (a, b) in schemas.iter().zip(schemas2.iter()) {
            assert_eq!(a.id, b.id, "content hash should be deterministic");
        }

        // Different types must produce different hashes.
        let mut tracker3 = SchemaSendTracker::new();
        let schemas3 = tracker3
            .extract_schemas(<(u64, String) as Facet>::SHAPE)
            .unwrap()
            .schemas;
        let root_hash = schemas.last().unwrap().id;
        let root_hash3 = schemas3.last().unwrap().id;
        assert_ne!(
            root_hash, root_hash3,
            "different types should have different hashes"
        );
    }

    // r[verify schema.type-id.hash.primitives]
    #[test]
    fn primitive_content_hashes_are_stable() {
        // These are the canonical hash values for primitive types.
        // Other implementations MUST produce identical values.
        let primitives = [
            PrimitiveType::Bool,
            PrimitiveType::U8,
            PrimitiveType::U16,
            PrimitiveType::U32,
            PrimitiveType::U64,
            PrimitiveType::U128,
            PrimitiveType::I8,
            PrimitiveType::I16,
            PrimitiveType::I32,
            PrimitiveType::I64,
            PrimitiveType::I128,
            PrimitiveType::F32,
            PrimitiveType::F64,
            PrimitiveType::Char,
            PrimitiveType::String,
            PrimitiveType::Unit,
            PrimitiveType::Bytes,
            PrimitiveType::Payload,
        ];

        // All primitive hashes must be unique.
        let hashes: Vec<SchemaHash> = primitives
            .iter()
            .map(|p| {
                compute_content_hash(&SchemaKind::Primitive { primitive_type: *p }, &[], &|id| id)
            })
            .collect();
        let unique: HashSet<SchemaHash> = hashes.iter().copied().collect();
        assert_eq!(
            unique.len(),
            hashes.len(),
            "all primitive hashes must be unique"
        );

        // Verify they're deterministic (same computation, same result).
        for (i, p) in primitives.iter().enumerate() {
            let hash2 =
                compute_content_hash(&SchemaKind::Primitive { primitive_type: *p }, &[], &|id| id);
            assert_eq!(hashes[i], hash2, "hash for {:?} must be deterministic", p);
        }
    }

    // r[verify schema.type-id.hash.struct]
    #[test]
    fn struct_hash_is_deterministic() {
        #[derive(Facet)]
        struct Point {
            x: f64,
            y: f64,
        }

        let schemas1 = extract_schemas(Point::SHAPE).unwrap().schemas;
        let schemas2 = extract_schemas(Point::SHAPE).unwrap().schemas;
        assert_eq!(
            schemas1.last().unwrap().id,
            schemas2.last().unwrap().id,
            "same struct must produce the same content hash"
        );
    }

    // r[verify schema.hash.recursive]
    #[test]
    fn recursive_type_hash_is_deterministic() {
        #[derive(Facet)]
        struct TreeNode {
            label: String,
            children: Vec<TreeNode>,
        }

        let schemas1 = extract_schemas(TreeNode::SHAPE).unwrap().schemas;
        let schemas2 = extract_schemas(TreeNode::SHAPE).unwrap().schemas;

        // Must have at least String, Vec<TreeNode>, TreeNode
        assert!(schemas1.len() >= 2);

        // Same recursive type must produce identical hashes.
        let root1 = schemas1.last().unwrap().id;
        let root2 = schemas2.last().unwrap().id;
        assert_eq!(root1, root2, "recursive type hash must be deterministic");

        // All type IDs in the output must be valid content hashes (non-zero).
        for s in &schemas1 {
            assert_ne!(s.id.0, 0, "content hash must not be zero");
        }
    }

    #[test]
    fn bidirectional_bindings_are_independent() {
        let mut tracker = SchemaSendTracker::new();
        let method = MethodId(1);

        // Send args binding
        let mut args_schematic = TestSchematic::new(BindingDirection::Args, <u32 as Facet>::SHAPE);
        let args = tracker
            .attach_schemas_for_shape_if_needed(method, args_schematic.shape, &mut args_schematic)
            .unwrap();
        assert!(!args.is_empty(), "should send args");
        let args_parsed = SchemaPayload::from_cbor(&args.0).expect("parse args CBOR");

        // Send response binding for the same method — should NOT be deduplicated
        let mut response_schematic =
            TestSchematic::new(BindingDirection::Response, <String as Facet>::SHAPE);
        let response = tracker
            .attach_schemas_for_shape_if_needed(
                method,
                response_schematic.shape,
                &mut response_schematic,
            )
            .unwrap();
        assert!(!response.is_empty(), "should send response");
        let response_parsed = SchemaPayload::from_cbor(&response.0).expect("parse response CBOR");
        assert_ne!(args_parsed.root, response_parsed.root);

        // Record received bindings and verify they go to separate maps
        let recv_tracker = SchemaRecvTracker::new();
        recv_tracker
            .record_received(
                MethodId(42),
                BindingDirection::Args,
                SchemaPayload {
                    schemas: extract_schemas(<u64 as Facet>::SHAPE).unwrap().schemas,
                    root: TypeRef::concrete(SchemaHash(100)),
                },
            )
            .expect("record should succeed");
        recv_tracker
            .record_received(
                MethodId(42),
                BindingDirection::Response,
                SchemaPayload {
                    schemas: vec![],
                    root: TypeRef::concrete(SchemaHash(200)),
                },
            )
            .expect("record should succeed");

        assert_eq!(
            recv_tracker.get_remote_args_root(MethodId(42)),
            Some(TypeRef::concrete(SchemaHash(100)))
        );
        assert_eq!(
            recv_tracker.get_remote_response_root(MethodId(42)),
            Some(TypeRef::concrete(SchemaHash(200)))
        );
    }

    #[test]
    fn duplicate_schema_is_protocol_error() {
        let tracker = SchemaRecvTracker::new();
        let schemas = extract_schemas(<u32 as Facet>::SHAPE).unwrap().schemas;
        tracker
            .record_received(
                MethodId(9),
                BindingDirection::Args,
                SchemaPayload {
                    schemas: schemas.clone(),
                    root: TypeRef::concrete(schemas[0].id),
                },
            )
            .expect("first record should succeed");
        let err = tracker
            .record_received(
                MethodId(9),
                BindingDirection::Args,
                SchemaPayload {
                    schemas: schemas.clone(),
                    root: TypeRef::concrete(schemas[0].id),
                },
            )
            .expect_err("duplicate should fail");
        assert_eq!(err.type_id, schemas[0].id);
    }

    #[test]
    fn send_tracker_reset_clears_all_state() {
        let mut tracker = SchemaSendTracker::new();
        let method = MethodId(1);
        let mut schematic = TestSchematic::new(BindingDirection::Args, <u32 as Facet>::SHAPE);
        let first = tracker
            .attach_schemas_for_shape_if_needed(method, schematic.shape, &mut schematic)
            .unwrap();
        assert!(!first.is_empty(), "first should return payload");

        tracker.reset();

        let after_reset = tracker
            .attach_schemas_for_shape_if_needed(method, schematic.shape, &mut schematic)
            .unwrap();
        assert!(
            !after_reset.is_empty(),
            "after reset, prepare_send should return payload again"
        );
    }

    // ========================================================================
    // Generic type deduplication tests
    // ========================================================================

    #[test]
    fn generic_vec_uses_var_in_body() {
        let schemas = extract_schemas(<Vec<u32> as Facet>::SHAPE).unwrap().schemas;
        let list_schema = schemas
            .iter()
            .find(|s| matches!(s.kind, SchemaKind::List { .. }))
            .unwrap();
        assert_eq!(
            list_schema.type_params.len(),
            1,
            "Vec should have 1 type param"
        );
        match &list_schema.kind {
            SchemaKind::List { element } => {
                assert!(
                    matches!(element, TypeRef::Var { .. }),
                    "element should be Var, got {element:?}"
                );
            }
            other => panic!("expected List, got {other:?}"),
        }
    }

    #[test]
    fn generic_option_uses_var_in_body() {
        let schemas = extract_schemas(<Option<String> as Facet>::SHAPE)
            .unwrap()
            .schemas;
        let opt_schema = schemas
            .iter()
            .find(|s| matches!(s.kind, SchemaKind::Option { .. }))
            .unwrap();
        assert_eq!(
            opt_schema.type_params.len(),
            1,
            "Option should have 1 type param"
        );
        match &opt_schema.kind {
            SchemaKind::Option { element } => {
                assert!(
                    matches!(element, TypeRef::Var { .. }),
                    "element should be Var, got {element:?}"
                );
            }
            other => panic!("expected Option, got {other:?}"),
        }
    }

    #[test]
    fn generic_tuple_uses_vars_in_body() {
        let schemas = extract_schemas(<(u32, String) as Facet>::SHAPE)
            .unwrap()
            .schemas;
        let tuple_schema = schemas
            .iter()
            .find(|s| matches!(s.kind, SchemaKind::Tuple { .. }))
            .unwrap();
        assert_eq!(
            tuple_schema.type_params.len(),
            2,
            "tuple arity 2 should have 2 type params"
        );
        match &tuple_schema.kind {
            SchemaKind::Tuple { elements } => {
                assert_eq!(elements.len(), 2);
                assert!(matches!(elements[0], TypeRef::Var { .. }));
                assert!(matches!(elements[1], TypeRef::Var { .. }));
            }
            other => panic!("expected Tuple, got {other:?}"),
        }
    }

    #[test]
    fn vec_of_option_of_u32_deduplicates() {
        // Vec<Option<u32>> should produce: u32, Option<T>, Vec<T>
        // NOT: u32, Option<u32>, Vec<Option<u32>>
        let schemas = extract_schemas(<Vec<Option<u32>> as Facet>::SHAPE)
            .unwrap()
            .schemas;

        let list_count = schemas
            .iter()
            .filter(|s| matches!(s.kind, SchemaKind::List { .. }))
            .count();
        let option_count = schemas
            .iter()
            .filter(|s| matches!(s.kind, SchemaKind::Option { .. }))
            .count();
        assert_eq!(list_count, 1, "should have exactly 1 List schema");
        assert_eq!(option_count, 1, "should have exactly 1 Option schema");
    }

    #[test]
    fn vec_u32_and_vec_string_share_one_list_schema() {
        #[derive(Facet)]
        struct Both {
            a: Vec<u32>,
            b: Vec<String>,
        }

        let schemas = extract_schemas(Both::SHAPE).unwrap().schemas;
        let list_count = schemas
            .iter()
            .filter(|s| matches!(s.kind, SchemaKind::List { .. }))
            .count();
        assert_eq!(
            list_count, 1,
            "Vec<u32> and Vec<String> should share one List schema"
        );
    }

    #[test]
    fn resolve_kind_substitutes_vars() {
        let schemas = extract_schemas(<Vec<u32> as Facet>::SHAPE).unwrap().schemas;
        let registry = build_registry(&schemas);

        // The root schema is Vec<u32> — find it
        let root = schemas.last().unwrap();
        assert!(matches!(root.kind, SchemaKind::List { .. }));

        // Build a TypeRef that says "Vec applied to u32"
        let u32_schema = schemas
            .iter()
            .find(|s| {
                matches!(
                    s.kind,
                    SchemaKind::Primitive {
                        primitive_type: PrimitiveType::U32
                    }
                )
            })
            .unwrap();
        let type_ref = TypeRef::generic(root.id, vec![TypeRef::concrete(u32_schema.id)]);

        // resolve_kind should substitute Var("T") → concrete u32 id
        let resolved = type_ref.resolve_kind(&registry).expect("should resolve");
        match &resolved {
            SchemaKind::List { element } => match element {
                TypeRef::Concrete { type_id, args } => {
                    assert_eq!(*type_id, u32_schema.id);
                    assert!(args.is_empty());
                }
                other => panic!("expected concrete after resolution, got {other:?}"),
            },
            other => panic!("expected List, got {other:?}"),
        }
    }

    #[test]
    fn extract_result_tuple_root_preserves_ok_tuple() {
        use crate::RoamError;

        let extracted = extract_schemas(
            <Result<(String, i32), RoamError<::core::convert::Infallible>> as Facet>::SHAPE,
        )
        .unwrap();
        let registry = build_registry(&extracted.schemas);
        let root = extracted
            .root
            .resolve_kind(&registry)
            .expect("result root should resolve");

        let SchemaKind::Enum { variants, .. } = root else {
            panic!("expected Result enum root");
        };
        let ok_variant = variants
            .iter()
            .find(|variant| variant.name == "Ok")
            .expect("Result should have Ok variant");
        let VariantPayload::Newtype { type_ref } = &ok_variant.payload else {
            panic!("Ok variant should be newtype");
        };
        let ok_kind = type_ref
            .resolve_kind(&registry)
            .expect("Ok payload should resolve");
        match ok_kind {
            SchemaKind::Tuple { elements } => {
                assert_eq!(elements.len(), 2, "Ok tuple should have two elements");
            }
            other => panic!("expected Ok payload to be tuple, got {other:?}"),
        }
    }

    #[test]
    fn result_ok_tuple_uses_generic_tuple_schema() {
        use crate::RoamError;

        let result_shape =
            <Result<(String, i32), RoamError<::core::convert::Infallible>> as Facet>::SHAPE;
        let ok_shape = result_shape.type_params[0].shape;
        let extracted = extract_schemas(
            <Result<(String, i32), RoamError<::core::convert::Infallible>> as Facet>::SHAPE,
        )
        .unwrap();
        let TypeRef::Concrete { args, .. } = &extracted.root else {
            panic!("Result root should be concrete");
        };
        assert_eq!(
            args.len(),
            2,
            "Result root should have Ok and Err type args"
        );
        let TypeRef::Concrete { args: ok_args, .. } = &args[0] else {
            panic!("Ok type arg should be concrete tuple ref");
        };
        assert_eq!(
            ok_args.len(),
            2,
            "Ok tuple ref should carry concrete tuple element args; root={:?}; ok_shape={}; ok_shape_ty={:?}",
            extracted.root,
            ok_shape.type_identifier,
            ok_shape.ty
        );
    }

    #[test]
    fn unary_tuple_root_preserves_nested_tuple() {
        let extracted = extract_schemas(<((i32, String),) as Facet>::SHAPE).unwrap();
        let registry = build_registry(&extracted.schemas);

        let root = extracted
            .root
            .resolve_kind(&registry)
            .expect("root should resolve");
        let SchemaKind::Tuple { elements } = root else {
            panic!("expected unary tuple root");
        };
        assert_eq!(elements.len(), 1, "outer tuple should remain unary");

        let inner = elements[0]
            .resolve_kind(&registry)
            .expect("inner tuple should resolve");
        match inner {
            SchemaKind::Tuple { elements } => {
                assert_eq!(elements.len(), 2, "inner tuple should remain binary");
            }
            other => panic!("expected inner tuple, got {other:?}"),
        }

        let tuple_count = extracted
            .schemas
            .iter()
            .filter(|schema| matches!(schema.kind, SchemaKind::Tuple { .. }))
            .count();
        assert_eq!(tuple_count, 2, "should emit one tuple schema per arity");
    }

    #[test]
    fn nested_generic_vec_of_vec_of_u32() {
        // Vec<Vec<u32>> — should produce u32, Vec<T>, not u32, Vec<u32>, Vec<Vec<u32>>
        let schemas = extract_schemas(<Vec<Vec<u32>> as Facet>::SHAPE)
            .unwrap()
            .schemas;
        let list_count = schemas
            .iter()
            .filter(|s| matches!(s.kind, SchemaKind::List { .. }))
            .count();
        assert_eq!(
            list_count, 1,
            "Vec<Vec<u32>> should have exactly 1 List schema (Vec<T>)"
        );
    }

    #[test]
    fn recursive_type_with_option_box() {
        #[derive(Facet)]
        struct Node {
            value: u32,
            next: Option<Box<Node>>,
        }

        let schemas = extract_schemas(Node::SHAPE).unwrap().schemas;
        // Should have: u32, Option<T>, Node
        let option_count = schemas
            .iter()
            .filter(|s| matches!(s.kind, SchemaKind::Option { .. }))
            .count();
        assert_eq!(option_count, 1, "should have exactly 1 Option schema");

        // The Option schema should use Var, not concrete
        let opt_schema = schemas
            .iter()
            .find(|s| matches!(s.kind, SchemaKind::Option { .. }))
            .unwrap();
        match &opt_schema.kind {
            SchemaKind::Option { element } => {
                assert!(
                    matches!(element, TypeRef::Var { .. }),
                    "element should be Var"
                );
            }
            _ => unreachable!(),
        }

        // All type IDs should be non-zero (properly hashed)
        for s in &schemas {
            assert_ne!(s.id.0, 0, "content hash must not be zero: {:?}", s.kind);
        }
    }

    #[test]
    fn map_schema_is_generic() {
        let schemas = extract_schemas(<std::collections::HashMap<String, u32> as Facet>::SHAPE)
            .unwrap()
            .schemas;
        let map_schema = schemas
            .iter()
            .find(|s| matches!(s.kind, SchemaKind::Map { .. }))
            .unwrap();
        assert_eq!(
            map_schema.type_params.len(),
            2,
            "HashMap should have 2 type params"
        );
        match &map_schema.kind {
            SchemaKind::Map { key, value } => {
                assert!(matches!(key, TypeRef::Var { .. }), "key should be Var");
                assert!(matches!(value, TypeRef::Var { .. }), "value should be Var");
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn schema_payload_cbor_round_trip() {
        let payload = SchemaPayload {
            schemas: vec![],
            root: TypeRef::Concrete {
                type_id: SchemaHash(123),
                args: vec![TypeRef::concrete(SchemaHash(456))],
            },
        };
        let bytes = payload.to_cbor();
        let parsed = SchemaPayload::from_cbor(&bytes.0).expect("should parse CBOR");
        match &parsed.root {
            TypeRef::Concrete { type_id, args } => {
                assert_eq!(*type_id, SchemaHash(123));
                assert_eq!(args.len(), 1);
                match &args[0] {
                    TypeRef::Concrete { type_id, args } => {
                        assert_eq!(*type_id, SchemaHash(456));
                        assert!(args.is_empty());
                    }
                    other => panic!("expected concrete arg, got {other:?}"),
                }
            }
            other => panic!("expected concrete root, got {other:?}"),
        }
    }
}
