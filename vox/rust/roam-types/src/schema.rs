//! Schema types, extraction, and tracking for roam wire protocol.
//!
//! This module contains:
//! - Schema data types (TypeSchemaId, Schema, SchemaKind, etc.)
//! - CBOR serialization for schema messages
//! - Schema extraction from facet Shape graphs
//! - SchemaSendTracker for outbound dedup (owned by SessionCore)
//! - SchemaRecvTracker for inbound storage (shared via Arc)

use facet::Facet;
use facet_core::{Def, ScalarType, Shape, StructKind, Type, UserType};
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use crate::{MethodId, is_rx, is_tx};

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
pub struct TypeSchemaId(pub u64);

/// A reference to a type in a schema. Either a concrete type (with optional
/// type arguments for generics) or a type variable bound by the enclosing
/// generic's `type_params`.
///
/// Generic over the ID type: `TypeSchemaId` for final schemas,
/// `MixedId` during extraction.
#[derive(Facet, Clone, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum TypeRef<Id = TypeSchemaId> {
    /// A concrete type, possibly generic.
    Concrete {
        type_id: Id,
        /// Type arguments for generic types. Empty for non-generic types.
        args: Vec<TypeRef<Id>>,
    },
    /// A reference to a type parameter of the enclosing generic type.
    /// The index refers to the `type_params` list on the `Schema`.
    Var(u32),
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
            TypeRef::Var(_) => {}
        }
    }

    /// Return the concrete type ID if this is a non-generic `Concrete` variant, panicking otherwise.
    pub fn expect_concrete_id(&self) -> &Id {
        match self {
            TypeRef::Concrete { type_id, args } if args.is_empty() => type_id,
            TypeRef::Concrete { .. } => panic!("TypeRef::expect_concrete_id: has type args"),
            TypeRef::Var(_) => panic!("TypeRef::expect_concrete_id: is a type variable"),
        }
    }

    /// Map a `TypeRef<Id>` to `TypeRef<OtherId>` by applying `f` to every concrete ID.
    pub fn map<OtherId, F: Fn(Id) -> OtherId + Copy>(self, f: F) -> TypeRef<OtherId> {
        match self {
            TypeRef::Concrete { type_id, args } => TypeRef::Concrete {
                type_id: f(type_id),
                args: args.into_iter().map(|a| a.map(f)).collect(),
            },
            TypeRef::Var(i) => TypeRef::Var(i),
        }
    }
}

/// During extraction, IDs can be either already-finalized content hashes
/// or temporary indices that will be resolved during finalization.
#[derive(Facet, Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[repr(u8)]
pub enum MixedId {
    /// A final content hash (from a previously extracted type).
    Final(TypeSchemaId),
    /// A temporary index assigned during the current extraction pass.
    Temp(u64),
}

/// The root schema type, generic over the ID representation.
#[derive(Facet, Clone, Debug)]
pub struct Schema<Id = TypeSchemaId> {
    pub type_id: Id,
    /// Type parameter names for generic types. Empty for non-generic types.
    #[facet(default)]
    pub type_params: Vec<String>,
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
pub enum SchemaKind<Id = TypeSchemaId> {
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
    Primitive {
        primitive_type: PrimitiveType,
    },
}

/// Type aliases for schemas during extraction (mixed temp/final IDs).
pub(crate) type MixedSchema = Schema<MixedId>;
pub(crate) type MixedSchemaKind = SchemaKind<MixedId>;

/// Describes a single field in a struct or struct variant.
#[derive(Facet, Clone, Debug)]
pub struct FieldSchema<Id = TypeSchemaId> {
    pub name: String,
    pub type_ref: TypeRef<Id>,
    pub required: bool,
}

/// Describes a single variant in an enum.
#[derive(Facet, Clone, Debug)]
pub struct VariantSchema<Id = TypeSchemaId> {
    pub name: String,
    pub index: u32,
    pub payload: VariantPayload<Id>,
}

/// The payload of an enum variant.
#[derive(Facet, Clone, Debug)]
#[repr(u8)]
pub enum VariantPayload<Id = TypeSchemaId> {
    Unit,
    Newtype { type_ref: TypeRef<Id> },
    Tuple { types: Vec<TypeRef<Id>> },
    Struct { fields: Vec<FieldSchema<Id>> },
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
    resolve: &'a dyn Fn(Id) -> TypeSchemaId,
}

impl<'a, Id: Copy> SchemaHasher<'a, Id> {
    fn new(resolve: &'a dyn Fn(Id) -> TypeSchemaId) -> Self {
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
                self.hasher.update(&[0x00]);
                let resolved = (self.resolve)(*type_id);
                self.hasher.update(&resolved.0.to_le_bytes());
                self.hasher.update(&(args.len() as u32).to_le_bytes());
                for arg in args {
                    self.feed_type_ref(arg);
                }
            }
            TypeRef::Var(idx) => {
                self.hasher.update(&[0x01]);
                self.hasher.update(&idx.to_le_bytes());
            }
        }
    }

    // r[impl schema.type-id.hash.primitives]
    // r[impl schema.type-id.hash.struct]
    // r[impl schema.type-id.hash.enum]
    // r[impl schema.type-id.hash.container]
    // r[impl schema.type-id.hash.tuple]
    fn feed_kind(&mut self, kind: &SchemaKind<Id>) {
        match kind {
            SchemaKind::Primitive { primitive_type } => {
                self.feed_string(primitive_type.hash_tag());
            }
            SchemaKind::Struct { fields, .. } => {
                self.feed_string("struct");
                for field in fields {
                    self.feed_string(&field.name);
                    self.feed_type_ref(&field.type_ref);
                }
            }
            SchemaKind::Enum { variants, .. } => {
                self.feed_string("enum");
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
        }
    }

    fn finalize(self) -> TypeSchemaId {
        let hash = self.hasher.finalize();
        let bytes: [u8; 8] = hash.as_bytes()[0..8].try_into().expect("slice len");
        TypeSchemaId(u64::from_le_bytes(bytes))
    }
}

/// Compute the content hash of a SchemaKind, given a resolver for child type IDs.
pub fn compute_content_hash<Id: Copy>(
    kind: &SchemaKind<Id>,
    resolve: &dyn Fn(Id) -> TypeSchemaId,
) -> TypeSchemaId {
    let mut hasher = SchemaHasher::new(resolve);
    hasher.feed_kind(kind);
    hasher.finalize()
}

/// Collect all TypeSchemaIds directly referenced by a SchemaKind.
pub fn schema_child_ids(kind: &SchemaKind) -> Vec<TypeSchemaId> {
    let mut refs = Vec::new();
    match kind {
        SchemaKind::Primitive { .. } => {}
        SchemaKind::Struct { fields, .. } => {
            for f in fields {
                f.type_ref.collect_ids(&mut refs);
            }
        }
        SchemaKind::Enum { variants, .. } => {
            for v in variants {
                match &v.payload {
                    VariantPayload::Unit => {}
                    VariantPayload::Newtype { type_ref } => type_ref.collect_ids(&mut refs),
                    VariantPayload::Tuple { types } => {
                        for t in types {
                            t.collect_ids(&mut refs);
                        }
                    }
                    VariantPayload::Struct { fields } => {
                        for f in fields {
                            f.type_ref.collect_ids(&mut refs);
                        }
                    }
                }
            }
        }
        SchemaKind::Tuple { elements } => {
            for e in elements {
                e.collect_ids(&mut refs);
            }
        }
        SchemaKind::List { element } | SchemaKind::Option { element } => {
            element.collect_ids(&mut refs);
        }
        SchemaKind::Map { key, value } => {
            key.collect_ids(&mut refs);
            value.collect_ids(&mut refs);
        }
        SchemaKind::Array { element, .. } => element.collect_ids(&mut refs),
    }
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

    /// Parse the CBOR-encoded schema message payload.
    pub fn parse(&self) -> Result<SchemaPayload, facet_cbor::CborError> {
        parse_schema_message(&self.0)
    }
}

/// Lookup table mapping TypeSchemaId → Schema, used for resolving type
/// references during deserialization with translation plans.
pub type SchemaRegistry = HashMap<TypeSchemaId, Schema>;

/// Build a SchemaRegistry from a list of schemas.
pub fn build_registry(schemas: &[Schema]) -> SchemaRegistry {
    schemas.iter().map(|s| (s.type_id, s.clone())).collect()
}

/// Binds a method to the root TypeSchemaId of the type being sent for that
/// method. Sent once per method per direction.
#[derive(Facet, Clone, Debug)]
pub struct MethodSchemaBinding {
    pub method_id: MethodId,
    pub root_type_schema_id: TypeSchemaId,
    /// Whether this binding is for args (caller → callee) or response (callee → caller).
    pub direction: BindingDirection,
}

/// Whether a method schema binding describes args or the response type.
#[derive(Facet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
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
pub struct SchemaPayload {
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
    let payload = SchemaPayload {
        schemas: schemas.to_vec(),
        method_bindings: method_bindings.to_vec(),
    };
    facet_cbor::to_vec(&payload).expect("schema CBOR serialization should not fail")
}

/// Parse a CBOR-encoded schema message.
// r[impl schema.format.batch]
// r[impl schema.principles.cbor]
pub fn parse_schema_message(bytes: &[u8]) -> Result<SchemaPayload, facet_cbor::CborError> {
    facet_cbor::from_slice(bytes)
}

// ============================================================================
// Schema extraction
// ============================================================================

/// What `prepare_send_for_method` returns when there's something to send.
pub struct PreparedSchemaMessage {
    pub schemas: Vec<Schema>,
    pub method_bindings: Vec<MethodSchemaBinding>,
}

impl PreparedSchemaMessage {
    /// CBOR-encode this prepared message for embedding in RequestCall/RequestResponse.
    pub fn to_cbor(&self) -> CborPayload {
        CborPayload(build_schema_message(&self.schemas, &self.method_bindings))
    }
}

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
    sent_methods: HashMap<(MethodId, BindingDirection), CborPayload>,
    /// Maps shapes to their MixedId during extraction.
    shape_to_id: HashMap<&'static Shape, MixedId>,
    /// All type schema IDs we've sent so far (for dedup across methods that
    /// share types).
    sent_type_ids: HashSet<TypeSchemaId>,
    /// Next ID to assign (temporary — will be replaced by content hashing).
    next_id: u64,
}

impl SchemaSendTracker {
    pub fn new() -> Self {
        SchemaSendTracker {
            sent_methods: HashMap::new(),
            shape_to_id: HashMap::new(),
            sent_type_ids: HashSet::new(),
            next_id: 1,
        }
    }

    /// Reset all state — call on reconnection.
    pub fn reset(&mut self) {
        self.sent_methods.clear();
        self.shape_to_id.clear();
        self.sent_type_ids.clear();
        self.next_id = 1;
    }

    /// Allocate or look up a MixedId for a Shape.
    fn id_for_shape(&mut self, shape: &'static Shape) -> MixedId {
        if let Some(&id) = self.shape_to_id.get(shape) {
            return id;
        }
        let id = MixedId::Temp(self.next_id);
        self.next_id += 1;
        self.shape_to_id.insert(shape, id);
        id
    }

    /// Prepare schemas for a method call/response, returning a CBOR payload
    /// to inline in the request/response. Returns empty payload if schemas
    /// were already sent for this method+direction.
    ///
    /// Fast path: if method+direction is in `sent_methods`, return immediately.
    /// Slow path: extract schemas, deduplicate, CBOR-encode, cache, return.
    // r[impl schema.tracking.transitive]
    // r[impl schema.exchange.idempotent]
    // r[impl schema.principles.once-per-type]
    // r[impl schema.principles.sender-driven]
    // r[impl schema.principles.no-roundtrips]
    pub fn prepare_send_for_method(
        &mut self,
        method_id: MethodId,
        shape: &'static Shape,
        direction: BindingDirection,
    ) -> CborPayload {
        let key = (method_id, direction);

        // Fast path: already sent for this method+direction.
        if self.sent_methods.contains_key(&key) {
            return CborPayload::default();
        }

        // Slow path: extract, deduplicate, encode.
        let all_schemas = self.extract_schemas(shape);
        let root_type_schema_id = match all_schemas.last() {
            Some(s) => s.type_id,
            None => return CborPayload::default(),
        };

        let unsent: Vec<Schema> = all_schemas
            .into_iter()
            .filter(|s| self.sent_type_ids.insert(s.type_id))
            .collect();

        let method_binding = MethodSchemaBinding {
            method_id,
            root_type_schema_id,
            direction,
        };

        let prepared = PreparedSchemaMessage {
            schemas: unsent,
            method_bindings: vec![method_binding],
        };
        let cbor = prepared.to_cbor();
        self.sent_methods.insert(key, cbor.clone());
        cbor
    }

    /// Extract all schemas for a type and its transitive dependencies.
    ///
    /// Returns schemas in dependency order: dependencies appear before dependents.
    /// The root type's schema is last.
    // r[impl schema.format]
    pub fn extract_schemas(&mut self, shape: &'static Shape) -> Vec<Schema> {
        let mut ctx = ExtractCtx {
            tracker: self,
            schemas: Vec::new(),
            seen: HashSet::new(),
            stack: Vec::new(),
        };
        ctx.extract(shape);
        let schemas = ctx.schemas;
        finalize_content_hashes(schemas)
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
    received: Mutex<HashMap<TypeSchemaId, Schema>>,
    /// Args bindings received: method_id → root TypeSchemaId for args.
    received_args_bindings: Mutex<HashMap<MethodId, TypeSchemaId>>,
    /// Response bindings received: method_id → root TypeSchemaId for response.
    received_response_bindings: Mutex<HashMap<MethodId, TypeSchemaId>>,
}

/// Error returned when recording received schemas detects a protocol violation.
#[derive(Debug)]
pub struct DuplicateSchemaError {
    pub type_id: TypeSchemaId,
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
    pub fn record_received(&self, payload: SchemaPayload) -> Result<(), DuplicateSchemaError> {
        {
            let mut received = self.received.lock().unwrap();
            for schema in payload.schemas {
                if received.contains_key(&schema.type_id) {
                    return Err(DuplicateSchemaError {
                        type_id: schema.type_id,
                    });
                }
                received.insert(schema.type_id, schema);
            }
        }
        for binding in payload.method_bindings {
            let map = match binding.direction {
                BindingDirection::Args => &self.received_args_bindings,
                BindingDirection::Response => &self.received_response_bindings,
            };
            map.lock()
                .unwrap()
                .insert(binding.method_id, binding.root_type_schema_id);
        }
        Ok(())
    }

    /// Look up the remote's root TypeSchemaId for a method's args.
    pub fn get_remote_args_root(&self, method_id: MethodId) -> Option<TypeSchemaId> {
        self.received_args_bindings
            .lock()
            .unwrap()
            .get(&method_id)
            .copied()
    }

    /// Look up the remote's root TypeSchemaId for a method's response.
    pub fn get_remote_response_root(&self, method_id: MethodId) -> Option<TypeSchemaId> {
        self.received_response_bindings
            .lock()
            .unwrap()
            .get(&method_id)
            .copied()
    }

    /// Look up a received schema by type ID.
    pub fn get_received(&self, type_id: &TypeSchemaId) -> Option<Schema> {
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

impl std::fmt::Debug for SchemaRecvTracker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SchemaRecvTracker").finish_non_exhaustive()
    }
}

/// Extract schemas without a tracker (uses a temporary counter).
/// Useful for tests and one-off schema extraction.
pub fn extract_schemas(shape: &'static Shape) -> Vec<Schema> {
    let mut tracker = SchemaSendTracker::new();
    tracker.extract_schemas(shape)
}

/// Replace temporary incrementing IDs with blake3 content hashes.
///
/// Schemas must be in dependency order (dependencies before dependents).
/// For non-recursive types, this is a simple bottom-up pass. For recursive
/// types, the 4-step algorithm from r[schema.hash.recursive] is used.
// r[impl schema.type-id.hash]
// r[impl schema.hash.recursive]
/// Resolve a MixedId to a TypeSchemaId for hashing purposes.
fn resolve_mixed(id: MixedId, temp_to_final: &HashMap<u64, TypeSchemaId>) -> TypeSchemaId {
    match id {
        MixedId::Final(tid) => tid,
        MixedId::Temp(t) => temp_to_final.get(&t).copied().unwrap_or(TypeSchemaId(0)),
    }
}

/// Convert a Vec<MixedSchema> (from extraction) into Vec<Schema> with
/// content-hashed TypeSchemaIds.
///
/// Schemas must be in dependency order (dependencies before dependents).
/// For non-recursive types, this is a simple bottom-up pass. For recursive
/// types, the 4-step algorithm from r[schema.hash.recursive] is used.
// r[impl schema.type-id.hash]
// r[impl schema.hash.recursive]
fn finalize_content_hashes(schemas: Vec<MixedSchema>) -> Vec<Schema> {
    // Only Temp entries need hashing. Build index of temp IDs.
    let temp_to_idx: HashMap<u64, usize> = schemas
        .iter()
        .enumerate()
        .filter_map(|(i, s)| match s.type_id {
            MixedId::Temp(t) => Some((t, i)),
            MixedId::Final(_) => None,
        })
        .collect();

    // Collect all MixedIds referenced by a schema kind.
    fn collect_refs(kind: &MixedSchemaKind) -> Vec<MixedId> {
        let mut refs = Vec::new();
        match kind {
            SchemaKind::Primitive { .. } => {}
            SchemaKind::Struct { fields, .. } => {
                for f in fields {
                    f.type_ref.collect_ids(&mut refs);
                }
            }
            SchemaKind::Enum { variants, .. } => {
                for v in variants {
                    match &v.payload {
                        VariantPayload::Unit => {}
                        VariantPayload::Newtype { type_ref } => type_ref.collect_ids(&mut refs),
                        VariantPayload::Tuple { types } => {
                            for t in types {
                                t.collect_ids(&mut refs);
                            }
                        }
                        VariantPayload::Struct { fields } => {
                            for f in fields {
                                f.type_ref.collect_ids(&mut refs);
                            }
                        }
                    }
                }
            }
            SchemaKind::Tuple { elements } => {
                for e in elements {
                    e.collect_ids(&mut refs);
                }
            }
            SchemaKind::List { element } | SchemaKind::Option { element } => {
                element.collect_ids(&mut refs);
            }
            SchemaKind::Map { key, value } => {
                key.collect_ids(&mut refs);
                value.collect_ids(&mut refs);
            }
            SchemaKind::Array { element, .. } => element.collect_ids(&mut refs),
        }
        refs
    }

    // Detect recursive groups among temp schemas.
    let n = schemas.len();
    let mut in_recursive_group: Vec<bool> = vec![false; n];

    for (i, schema) in schemas.iter().enumerate() {
        if matches!(schema.type_id, MixedId::Final(_)) {
            continue; // Already finalized, skip.
        }
        for r in collect_refs(&schema.kind) {
            if let MixedId::Temp(t) = r {
                if let Some(&ref_idx) = temp_to_idx.get(&t) {
                    if ref_idx >= i {
                        in_recursive_group[i] = true;
                        in_recursive_group[ref_idx] = true;
                    }
                }
            }
        }
    }

    // Map from temp ID -> final content hash.
    let mut temp_to_final: HashMap<u64, TypeSchemaId> = HashMap::new();

    // Phase 1: Hash non-recursive temp types bottom-up.
    for (i, schema) in schemas.iter().enumerate() {
        if in_recursive_group[i] {
            continue;
        }
        if let MixedId::Temp(temp) = schema.type_id {
            let final_id =
                compute_content_hash(&schema.kind, &|mid| resolve_mixed(mid, &temp_to_final));
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
        let group_temp_ids: HashSet<u64> = schemas[group_start..group_end]
            .iter()
            .filter_map(|s| match s.type_id {
                MixedId::Temp(t) => Some(t),
                _ => None,
            })
            .collect();

        // Step 1: Preliminary hashes — intra-group refs become sentinel (0).
        let mut prelim_hashes: Vec<TypeSchemaId> = Vec::new();
        for schema in &schemas[group_start..group_end] {
            let prelim = compute_content_hash(&schema.kind, &|mid| match mid {
                MixedId::Final(tid) => tid,
                MixedId::Temp(t) => {
                    if group_temp_ids.contains(&t) {
                        TypeSchemaId(0) // sentinel
                    } else {
                        temp_to_final.get(&t).copied().unwrap_or(TypeSchemaId(0))
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
                TypeSchemaId(u64::from_le_bytes(fo.as_bytes()[0..8].try_into().unwrap()));

            if let MixedId::Temp(t) = schemas[group_start + idx].type_id {
                temp_to_final.insert(t, final_hash);
            }
        }
    }

    // Phase 3: Convert MixedSchema -> Schema by resolving all MixedIds.
    fn resolve_kind(
        kind: MixedSchemaKind,
        temp_to_final: &HashMap<u64, TypeSchemaId>,
    ) -> SchemaKind {
        let r = |mid: MixedId| -> TypeSchemaId {
            match mid {
                MixedId::Final(tid) => tid,
                MixedId::Temp(t) => *temp_to_final
                    .get(&t)
                    .expect("unresolved temp ID during finalization"),
            }
        };
        let rt = |type_ref: TypeRef<MixedId>| -> TypeRef<TypeSchemaId> { type_ref.map(r) };
        match kind {
            SchemaKind::Primitive { primitive_type } => SchemaKind::Primitive { primitive_type },
            SchemaKind::Struct { name, fields } => SchemaKind::Struct {
                name,
                fields: fields
                    .into_iter()
                    .map(|f| FieldSchema {
                        name: f.name,
                        type_ref: rt(f.type_ref),
                        required: f.required,
                    })
                    .collect(),
            },
            SchemaKind::Enum { name, variants } => SchemaKind::Enum {
                name,
                variants: variants
                    .into_iter()
                    .map(|v| VariantSchema {
                        name: v.name,
                        index: v.index,
                        payload: match v.payload {
                            VariantPayload::Unit => VariantPayload::Unit,
                            VariantPayload::Newtype { type_ref } => VariantPayload::Newtype {
                                type_ref: rt(type_ref),
                            },
                            VariantPayload::Tuple { types } => VariantPayload::Tuple {
                                types: types.into_iter().map(rt).collect(),
                            },
                            VariantPayload::Struct { fields } => VariantPayload::Struct {
                                fields: fields
                                    .into_iter()
                                    .map(|f| FieldSchema {
                                        name: f.name,
                                        type_ref: rt(f.type_ref),
                                        required: f.required,
                                    })
                                    .collect(),
                            },
                        },
                    })
                    .collect(),
            },
            SchemaKind::Tuple { elements } => SchemaKind::Tuple {
                elements: elements.into_iter().map(rt).collect(),
            },
            SchemaKind::List { element } => SchemaKind::List {
                element: rt(element),
            },
            SchemaKind::Map { key, value } => SchemaKind::Map {
                key: rt(key),
                value: rt(value),
            },
            SchemaKind::Array { element, length } => SchemaKind::Array {
                element: rt(element),
                length,
            },
            SchemaKind::Option { element } => SchemaKind::Option {
                element: rt(element),
            },
        }
    }

    schemas
        .into_iter()
        .map(|s| {
            let type_id = match s.type_id {
                MixedId::Final(tid) => tid,
                MixedId::Temp(t) => *temp_to_final
                    .get(&t)
                    .expect("unresolved temp ID during finalization"),
            };
            Schema {
                type_id,
                type_params: s.type_params,
                kind: resolve_kind(s.kind, &temp_to_final),
            }
        })
        .collect()
}

struct ExtractCtx<'a> {
    tracker: &'a mut SchemaSendTracker,
    schemas: Vec<MixedSchema>,
    /// Shapes already fully processed.
    seen: HashSet<&'static Shape>,
    /// Stack for cycle detection.
    stack: Vec<&'static Shape>,
}

impl<'a> ExtractCtx<'a> {
    fn push_schema(&mut self, shape: &'static Shape, type_id: MixedId, kind: MixedSchemaKind) {
        if self.seen.insert(shape) {
            self.schemas.push(MixedSchema {
                type_id,
                type_params: vec![],
                kind,
            });
        }
    }

    /// Extract a schema for the given shape, returning its MixedId.
    /// Recursively extracts dependencies first.
    fn extract(&mut self, shape: &'static Shape) -> MixedId {
        // Channel types: extract the element type, skip the channel wrapper.
        if (is_tx(shape) || is_rx(shape))
            && let Some(inner) = shape.type_params.first()
        {
            return self.extract(inner.shape);
        }

        // Transparent wrappers: follow inner.
        if shape.is_transparent()
            && let Some(inner) = shape.inner
        {
            return self.extract(inner);
        }

        let type_id = self.tracker.id_for_shape(shape);

        // Already fully processed — just return its id.
        if self.seen.contains(shape) {
            return type_id;
        }

        // r[impl schema.format.recursive]
        // Cycle detection: if on the stack, return the id without re-entering.
        if self.stack.contains(&shape) {
            return type_id;
        }

        // r[impl schema.format.primitive]
        // Scalars
        if let Some(scalar) = shape.scalar_type() {
            self.push_schema(
                shape,
                type_id,
                SchemaKind::Primitive {
                    primitive_type: scalar_to_primitive(scalar),
                },
            );
            return type_id;
        }

        // r[impl schema.format.container]
        // Containers
        match shape.def {
            Def::List(list_def) => {
                if let Some(ScalarType::U8) = list_def.t().scalar_type() {
                    self.push_schema(
                        shape,
                        type_id,
                        SchemaKind::Primitive {
                            primitive_type: PrimitiveType::Bytes,
                        },
                    );
                } else {
                    let elem_id = self.extract(list_def.t());
                    self.push_schema(
                        shape,
                        type_id,
                        SchemaKind::List {
                            element: TypeRef::concrete(elem_id),
                        },
                    );
                }
                return type_id;
            }
            Def::Array(array_def) => {
                let elem_id = self.extract(array_def.t());
                self.push_schema(
                    shape,
                    type_id,
                    SchemaKind::Array {
                        element: TypeRef::concrete(elem_id),
                        length: array_def.n as u64,
                    },
                );
                return type_id;
            }
            Def::Slice(slice_def) => {
                let elem_id = self.extract(slice_def.t());
                self.push_schema(
                    shape,
                    type_id,
                    SchemaKind::List {
                        element: TypeRef::concrete(elem_id),
                    },
                );
                return type_id;
            }
            Def::Map(map_def) => {
                let key_id = self.extract(map_def.k());
                let val_id = self.extract(map_def.v());
                self.push_schema(
                    shape,
                    type_id,
                    SchemaKind::Map {
                        key: TypeRef::concrete(key_id),
                        value: TypeRef::concrete(val_id),
                    },
                );
                return type_id;
            }
            Def::Set(set_def) => {
                let elem_id = self.extract(set_def.t());
                self.push_schema(
                    shape,
                    type_id,
                    SchemaKind::List {
                        element: TypeRef::concrete(elem_id),
                    },
                );
                return type_id;
            }
            Def::Option(opt_def) => {
                let elem_id = self.extract(opt_def.t());
                self.push_schema(
                    shape,
                    type_id,
                    SchemaKind::Option {
                        element: TypeRef::concrete(elem_id),
                    },
                );
                return type_id;
            }
            Def::Result(result_def) => {
                let ok_id = self.extract(result_def.t());
                let err_id = self.extract(result_def.e());
                self.push_schema(
                    shape,
                    type_id,
                    SchemaKind::Enum {
                        name: format!("{shape}"),
                        variants: vec![
                            VariantSchema {
                                name: "Ok".to_string(),
                                index: 0,
                                payload: VariantPayload::Newtype {
                                    type_ref: TypeRef::concrete(ok_id),
                                },
                            },
                            VariantSchema {
                                name: "Err".to_string(),
                                index: 1,
                                payload: VariantPayload::Newtype {
                                    type_ref: TypeRef::concrete(err_id),
                                },
                            },
                        ],
                    },
                );
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
        self.stack.push(shape);

        let kind = match shape.ty {
            // r[impl schema.format.struct]
            // r[impl schema.format.tuple]
            Type::User(UserType::Struct(struct_type)) => match struct_type.kind {
                StructKind::Unit => SchemaKind::Primitive {
                    primitive_type: PrimitiveType::Unit,
                },
                StructKind::TupleStruct | StructKind::Tuple => {
                    let elements: Vec<TypeRef<MixedId>> = struct_type
                        .fields
                        .iter()
                        .map(|f| TypeRef::concrete(self.extract(f.shape())))
                        .collect();
                    SchemaKind::Tuple { elements }
                }
                StructKind::Struct => {
                    let fields: Vec<FieldSchema<MixedId>> = struct_type
                        .fields
                        .iter()
                        .map(|f| FieldSchema {
                            name: f.name.to_string(),
                            type_ref: TypeRef::concrete(self.extract(f.shape())),
                            required: f.default.is_none(),
                        })
                        .collect();
                    SchemaKind::Struct {
                        name: format!("{shape}"),
                        fields,
                    }
                }
            },
            // r[impl schema.format.enum]
            Type::User(UserType::Enum(enum_type)) => {
                let variants: Vec<VariantSchema<MixedId>> = enum_type
                    .variants
                    .iter()
                    .enumerate()
                    .map(|(i, v)| {
                        let payload = match v.data.kind {
                            StructKind::Unit => VariantPayload::Unit,
                            StructKind::TupleStruct | StructKind::Tuple => {
                                if v.data.fields.len() == 1 {
                                    VariantPayload::Newtype {
                                        type_ref: TypeRef::concrete(
                                            self.extract(v.data.fields[0].shape()),
                                        ),
                                    }
                                } else {
                                    let types: Vec<TypeRef<MixedId>> = v
                                        .data
                                        .fields
                                        .iter()
                                        .map(|f| TypeRef::concrete(self.extract(f.shape())))
                                        .collect();
                                    VariantPayload::Tuple { types }
                                }
                            }
                            StructKind::Struct => {
                                let fields: Vec<FieldSchema<MixedId>> = v
                                    .data
                                    .fields
                                    .iter()
                                    .map(|f| FieldSchema {
                                        name: f.name.to_string(),
                                        type_ref: TypeRef::concrete(self.extract(f.shape())),
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
                SchemaKind::Enum {
                    name: format!("{shape}"),
                    variants,
                }
            }
            Type::User(UserType::Opaque) => {
                // Opaque types (like Payload) are represented as bytes on the wire.
                self.stack.pop();
                self.push_schema(
                    shape,
                    type_id,
                    SchemaKind::Primitive {
                        primitive_type: PrimitiveType::Bytes,
                    },
                );
                return type_id;
            }
            Type::Pointer(_) => {
                // Follow pointer type params
                if let Some(inner) = shape.type_params.first() {
                    self.stack.pop();
                    return self.extract(inner.shape);
                }
                panic!("schema extraction: Pointer type without type_params: {shape}");
            }
            other => panic!(
                "schema extraction: unhandled type {other:?} for shape {shape} (def={:?})",
                shape.def
            ),
        };

        self.stack.pop();

        self.push_schema(shape, type_id, kind);

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

#[cfg(test)]
mod tests {
    use super::*;
    use facet::Facet;

    // r[verify schema.type-id]
    #[test]
    fn type_ids_are_u64_content_hashes() {
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
            type_params: vec![],
            kind: SchemaKind::Primitive {
                primitive_type: PrimitiveType::U32,
            },
        };
        let bytes = build_schema_message(std::slice::from_ref(&schema), &[]);
        let payload = parse_schema_message(&bytes).expect("should parse CBOR");
        assert_eq!(payload.schemas.len(), 1);
        assert_eq!(payload.schemas[0].type_id, schema.type_id);
    }

    // r[verify schema.format.primitive]
    #[test]
    fn primitive_u32() {
        let schemas = extract_schemas(<u32 as Facet>::SHAPE);
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
        let schemas = extract_schemas(<String as Facet>::SHAPE);
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
        let schemas = extract_schemas(<bool as Facet>::SHAPE);
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

        let schemas = extract_schemas(Point::SHAPE);
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

        let schemas = extract_schemas(Color::SHAPE);
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

        let schemas = extract_schemas(Shape::SHAPE);
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
        let schemas = extract_schemas(<Vec<u32> as Facet>::SHAPE);
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
        let schemas = extract_schemas(<Option<String> as Facet>::SHAPE);
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

        let schemas = extract_schemas(Node::SHAPE);
        assert!(schemas.len() >= 2);

        let node_schema = schemas.last().unwrap();
        assert!(matches!(node_schema.kind, SchemaKind::Struct { .. }));
    }

    // r[verify schema.format.primitive]
    #[test]
    fn vec_u8_is_bytes() {
        let schemas = extract_schemas(<Vec<u8> as Facet>::SHAPE);
        assert_eq!(schemas.len(), 1);
        assert!(matches!(
            schemas[0].kind,
            SchemaKind::Primitive {
                primitive_type: PrimitiveType::Bytes
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

        let schemas = extract_schemas(TwoU32::SHAPE);
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
        let schemas = extract_schemas(<std::collections::HashMap<String, u32> as Facet>::SHAPE);
        let map_schema = schemas.last().unwrap();
        assert!(matches!(map_schema.kind, SchemaKind::Map { .. }));
    }

    // r[verify schema.format.container]
    #[test]
    fn container_array() {
        let schemas = extract_schemas(<[u32; 4] as Facet>::SHAPE);
        let arr_schema = schemas.last().unwrap();
        match &arr_schema.kind {
            SchemaKind::Array { length, .. } => assert_eq!(*length, 4),
            other => panic!("expected Array, got {other:?}"),
        }
    }

    // r[verify schema.format.tuple]
    #[test]
    fn tuple_type() {
        let schemas = extract_schemas(<(u32, String) as Facet>::SHAPE);
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

        let schemas = extract_schemas(Mixed::SHAPE);
        assert!(schemas.len() >= 4);
    }

    // r[verify schema.principles.once-per-type]
    // r[verify schema.exchange.idempotent]
    #[test]
    fn tracker_prepare_send_returns_payload_then_empty() {
        let mut tracker = SchemaSendTracker::new();
        let method = MethodId(1);
        let first =
            tracker.prepare_send_for_method(method, <u32 as Facet>::SHAPE, BindingDirection::Args);
        assert!(
            !first.is_empty(),
            "first prepare_send should return payload"
        );
        let second =
            tracker.prepare_send_for_method(method, <u32 as Facet>::SHAPE, BindingDirection::Args);
        assert!(
            second.is_empty(),
            "second prepare_send for same method should return empty"
        );
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
        let first = tracker.prepare_send_for_method(method, Outer::SHAPE, BindingDirection::Args);
        assert!(!first.is_empty(), "should return schemas");
        let parsed = first.parse().expect("should parse CBOR");
        assert!(
            parsed.schemas.len() >= 3,
            "should include transitive deps, got {}",
            parsed.schemas.len()
        );

        // Same method again — nothing to send
        let again =
            tracker.prepare_send_for_method(method, <u32 as Facet>::SHAPE, BindingDirection::Args);
        assert!(
            again.is_empty(),
            "u32 was already sent as transitive dep, method already bound"
        );
    }

    // r[verify schema.tracking.received]
    #[test]
    fn tracker_record_and_get_received() {
        let tracker = SchemaRecvTracker::new();
        let schemas = extract_schemas(<u32 as Facet>::SHAPE);
        let id = schemas[0].type_id;
        assert!(tracker.get_received(&id).is_none());
        tracker
            .record_received(SchemaPayload {
                schemas,
                method_bindings: vec![],
            })
            .expect("first record should succeed");
        assert!(tracker.get_received(&id).is_some());
    }

    // r[verify schema.type-id]
    // r[verify schema.type-id.hash]
    #[test]
    fn type_ids_are_content_hashes() {
        let mut tracker = SchemaSendTracker::new();
        let schemas = tracker.extract_schemas(<(u32, String) as Facet>::SHAPE);
        assert!(schemas.len() >= 3);

        // Same type extracted again must produce the same content hash.
        let mut tracker2 = SchemaSendTracker::new();
        let schemas2 = tracker2.extract_schemas(<(u32, String) as Facet>::SHAPE);
        assert_eq!(schemas.len(), schemas2.len());
        for (a, b) in schemas.iter().zip(schemas2.iter()) {
            assert_eq!(a.type_id, b.type_id, "content hash should be deterministic");
        }

        // Different types must produce different hashes.
        let mut tracker3 = SchemaSendTracker::new();
        let schemas3 = tracker3.extract_schemas(<(u64, String) as Facet>::SHAPE);
        let root_hash = schemas.last().unwrap().type_id;
        let root_hash3 = schemas3.last().unwrap().type_id;
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
        let hashes: Vec<TypeSchemaId> = primitives
            .iter()
            .map(|p| compute_content_hash(&SchemaKind::Primitive { primitive_type: *p }, &|id| id))
            .collect();
        let unique: HashSet<TypeSchemaId> = hashes.iter().copied().collect();
        assert_eq!(
            unique.len(),
            hashes.len(),
            "all primitive hashes must be unique"
        );

        // Verify they're deterministic (same computation, same result).
        for (i, p) in primitives.iter().enumerate() {
            let hash2 =
                compute_content_hash(&SchemaKind::Primitive { primitive_type: *p }, &|id| id);
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

        let schemas1 = extract_schemas(Point::SHAPE);
        let schemas2 = extract_schemas(Point::SHAPE);
        assert_eq!(
            schemas1.last().unwrap().type_id,
            schemas2.last().unwrap().type_id,
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

        let schemas1 = extract_schemas(TreeNode::SHAPE);
        let schemas2 = extract_schemas(TreeNode::SHAPE);

        // Must have at least String, Vec<TreeNode>, TreeNode
        assert!(schemas1.len() >= 2);

        // Same recursive type must produce identical hashes.
        let root1 = schemas1.last().unwrap().type_id;
        let root2 = schemas2.last().unwrap().type_id;
        assert_eq!(root1, root2, "recursive type hash must be deterministic");

        // All type IDs in the output must be valid content hashes (non-zero).
        for s in &schemas1 {
            assert_ne!(s.type_id.0, 0, "content hash must not be zero");
        }
    }

    #[test]
    fn bidirectional_bindings_are_independent() {
        let mut tracker = SchemaSendTracker::new();
        let method = MethodId(1);

        // Send args binding
        let args =
            tracker.prepare_send_for_method(method, <u32 as Facet>::SHAPE, BindingDirection::Args);
        assert!(!args.is_empty(), "should send args");
        let args_parsed = args.parse().expect("parse args CBOR");
        assert_eq!(args_parsed.method_bindings.len(), 1);
        assert_eq!(
            args_parsed.method_bindings[0].direction,
            BindingDirection::Args
        );

        // Send response binding for the same method — should NOT be deduplicated
        let response = tracker.prepare_send_for_method(
            method,
            <String as Facet>::SHAPE,
            BindingDirection::Response,
        );
        assert!(!response.is_empty(), "should send response");
        let response_parsed = response.parse().expect("parse response CBOR");
        assert_eq!(response_parsed.method_bindings.len(), 1);
        assert_eq!(
            response_parsed.method_bindings[0].direction,
            BindingDirection::Response
        );

        // Record received bindings and verify they go to separate maps
        let recv_tracker = SchemaRecvTracker::new();
        recv_tracker
            .record_received(SchemaPayload {
                schemas: extract_schemas(<u64 as Facet>::SHAPE),
                method_bindings: vec![
                    MethodSchemaBinding {
                        method_id: MethodId(42),
                        root_type_schema_id: TypeSchemaId(100),
                        direction: BindingDirection::Args,
                    },
                    MethodSchemaBinding {
                        method_id: MethodId(42),
                        root_type_schema_id: TypeSchemaId(200),
                        direction: BindingDirection::Response,
                    },
                ],
            })
            .expect("record should succeed");

        assert_eq!(
            recv_tracker.get_remote_args_root(MethodId(42)),
            Some(TypeSchemaId(100))
        );
        assert_eq!(
            recv_tracker.get_remote_response_root(MethodId(42)),
            Some(TypeSchemaId(200))
        );
    }

    #[test]
    fn duplicate_schema_is_protocol_error() {
        let tracker = SchemaRecvTracker::new();
        let schemas = extract_schemas(<u32 as Facet>::SHAPE);
        tracker
            .record_received(SchemaPayload {
                schemas: schemas.clone(),
                method_bindings: vec![],
            })
            .expect("first record should succeed");
        let err = tracker
            .record_received(SchemaPayload {
                schemas: schemas.clone(),
                method_bindings: vec![],
            })
            .expect_err("duplicate should fail");
        assert_eq!(err.type_id, schemas[0].type_id);
    }

    #[test]
    fn send_tracker_reset_clears_all_state() {
        let mut tracker = SchemaSendTracker::new();
        let method = MethodId(1);
        let first =
            tracker.prepare_send_for_method(method, <u32 as Facet>::SHAPE, BindingDirection::Args);
        assert!(!first.is_empty(), "first should return payload");

        tracker.reset();

        let after_reset =
            tracker.prepare_send_for_method(method, <u32 as Facet>::SHAPE, BindingDirection::Args);
        assert!(
            !after_reset.is_empty(),
            "after reset, prepare_send should return payload again"
        );
    }
}
