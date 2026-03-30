use std::collections::{BTreeMap, HashMap};

use moire::sync::SyncMutex;
use vox_types::{
    MaybeSend, MaybeSync, OperationId, PostcardPayload, Schema, SchemaHash, SchemaRegistry,
    SchemaSource, TypeRef,
};

/// A sealed response stored in the operation store.
pub struct SealedResponse {
    /// Postcard-encoded response payload (without schemas).
    pub response: PostcardPayload,
    /// Root type ref for rebuilding the schema payload on replay.
    pub root_type: TypeRef,
}

/// State of an operation in the store.
pub enum OperationState {
    /// Never seen this operation ID.
    Unknown,
    /// Operation was admitted but never sealed (crash/disconnect before completion).
    Admitted,
    /// Operation completed and response is available.
    Sealed,
}

/// Operation state backing for exactly-once delivery across session resumption.
///
/// The default implementation is in-memory. Applications that want durability
/// can implement this trait backed by a database.
///
/// Schemas are stored separately from payloads, deduplicated by SchemaHash.
pub trait OperationStore: MaybeSend + MaybeSync + 'static {
    /// Record that we're starting to process this operation.
    fn admit(&self, operation_id: OperationId);

    /// Check the state of an operation.
    fn lookup(&self, operation_id: OperationId) -> OperationState;

    /// Retrieve a sealed response.
    fn get_sealed(&self, operation_id: OperationId) -> Option<SealedResponse>;

    /// Store the sealed response for an operation.
    ///
    /// `response` is the postcard-encoded payload WITHOUT schemas.
    /// The store pulls needed schemas from `registry`, deduplicated by SchemaHash.
    fn seal(
        &self,
        operation_id: OperationId,
        response: &PostcardPayload,
        root_type: &TypeRef,
        registry: &SchemaRegistry,
    );

    /// Remove an admitted (but not sealed) operation, e.g. after handler failure.
    fn remove(&self, operation_id: OperationId);

    /// Access the store's schema source for looking up schemas by hash.
    fn schema_source(&self) -> &dyn SchemaSource;
}

// ============================================================================
// In-memory implementation
// ============================================================================

enum InMemoryState {
    Admitted,
    Sealed {
        response: PostcardPayload,
        root_type: TypeRef,
    },
}

/// Default in-memory operation store.
pub struct InMemoryOperationStore {
    inner: SyncMutex<InMemoryRegistry>,
}

#[derive(Default)]
struct InMemoryRegistry {
    operations: BTreeMap<OperationId, InMemoryState>,
    schemas: HashMap<SchemaHash, Schema>,
}

impl InMemoryOperationStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Default for InMemoryOperationStore {
    fn default() -> Self {
        Self {
            inner: SyncMutex::new("driver.operations", InMemoryRegistry::default()),
        }
    }
}

impl SchemaSource for InMemoryOperationStore {
    fn get_schema(&self, id: SchemaHash) -> Option<Schema> {
        self.inner.lock().schemas.get(&id).cloned()
    }
}

impl OperationStore for InMemoryOperationStore {
    fn admit(&self, operation_id: OperationId) {
        let mut inner = self.inner.lock();
        inner
            .operations
            .entry(operation_id)
            .or_insert(InMemoryState::Admitted);
        tracing::trace!(
            %operation_id,
            operations = inner.operations.len(),
            schemas = inner.schemas.len(),
            "operation store admit"
        );
    }

    fn lookup(&self, operation_id: OperationId) -> OperationState {
        let inner = self.inner.lock();
        match inner.operations.get(&operation_id) {
            None => OperationState::Unknown,
            Some(InMemoryState::Admitted) => OperationState::Admitted,
            Some(InMemoryState::Sealed { .. }) => OperationState::Sealed,
        }
    }

    fn get_sealed(&self, operation_id: OperationId) -> Option<SealedResponse> {
        let inner = self.inner.lock();
        match inner.operations.get(&operation_id) {
            Some(InMemoryState::Sealed {
                response,
                root_type,
            }) => Some(SealedResponse {
                response: response.clone(),
                root_type: root_type.clone(),
            }),
            _ => None,
        }
    }

    fn seal(
        &self,
        operation_id: OperationId,
        response: &PostcardPayload,
        root_type: &TypeRef,
        registry: &SchemaRegistry,
    ) {
        let mut inner = self.inner.lock();
        // Store schemas the store doesn't have yet.
        let mut queue = Vec::new();
        root_type.collect_ids(&mut queue);
        let mut visited = std::collections::HashSet::new();
        while let Some(id) = queue.pop() {
            if !visited.insert(id) {
                continue;
            }
            if inner.schemas.contains_key(&id) {
                continue;
            }
            if let Some(schema) = registry.get(&id) {
                for child_id in vox_types::schema_child_ids(&schema.kind) {
                    queue.push(child_id);
                }
                inner.schemas.insert(id, schema.clone());
            }
        }
        inner.operations.insert(
            operation_id,
            InMemoryState::Sealed {
                response: response.clone(),
                root_type: root_type.clone(),
            },
        );
        tracing::trace!(
            %operation_id,
            response_bytes = response.as_bytes().len(),
            operations = inner.operations.len(),
            schemas = inner.schemas.len(),
            "operation store seal"
        );
    }

    fn remove(&self, operation_id: OperationId) {
        let mut inner = self.inner.lock();
        if matches!(
            inner.operations.get(&operation_id),
            Some(InMemoryState::Admitted)
        ) {
            inner.operations.remove(&operation_id);
            tracing::trace!(
                %operation_id,
                operations = inner.operations.len(),
                schemas = inner.schemas.len(),
                "operation store remove admitted"
            );
        }
    }

    fn schema_source(&self) -> &dyn SchemaSource {
        self
    }
}
