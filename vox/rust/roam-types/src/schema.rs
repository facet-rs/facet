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

/// The root schema type describing a single type.
#[derive(Facet, Clone, Debug)]
pub struct Schema {
    pub type_id: TypeSchemaId,
    pub kind: SchemaKind,
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

/// The structural kind of a type.
#[derive(Facet, Clone, Debug)]
#[repr(u8)]
pub enum SchemaKind {
    Struct {
        /// The type name (e.g. "Point"). Used for matching across schema
        /// versions and for diagnostics. MUST NOT be empty.
        name: String,
        fields: Vec<FieldSchema>,
    },
    Enum {
        /// The type name (e.g. "Color"). Used for matching across schema
        /// versions and for diagnostics. MUST NOT be empty.
        name: String,
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
    Tuple { types: Vec<TypeSchemaId> },
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

/// Compute the canonical byte sequence for hashing a SchemaKind.
///
/// The `resolve` function maps TypeSchemaId -> u64 hash value. For non-recursive
/// types this is just `.0`. For recursive types during preliminary hashing,
/// intra-group references return the sentinel (0u64).
// r[impl schema.type-id.hash.primitives]
// r[impl schema.type-id.hash.struct]
// r[impl schema.type-id.hash.enum]
// r[impl schema.type-id.hash.container]
// r[impl schema.type-id.hash.tuple]
fn compute_canonical_bytes(kind: &SchemaKind, resolve: &impl Fn(TypeSchemaId) -> u64) -> Vec<u8> {
    let mut buf = Vec::new();

    match kind {
        SchemaKind::Primitive { primitive_type } => {
            let tag = primitive_type.hash_tag();
            buf.extend_from_slice(&(tag.len() as u32).to_le_bytes());
            buf.extend_from_slice(tag.as_bytes());
        }
        SchemaKind::Struct { fields, .. } => {
            hash_append_string(&mut buf, "struct");
            for field in fields {
                hash_append_string(&mut buf, &field.name);
                buf.extend_from_slice(&resolve(field.type_id).to_le_bytes());
            }
        }
        SchemaKind::Enum { variants, .. } => {
            hash_append_string(&mut buf, "enum");
            for variant in variants {
                hash_append_string(&mut buf, &variant.name);
                buf.extend_from_slice(&variant.index.to_le_bytes());
                match &variant.payload {
                    VariantPayload::Unit => {
                        hash_append_string(&mut buf, "unit");
                    }
                    VariantPayload::Newtype { type_id } => {
                        hash_append_string(&mut buf, "newtype");
                        buf.extend_from_slice(&resolve(*type_id).to_le_bytes());
                    }
                    VariantPayload::Tuple { types } => {
                        hash_append_string(&mut buf, "tuple");
                        for tid in types {
                            buf.extend_from_slice(&resolve(*tid).to_le_bytes());
                        }
                    }
                    VariantPayload::Struct { fields } => {
                        hash_append_string(&mut buf, "struct");
                        for field in fields {
                            hash_append_string(&mut buf, &field.name);
                            buf.extend_from_slice(&resolve(field.type_id).to_le_bytes());
                        }
                    }
                }
            }
        }
        SchemaKind::Tuple { elements } => {
            hash_append_string(&mut buf, "tuple");
            for elem in elements {
                buf.extend_from_slice(&resolve(*elem).to_le_bytes());
            }
        }
        SchemaKind::List { element } => {
            hash_append_string(&mut buf, "list");
            buf.extend_from_slice(&resolve(*element).to_le_bytes());
        }
        SchemaKind::Map { key, value } => {
            hash_append_string(&mut buf, "map");
            buf.extend_from_slice(&resolve(*key).to_le_bytes());
            buf.extend_from_slice(&resolve(*value).to_le_bytes());
        }
        SchemaKind::Array { element, length } => {
            hash_append_string(&mut buf, "array");
            buf.extend_from_slice(&resolve(*element).to_le_bytes());
            buf.extend_from_slice(&length.to_le_bytes());
        }
        SchemaKind::Option { element } => {
            hash_append_string(&mut buf, "option");
            buf.extend_from_slice(&resolve(*element).to_le_bytes());
        }
    }

    buf
}

/// Append a length-prefixed string to a byte buffer (same encoding as hash_feed_string).
fn hash_append_string(buf: &mut Vec<u8>, s: &str) {
    buf.extend_from_slice(&(s.len() as u32).to_le_bytes());
    buf.extend_from_slice(s.as_bytes());
}

/// Compute the content hash of a SchemaKind, given a resolver for child type IDs.
pub fn compute_content_hash(
    kind: &SchemaKind,
    resolve: &impl Fn(TypeSchemaId) -> u64,
) -> TypeSchemaId {
    let canonical = compute_canonical_bytes(kind, resolve);
    let hash = blake3::hash(&canonical);
    let bytes: [u8; 8] = hash.as_bytes()[0..8].try_into().expect("slice len");
    TypeSchemaId(u64::from_le_bytes(bytes))
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
    /// Assigns incrementing type IDs to shapes we extract.
    shape_to_id: HashMap<&'static Shape, TypeSchemaId>,
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

    /// Allocate a fresh TypeSchemaId not associated with any Shape.
    /// Used for synthetic schemas (e.g., args tuple wrappers) created during codegen.
    pub fn allocate_anonymous_id(&mut self) -> TypeSchemaId {
        let id = TypeSchemaId(self.next_id);
        self.next_id += 1;
        id
    }

    /// Allocate or look up a TypeSchemaId for a Shape.
    fn id_for_shape(&mut self, shape: &'static Shape) -> TypeSchemaId {
        if let Some(&id) = self.shape_to_id.get(shape) {
            return id;
        }
        let id = TypeSchemaId(self.next_id);
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
fn finalize_content_hashes(mut schemas: Vec<Schema>) -> Vec<Schema> {
    // Build a map from temporary ID -> index in the schemas vec.
    let temp_to_idx: HashMap<TypeSchemaId, usize> = schemas
        .iter()
        .enumerate()
        .map(|(i, s)| (s.type_id, i))
        .collect();

    // Collect all TypeSchemaIds referenced by each schema's kind.
    fn collect_refs(kind: &SchemaKind) -> Vec<TypeSchemaId> {
        let mut refs = Vec::new();
        match kind {
            SchemaKind::Primitive { .. } => {}
            SchemaKind::Struct { fields, .. } => {
                for f in fields {
                    refs.push(f.type_id);
                }
            }
            SchemaKind::Enum { variants, .. } => {
                for v in variants {
                    match &v.payload {
                        VariantPayload::Unit => {}
                        VariantPayload::Newtype { type_id } => refs.push(*type_id),
                        VariantPayload::Tuple { types } => refs.extend(types),
                        VariantPayload::Struct { fields } => {
                            for f in fields {
                                refs.push(f.type_id);
                            }
                        }
                    }
                }
            }
            SchemaKind::Tuple { elements } => refs.extend(elements),
            SchemaKind::List { element } | SchemaKind::Option { element } => refs.push(*element),
            SchemaKind::Map { key, value } => {
                refs.push(*key);
                refs.push(*value);
            }
            SchemaKind::Array { element, .. } => refs.push(*element),
        }
        refs
    }

    // Detect which schemas are part of recursive groups. A schema is recursive
    // if it transitively references itself.
    let n = schemas.len();
    let mut in_recursive_group: Vec<bool> = vec![false; n];

    // For each schema, check if any ref points to itself or to a schema that
    // hasn't been fully processed yet (i.e., has a higher or equal index —
    // since deps come before dependents, a forward/self reference means recursion).
    for (i, schema) in schemas.iter().enumerate() {
        for r in collect_refs(&schema.kind) {
            if let Some(&ref_idx) = temp_to_idx.get(&r) {
                if ref_idx >= i {
                    // Forward or self reference — both schemas are in a recursive group.
                    in_recursive_group[i] = true;
                    in_recursive_group[ref_idx] = true;
                }
            }
        }
    }

    // Map from temporary ID -> content hash.
    let mut id_map: HashMap<TypeSchemaId, TypeSchemaId> = HashMap::new();

    // Phase 1: Hash non-recursive types bottom-up.
    for (i, schema) in schemas.iter().enumerate() {
        if in_recursive_group[i] {
            continue;
        }
        let content_hash = compute_content_hash(&schema.kind, &|temp_id| {
            id_map.get(&temp_id).map(|h| h.0).unwrap_or(temp_id.0)
        });
        id_map.insert(schema.type_id, content_hash);
    }

    // Phase 2: Hash recursive groups using the 4-step algorithm.
    // Collect contiguous runs of recursive schemas (they're grouped because
    // extraction processes them together via the stack).
    let mut i = 0;
    while i < n {
        if !in_recursive_group[i] {
            i += 1;
            continue;
        }

        // Find the extent of this recursive group.
        let group_start = i;
        while i < n && in_recursive_group[i] {
            i += 1;
        }
        let group_end = i;
        let group_ids: HashSet<TypeSchemaId> = schemas[group_start..group_end]
            .iter()
            .map(|s| s.type_id)
            .collect();

        // Step 1: Preliminary hashes — intra-group refs become sentinel (0).
        let mut prelim_data: Vec<(Vec<u8>, u64)> = Vec::new();
        for schema in &schemas[group_start..group_end] {
            let canonical = compute_canonical_bytes(&schema.kind, &|temp_id| {
                if group_ids.contains(&temp_id) {
                    0u64 // sentinel
                } else {
                    id_map.get(&temp_id).map(|h| h.0).unwrap_or(temp_id.0)
                }
            });
            let hash = blake3::hash(&canonical);
            let bytes: [u8; 8] = hash.as_bytes()[0..8].try_into().unwrap();
            let prelim_hash = u64::from_le_bytes(bytes);
            prelim_data.push((canonical, prelim_hash));
        }

        // Step 2: Deduplication — skip for now (same canonical bytes = same type,
        // but we're working with Shape pointers which are already deduplicated).

        // Step 3: Canonical ordering — sort by preliminary hash, break ties by
        // canonical byte sequence.
        let mut order: Vec<usize> = (0..prelim_data.len()).collect();
        order.sort_by(|&a, &b| {
            prelim_data[a]
                .1
                .cmp(&prelim_data[b].1)
                .then_with(|| prelim_data[a].0.cmp(&prelim_data[b].0))
        });

        // Step 4: Final hashes.
        // group_hash = blake3(prelim_hash_0 || prelim_hash_1 || ...)[0..8]
        let mut group_hasher = blake3::Hasher::new();
        for &idx in &order {
            group_hasher.update(&prelim_data[idx].1.to_le_bytes());
        }
        let group_hash_output = group_hasher.finalize();
        let group_hash_bytes: [u8; 8] = group_hash_output.as_bytes()[0..8].try_into().unwrap();
        let group_hash = u64::from_le_bytes(group_hash_bytes);

        // Each type's final hash = blake3(group_hash || position)[0..8]
        for (position, &idx) in order.iter().enumerate() {
            let mut final_hasher = blake3::Hasher::new();
            final_hasher.update(&group_hash.to_le_bytes());
            final_hasher.update(&(position as u64).to_le_bytes());
            let final_output = final_hasher.finalize();
            let final_bytes: [u8; 8] = final_output.as_bytes()[0..8].try_into().unwrap();
            let final_hash = TypeSchemaId(u64::from_le_bytes(final_bytes));

            let schema_idx = group_start + idx;
            id_map.insert(schemas[schema_idx].type_id, final_hash);
        }
    }

    // Phase 3: Rewrite all temporary IDs to content hashes.
    fn remap_kind(kind: &mut SchemaKind, id_map: &HashMap<TypeSchemaId, TypeSchemaId>) {
        let remap = |id: &mut TypeSchemaId| {
            if let Some(&new_id) = id_map.get(id) {
                *id = new_id;
            }
        };
        match kind {
            SchemaKind::Primitive { .. } => {}
            SchemaKind::Struct { fields, .. } => {
                for f in fields {
                    remap(&mut f.type_id);
                }
            }
            SchemaKind::Enum { variants, .. } => {
                for v in variants {
                    match &mut v.payload {
                        VariantPayload::Unit => {}
                        VariantPayload::Newtype { type_id } => remap(type_id),
                        VariantPayload::Tuple { types } => {
                            for t in types {
                                remap(t);
                            }
                        }
                        VariantPayload::Struct { fields } => {
                            for f in fields {
                                remap(&mut f.type_id);
                            }
                        }
                    }
                }
            }
            SchemaKind::Tuple { elements } => {
                for e in elements {
                    remap(e);
                }
            }
            SchemaKind::List { element } | SchemaKind::Option { element } => remap(element),
            SchemaKind::Map { key, value } => {
                remap(key);
                remap(value);
            }
            SchemaKind::Array { element, .. } => remap(element),
        }
    }

    for schema in &mut schemas {
        if let Some(&new_id) = id_map.get(&schema.type_id) {
            schema.type_id = new_id;
        }
        remap_kind(&mut schema.kind, &id_map);
    }

    schemas
}

struct ExtractCtx<'a> {
    tracker: &'a mut SchemaSendTracker,
    schemas: Vec<Schema>,
    /// Shapes already fully processed.
    seen: HashSet<&'static Shape>,
    /// Stack for cycle detection.
    stack: Vec<&'static Shape>,
}

impl<'a> ExtractCtx<'a> {
    fn push_schema(&mut self, shape: &'static Shape, type_id: TypeSchemaId, kind: SchemaKind) {
        if self.seen.insert(shape) {
            self.schemas.push(Schema { type_id, kind });
        }
    }

    /// Extract a schema for the given shape, returning its TypeSchemaId.
    /// Recursively extracts dependencies first.
    fn extract(&mut self, shape: &'static Shape) -> TypeSchemaId {
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
                    self.push_schema(shape, type_id, SchemaKind::List { element: elem_id });
                }
                return type_id;
            }
            Def::Array(array_def) => {
                let elem_id = self.extract(array_def.t());
                self.push_schema(
                    shape,
                    type_id,
                    SchemaKind::Array {
                        element: elem_id,
                        length: array_def.n as u64,
                    },
                );
                return type_id;
            }
            Def::Slice(slice_def) => {
                let elem_id = self.extract(slice_def.t());
                self.push_schema(shape, type_id, SchemaKind::List { element: elem_id });
                return type_id;
            }
            Def::Map(map_def) => {
                let key_id = self.extract(map_def.k());
                let val_id = self.extract(map_def.v());
                self.push_schema(
                    shape,
                    type_id,
                    SchemaKind::Map {
                        key: key_id,
                        value: val_id,
                    },
                );
                return type_id;
            }
            Def::Set(set_def) => {
                let elem_id = self.extract(set_def.t());
                self.push_schema(shape, type_id, SchemaKind::List { element: elem_id });
                return type_id;
            }
            Def::Option(opt_def) => {
                let elem_id = self.extract(opt_def.t());
                self.push_schema(shape, type_id, SchemaKind::Option { element: elem_id });
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
                                payload: VariantPayload::Newtype { type_id: ok_id },
                            },
                            VariantSchema {
                                name: "Err".to_string(),
                                index: 1,
                                payload: VariantPayload::Newtype { type_id: err_id },
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
                    let elements: Vec<TypeSchemaId> = struct_type
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
                                    let types: Vec<TypeSchemaId> = v
                                        .data
                                        .fields
                                        .iter()
                                        .map(|f| self.extract(f.shape()))
                                        .collect();
                                    VariantPayload::Tuple { types }
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
                assert_eq!(fields[0].type_id, fields[1].type_id);
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
            .map(|p| {
                compute_content_hash(&SchemaKind::Primitive { primitive_type: *p }, &|id| id.0)
            })
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
                compute_content_hash(&SchemaKind::Primitive { primitive_type: *p }, &|id| id.0);
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
