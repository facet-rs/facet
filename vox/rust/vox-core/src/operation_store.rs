use std::collections::BTreeMap;

use moire::sync::SyncMutex;
use vox_types::{MaybeSend, MaybeSync, MethodId, OperationId, PostcardPayload};

/// A sealed response stored in the operation store.
///
/// The store is in-process and same-version: the running code's static
/// `SHAPE` for `method_id` is the source of truth for the response's
/// schemas, both at admission time and at replay time. The store does
/// not freeze schemas alongside payloads — that would only matter for
/// cross-process / cross-version replay, and we don't promise that.
pub struct SealedResponse {
    /// Postcard-encoded response payload (without schemas).
    pub response: PostcardPayload,
    /// Method this response was produced for. Replay derives the
    /// response's static `&'static Shape` from this.
    pub method_id: MethodId,
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

/// Operation state backing for exactly-once delivery across session
/// resumption within a single process.
///
/// The default implementation is in-memory. Schemas are NOT stored —
/// they come from the running code on replay. Cross-process /
/// cross-version durability is the responsibility of `persist` methods,
/// which would require a separate store implementation that grapples
/// with schema migration; that contract is out of scope here.
pub trait OperationStore: MaybeSend + MaybeSync + 'static {
    /// Record that we're starting to process this operation.
    fn admit(&self, operation_id: OperationId);

    /// Check the state of an operation.
    fn lookup(&self, operation_id: OperationId) -> OperationState;

    /// Retrieve a sealed response.
    fn get_sealed(&self, operation_id: OperationId) -> Option<SealedResponse>;

    /// Store the sealed response for an operation. `response` is the
    /// postcard-encoded payload without schemas; `method_id` is needed
    /// at replay time to look up the response shape from the running
    /// code.
    fn seal(
        &self,
        operation_id: OperationId,
        method_id: MethodId,
        response: &PostcardPayload,
    );

    /// Remove an admitted (but not sealed) operation, e.g. after handler failure.
    fn remove(&self, operation_id: OperationId);
}

// ============================================================================
// In-memory implementation
// ============================================================================

enum InMemoryState {
    Admitted,
    Sealed {
        response: PostcardPayload,
        method_id: MethodId,
    },
}

/// Default in-memory operation store.
pub struct InMemoryOperationStore {
    inner: SyncMutex<InMemoryRegistry>,
}

#[derive(Default)]
struct InMemoryRegistry {
    operations: BTreeMap<OperationId, InMemoryState>,
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
                method_id,
            }) => Some(SealedResponse {
                response: response.clone(),
                method_id: *method_id,
            }),
            _ => None,
        }
    }

    fn seal(
        &self,
        operation_id: OperationId,
        method_id: MethodId,
        response: &PostcardPayload,
    ) {
        let mut inner = self.inner.lock();
        inner.operations.insert(
            operation_id,
            InMemoryState::Sealed {
                response: response.clone(),
                method_id,
            },
        );
        tracing::trace!(
            %operation_id,
            response_bytes = response.as_bytes().len(),
            operations = inner.operations.len(),
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
                "operation store remove admitted"
            );
        }
    }
}
