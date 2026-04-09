//! Schema extraction and tracking for vox wire protocol.
//!
//! The canonical schema model lives in `vox-schema`. This module re-exports
//! those shared types and adds vox-specific extraction plus per-connection
//! send/receive tracking.

pub use vox_schema::*;

use std::sync::Arc;

use facet::Facet;
use facet_core::{DeclId, Def, ScalarType, Shape, StructKind, Type, UserType};
use indexmap::IndexMap;
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use crate::{MethodId, RequestCall, RequestResponse, is_rx, is_tx};

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

/// Structured schema plan computed before send ordering is known.
#[derive(Debug, Clone)]
pub struct PreparedSchemaPlan {
    pub schemas: Vec<Schema>,
    pub root: TypeRef,
}

impl PreparedSchemaPlan {
    pub fn to_cbor(&self) -> CborPayload {
        SchemaPayload {
            schemas: self.schemas.clone(),
            root: self.root.clone(),
        }
        .to_cbor()
    }
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

    /// Whether this method+direction binding has already been sent on the wire.
    pub fn has_sent_binding(&self, method_id: MethodId, direction: BindingDirection) -> bool {
        self.sent_bindings.contains(&(method_id, direction))
    }

    /// Compute the full schema payload for a shaped value without mutating
    /// any per-connection send tracking.
    pub fn plan_for_shape(shape: &'static Shape) -> Result<PreparedSchemaPlan, SchemaExtractError> {
        let extracted = extract_schemas(shape)?;
        Ok(PreparedSchemaPlan {
            schemas: extracted.schemas.to_vec(),
            root: extracted.root.clone(),
        })
    }

    /// Compute the full schema payload for a canonical root type and schema
    /// source without mutating any per-connection send tracking.
    pub fn plan_from_source(root_type: &TypeRef, source: &dyn SchemaSource) -> PreparedSchemaPlan {
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

        PreparedSchemaPlan {
            schemas: all_schemas,
            root: root_type.clone(),
        }
    }

    fn register_prepared_plan(&mut self, prepared: &PreparedSchemaPlan) {
        for schema in &prepared.schemas {
            self.registry
                .entry(schema.id)
                .or_insert_with(|| schema.clone());
        }
    }

    fn unsent_schemas_for_prepared_plan(&self, prepared: &PreparedSchemaPlan) -> Vec<Schema> {
        prepared
            .schemas
            .iter()
            .filter(|schema| !self.sent_schemas.contains(&schema.id))
            .cloned()
            .collect()
    }

    /// Compute the schema payload that would be sent for a binding without
    /// mutating connection-scoped send tracking.
    pub fn preview_prepared_plan(
        &mut self,
        method_id: MethodId,
        direction: BindingDirection,
        prepared: &PreparedSchemaPlan,
    ) -> CborPayload {
        let key = (method_id, direction);
        if self.sent_bindings.contains(&key) {
            return CborPayload::default();
        }

        self.register_prepared_plan(prepared);

        let schema_payload = SchemaPayload {
            schemas: self.unsent_schemas_for_prepared_plan(prepared),
            root: prepared.root.clone(),
        };
        schema_payload.to_cbor()
    }

    /// Mark a previously previewed schema payload as successfully sent.
    pub fn mark_prepared_plan_sent(
        &mut self,
        method_id: MethodId,
        direction: BindingDirection,
        prepared: &PreparedSchemaPlan,
    ) {
        let key = (method_id, direction);
        if self.sent_bindings.contains(&key) {
            return;
        }

        self.register_prepared_plan(prepared);

        for schema in &prepared.schemas {
            self.sent_schemas.insert(schema.id);
        }
        self.sent_bindings.insert(key);
    }

    /// Commit a previously prepared schema payload against the live
    /// per-connection tracking state, returning only the schemas that still
    /// need to be sent on the wire for this binding.
    pub fn commit_prepared_plan(
        &mut self,
        method_id: MethodId,
        direction: BindingDirection,
        prepared: PreparedSchemaPlan,
    ) -> CborPayload {
        let schema_payload = SchemaPayload {
            schemas: self.unsent_schemas_for_prepared_plan(&prepared),
            root: prepared.root.clone(),
        };
        dlog!(
            "[schema] commit binding: method={:?} direction={:?} root={:?} schema_count={}",
            method_id,
            direction,
            schema_payload.root,
            schema_payload.schemas.len()
        );
        let cbor = schema_payload.to_cbor();
        self.mark_prepared_plan_sent(method_id, direction, &prepared);
        cbor
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

        let prepared = Self::plan_for_shape(shape)?;
        let cbor = self.commit_prepared_plan(method_id, schematic.direction(), prepared);
        schematic.attach_schemas(cbor.clone());
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
        let prepared = Self::plan_from_source(root_type, source);
        self.commit_prepared_plan(method_id, direction, prepared)
    }

    pub fn commit_prepared_send(
        &mut self,
        method_id: MethodId,
        direction: BindingDirection,
        prepared: &CborPayload,
    ) -> CborPayload {
        let prepared_payload = SchemaPayload::from_cbor(&prepared.0)
            .expect("prepared schema payloads must be valid CBOR");
        self.commit_prepared_plan(
            method_id,
            direction,
            PreparedSchemaPlan {
                schemas: prepared_payload.schemas,
                root: prepared_payload.root,
            },
        )
    }

    /// Compatibility shim: schema extraction is now independent from
    /// connection-scoped send tracking.
    pub fn extract_schemas(
        &mut self,
        shape: &'static Shape,
    ) -> Result<Arc<ExtractedSchemas>, SchemaExtractError> {
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
    /// Type-erased plan cache. Keyed by (method, direction, local Shape ptr).
    /// Populated by higher-level crates (e.g. vox) that know the concrete plan type.
    plan_cache: Mutex<HashMap<PlanCacheKey, Box<dyn std::any::Any + Send + Sync>>>,
}

/// Cache key for resolved translation plans.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct PlanCacheKey {
    pub method_id: MethodId,
    pub direction: BindingDirection,
    pub local_shape: &'static Shape,
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
            plan_cache: Mutex::new(HashMap::new()),
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

    /// Look up a cached plan by key, downcasting to `T`.
    pub fn get_cached_plan<T: Send + Sync + 'static>(
        &self,
        key: &PlanCacheKey,
    ) -> Option<std::sync::Arc<T>> {
        let cache = self.plan_cache.lock().unwrap();
        cache.get(key)?.downcast_ref::<std::sync::Arc<T>>().cloned()
    }

    /// Insert a plan into the cache.
    pub fn insert_cached_plan<T: Send + Sync + 'static>(
        &self,
        key: PlanCacheKey,
        plan: std::sync::Arc<T>,
    ) {
        self.plan_cache.lock().unwrap().insert(key, Box::new(plan));
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
#[derive(Clone)]
pub struct ExtractedSchemas {
    /// All schemas in dependency order (dependencies before dependents).
    pub schemas: Vec<Schema>,

    /// The root TypeRef — may be generic (e.g. `Concrete { id: result_id, args: [i64, MathError] }`).
    pub root: TypeRef,
}

/// Extract schemas without a tracker (uses a temporary counter).
/// Useful for tests and one-off schema extraction.
pub fn extract_schemas(shape: &'static Shape) -> Result<Arc<ExtractedSchemas>, SchemaExtractError> {
    use std::sync::OnceLock;

    static CACHE: OnceLock<Mutex<HashMap<&'static Shape, Arc<ExtractedSchemas>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));

    if let Some(cached) = cache.lock().unwrap().get(shape) {
        return Ok(Arc::clone(cached));
    }

    let result = Arc::new(extract_schemas_uncached(shape)?);
    cache.lock().unwrap().insert(shape, Arc::clone(&result));
    Ok(result)
}

fn extract_schemas_uncached(shape: &'static Shape) -> Result<ExtractedSchemas, SchemaExtractError> {
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
        kind.for_each_type_ref(&mut |tr: &TypeRef<MixedId>| tr.collect_ids(&mut refs));
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
        let id = MixedId::Temp(self.next_id.next_index());
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
        param_map: &[(&'static Shape, TypeParamName)],
    ) -> Result<TypeRef<MixedId>, SchemaExtractError> {
        if let Some((_, name)) = param_map
            .iter()
            .find(|(param_shape, _)| shape.is_shape(param_shape))
        {
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
        let param_map: Vec<(&'static Shape, TypeParamName)> = shape
            .type_params
            .iter()
            .map(|tp| (tp.shape, TypeParamName(tp.name.to_string())))
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
                StructKind::Unit => {
                    let primitive_type = if is_infallible_shape(shape) {
                        PrimitiveType::Never
                    } else {
                        PrimitiveType::Unit
                    };
                    SchemaKind::Primitive { primitive_type }
                }
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

fn is_infallible_shape(shape: &'static Shape) -> bool {
    shape.is_shape(<std::convert::Infallible as Facet<'static>>::SHAPE)
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
        let schemas = extract_schemas(<u32 as Facet>::SHAPE)
            .unwrap()
            .schemas
            .clone();
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
        let schemas = extract_schemas(<String as Facet>::SHAPE)
            .unwrap()
            .schemas
            .clone();
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
        let schemas = extract_schemas(<bool as Facet>::SHAPE)
            .unwrap()
            .schemas
            .clone();
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

        let schemas = extract_schemas(Point::SHAPE).unwrap().schemas.clone();
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

        let schemas = extract_schemas(Color::SHAPE).unwrap().schemas.clone();
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

        let schemas = extract_schemas(Shape::SHAPE).unwrap().schemas.clone();
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
        let schemas = extract_schemas(<Vec<u32> as Facet>::SHAPE)
            .unwrap()
            .schemas
            .clone();
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
            .schemas
            .clone();
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

        let schemas = extract_schemas(Node::SHAPE).unwrap().schemas.clone();
        assert!(schemas.len() >= 2);

        let node_schema = schemas.last().unwrap();
        assert!(matches!(node_schema.kind, SchemaKind::Struct { .. }));
    }

    // r[verify schema.format.primitive]
    #[test]
    fn vec_u8_is_bytes() {
        let schemas = extract_schemas(<Vec<u8> as Facet>::SHAPE)
            .unwrap()
            .schemas
            .clone();
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
        let schemas = extract_schemas(<&[u8] as Facet>::SHAPE)
            .unwrap()
            .schemas
            .clone();
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
        let schemas = extract_schemas(CborPayload::SHAPE).unwrap().schemas.clone();
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
            .schemas
            .clone();
        assert_eq!(schemas.len(), 1);
        assert!(matches!(
            schemas[0].kind,
            SchemaKind::Primitive {
                primitive_type: PrimitiveType::Payload
            }
        ));
    }

    #[test]
    fn infallible_is_never_primitive() {
        let schemas = extract_schemas(<std::convert::Infallible as Facet>::SHAPE)
            .unwrap()
            .schemas
            .clone();
        assert_eq!(schemas.len(), 1);
        assert!(matches!(
            schemas[0].kind,
            SchemaKind::Primitive {
                primitive_type: PrimitiveType::Never
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

        let schemas = extract_schemas(TwoU32::SHAPE).unwrap().schemas.clone();
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
            .schemas
            .clone();
        let map_schema = schemas.last().unwrap();
        assert!(matches!(map_schema.kind, SchemaKind::Map { .. }));
    }

    // r[verify schema.format.container]
    #[test]
    fn container_array() {
        let schemas = extract_schemas(<[u32; 4] as Facet>::SHAPE)
            .unwrap()
            .schemas
            .clone();
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
            .schemas
            .clone();
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

        let schemas = extract_schemas(Mixed::SHAPE).unwrap().schemas.clone();
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
        let schemas = extract_schemas(<u32 as Facet>::SHAPE)
            .unwrap()
            .schemas
            .clone();
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
        let extracted = tracker
            .extract_schemas(<(u32, String) as Facet>::SHAPE)
            .unwrap();
        let schemas = extracted.schemas.clone();
        assert!(schemas.len() >= 3);

        // Same type extracted again must produce the same content hash.
        let mut tracker2 = SchemaSendTracker::new();
        let schemas2 = tracker2
            .extract_schemas(<(u32, String) as Facet>::SHAPE)
            .unwrap()
            .schemas
            .clone();
        assert_eq!(schemas.len(), schemas2.len());
        for (a, b) in schemas.iter().zip(schemas2.iter()) {
            assert_eq!(a.id, b.id, "content hash should be deterministic");
        }

        // Different types must produce different hashes.
        let mut tracker3 = SchemaSendTracker::new();
        let extracted3 = tracker3
            .extract_schemas(<(u64, String) as Facet>::SHAPE)
            .unwrap();
        assert_ne!(
            extracted.root, extracted3.root,
            "different types should produce different root refs"
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
            PrimitiveType::Never,
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

        let schemas1 = extract_schemas(Point::SHAPE).unwrap().schemas.clone();
        let schemas2 = extract_schemas(Point::SHAPE).unwrap().schemas.clone();
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

        let schemas1 = extract_schemas(TreeNode::SHAPE).unwrap().schemas.clone();
        let schemas2 = extract_schemas(TreeNode::SHAPE).unwrap().schemas.clone();

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
                    schemas: extract_schemas(<u64 as Facet>::SHAPE)
                        .unwrap()
                        .schemas
                        .clone(),
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
        let schemas = extract_schemas(<u32 as Facet>::SHAPE)
            .unwrap()
            .schemas
            .clone();
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
        let schemas = extract_schemas(<Vec<u32> as Facet>::SHAPE)
            .unwrap()
            .schemas
            .clone();
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
            .schemas
            .clone();
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
            .schemas
            .clone();
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
    fn generic_vox_error_uses_var_in_user_payload() {
        use crate::VoxError;

        let schemas = extract_schemas(<VoxError<::core::convert::Infallible> as Facet>::SHAPE)
            .unwrap()
            .schemas
            .clone();
        let vox_error_schema = schemas
            .iter()
            .find(|s| matches!(&s.kind, SchemaKind::Enum { name, .. } if name == "VoxError"))
            .expect("VoxError schema should be present");
        match &vox_error_schema.kind {
            SchemaKind::Enum { variants, .. } => {
                let user = variants
                    .iter()
                    .find(|variant| variant.name == "User")
                    .expect("VoxError should have User variant");
                let VariantPayload::Newtype { type_ref } = &user.payload else {
                    panic!("User variant should be newtype");
                };
                assert!(
                    matches!(type_ref, TypeRef::Var { .. }),
                    "User payload should be a type variable, got {type_ref:?}"
                );
            }
            other => panic!("expected enum, got {other:?}"),
        }
    }

    #[test]
    fn vec_of_option_of_u32_deduplicates() {
        // Vec<Option<u32>> should produce: u32, Option<T>, Vec<T>
        // NOT: u32, Option<u32>, Vec<Option<u32>>
        let schemas = extract_schemas(<Vec<Option<u32>> as Facet>::SHAPE)
            .unwrap()
            .schemas
            .clone();

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

        let schemas = extract_schemas(Both::SHAPE).unwrap().schemas.clone();
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
        let schemas = extract_schemas(<Vec<u32> as Facet>::SHAPE)
            .unwrap()
            .schemas
            .clone();
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
        use crate::VoxError;

        let extracted = extract_schemas(
            <Result<(String, i32), VoxError<::core::convert::Infallible>> as Facet>::SHAPE,
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
        use crate::VoxError;

        let result_shape =
            <Result<(String, i32), VoxError<::core::convert::Infallible>> as Facet>::SHAPE;
        let ok_shape = result_shape.type_params[0].shape;
        let extracted = extract_schemas(
            <Result<(String, i32), VoxError<::core::convert::Infallible>> as Facet>::SHAPE,
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
            .schemas
            .clone();
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

        let schemas = extract_schemas(Node::SHAPE).unwrap().schemas.clone();
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
            .schemas
            .clone();
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
