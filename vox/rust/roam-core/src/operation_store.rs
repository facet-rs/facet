use std::{collections::BTreeMap, sync::Arc};

use moire::sync::SyncMutex;
use roam_types::{MaybeSend, MaybeSync, MethodId, RequestId, RetryPolicy};

#[derive(Clone, PartialEq, Eq)]
struct OperationSignature {
    method_id: MethodId,
    args: Arc<[u8]>,
}

impl OperationSignature {
    fn from_args(method_id: MethodId, args: &[u8]) -> Self {
        Self {
            method_id,
            args: Arc::<[u8]>::from(args.to_vec()),
        }
    }

    fn matches_call(&self, method_id: MethodId, args: &[u8]) -> bool {
        self.method_id == method_id && self.args.as_ref() == args
    }
}

struct StoredOperation {
    signature: OperationSignature,
    retry: RetryPolicy,
}

struct LiveOperation {
    stored: StoredOperation,
    owner_request_id: RequestId,
    waiters: Vec<RequestId>,
}

struct SealedOperation {
    stored: StoredOperation,
    encoded_response: Arc<[u8]>,
}

enum OperationState {
    Live(LiveOperation),
    Released(StoredOperation),
    Sealed(SealedOperation),
    Indeterminate(StoredOperation),
}

/// Result of admitting an operation ID against the current store state.
pub enum OperationAdmit {
    Start,
    Attached,
    Replay(Arc<[u8]>),
    Conflict,
    Indeterminate,
}

/// Effect of cancelling one waiter or owner request.
pub enum OperationCancel {
    None,
    DetachOnly,
    Release {
        owner_request_id: RequestId,
        waiters: Vec<RequestId>,
    },
}

/// Connection-scoped operation state backing for retry/session recovery.
///
/// The default implementation is in-memory. Applications that want stronger
/// retention or durability can provide their own implementation.
pub trait OperationStore: MaybeSend + MaybeSync + 'static {
    fn admit(
        &self,
        operation_id: u64,
        method_id: MethodId,
        args: &[u8],
        retry: RetryPolicy,
        request_id: RequestId,
    ) -> OperationAdmit;

    fn seal(
        &self,
        operation_id: u64,
        owner_request_id: RequestId,
        encoded_response: Arc<[u8]>,
    ) -> Vec<RequestId>;

    fn fail_without_reply(&self, operation_id: u64, owner_request_id: RequestId) -> Vec<RequestId>;

    fn cancel(&self, request_id: RequestId) -> OperationCancel;
}

#[derive(Default)]
struct OperationRegistry {
    states: BTreeMap<u64, OperationState>,
    request_to_operation: BTreeMap<RequestId, u64>,
}

impl OperationRegistry {
    fn admit(
        &mut self,
        operation_id: u64,
        method_id: MethodId,
        args: &[u8],
        retry: RetryPolicy,
        request_id: RequestId,
    ) -> OperationAdmit {
        let signature = OperationSignature::from_args(method_id, args);
        let Some(existing) = self.states.remove(&operation_id) else {
            self.request_to_operation.insert(request_id, operation_id);
            self.states.insert(
                operation_id,
                OperationState::Live(LiveOperation {
                    stored: StoredOperation { signature, retry },
                    owner_request_id: request_id,
                    waiters: vec![request_id],
                }),
            );
            return OperationAdmit::Start;
        };

        match existing {
            OperationState::Live(mut live) => {
                if !live.stored.signature.matches_call(method_id, args) {
                    self.states.insert(operation_id, OperationState::Live(live));
                    return OperationAdmit::Conflict;
                }
                live.waiters.push(request_id);
                self.request_to_operation.insert(request_id, operation_id);
                self.states.insert(operation_id, OperationState::Live(live));
                OperationAdmit::Attached
            }
            OperationState::Sealed(sealed) => {
                let replay = if sealed.stored.signature.matches_call(method_id, args) {
                    OperationAdmit::Replay(Arc::clone(&sealed.encoded_response))
                } else {
                    OperationAdmit::Conflict
                };
                self.states
                    .insert(operation_id, OperationState::Sealed(sealed));
                replay
            }
            OperationState::Released(stored) => {
                if !stored.signature.matches_call(method_id, args) || !stored.retry.idem {
                    let admit = if stored.signature.matches_call(method_id, args) {
                        OperationAdmit::Indeterminate
                    } else {
                        OperationAdmit::Conflict
                    };
                    self.states
                        .insert(operation_id, OperationState::Released(stored));
                    return admit;
                }
                self.request_to_operation.insert(request_id, operation_id);
                self.states.insert(
                    operation_id,
                    OperationState::Live(LiveOperation {
                        stored: StoredOperation {
                            signature,
                            retry: stored.retry,
                        },
                        owner_request_id: request_id,
                        waiters: vec![request_id],
                    }),
                );
                OperationAdmit::Start
            }
            OperationState::Indeterminate(stored) => {
                if !stored.signature.matches_call(method_id, args) || !stored.retry.idem {
                    let admit = if stored.signature.matches_call(method_id, args) {
                        OperationAdmit::Indeterminate
                    } else {
                        OperationAdmit::Conflict
                    };
                    self.states
                        .insert(operation_id, OperationState::Indeterminate(stored));
                    return admit;
                }
                self.request_to_operation.insert(request_id, operation_id);
                self.states.insert(
                    operation_id,
                    OperationState::Live(LiveOperation {
                        stored: StoredOperation {
                            signature,
                            retry: stored.retry,
                        },
                        owner_request_id: request_id,
                        waiters: vec![request_id],
                    }),
                );
                OperationAdmit::Start
            }
        }
    }

    fn seal(
        &mut self,
        operation_id: u64,
        owner_request_id: RequestId,
        encoded_response: Arc<[u8]>,
    ) -> Vec<RequestId> {
        let Some(existing) = self.states.remove(&operation_id) else {
            return vec![];
        };
        let OperationState::Live(live) = existing else {
            self.states.insert(operation_id, existing);
            return vec![];
        };
        if live.owner_request_id != owner_request_id {
            self.states.insert(operation_id, OperationState::Live(live));
            return vec![];
        }
        for waiter in &live.waiters {
            self.request_to_operation.remove(waiter);
        }
        let waiters = live.waiters.clone();
        self.states.insert(
            operation_id,
            OperationState::Sealed(SealedOperation {
                stored: live.stored,
                encoded_response,
            }),
        );
        waiters
    }

    fn fail_without_reply(
        &mut self,
        operation_id: u64,
        owner_request_id: RequestId,
    ) -> Vec<RequestId> {
        let Some(existing) = self.states.remove(&operation_id) else {
            return vec![];
        };
        let OperationState::Live(live) = existing else {
            self.states.insert(operation_id, existing);
            return vec![];
        };
        if live.owner_request_id != owner_request_id {
            self.states.insert(operation_id, OperationState::Live(live));
            return vec![];
        }
        for waiter in &live.waiters {
            self.request_to_operation.remove(waiter);
        }
        let waiters = live.waiters.clone();
        let next = if live.stored.retry.persist {
            OperationState::Indeterminate(live.stored)
        } else {
            OperationState::Released(live.stored)
        };
        self.states.insert(operation_id, next);
        waiters
    }

    fn cancel(&mut self, request_id: RequestId) -> OperationCancel {
        let Some(operation_id) = self.request_to_operation.get(&request_id).copied() else {
            return OperationCancel::None;
        };
        let Some(OperationState::Live(live)) = self.states.get_mut(&operation_id) else {
            self.request_to_operation.remove(&request_id);
            return OperationCancel::None;
        };

        if live.stored.retry.persist {
            if live.owner_request_id == request_id {
                return OperationCancel::None;
            }
            live.waiters.retain(|candidate| *candidate != request_id);
            self.request_to_operation.remove(&request_id);
            return OperationCancel::DetachOnly;
        }

        let Some(OperationState::Live(live)) = self.states.remove(&operation_id) else {
            return OperationCancel::None;
        };
        for waiter in &live.waiters {
            self.request_to_operation.remove(waiter);
        }
        let waiters = live.waiters.clone();
        self.states
            .insert(operation_id, OperationState::Released(live.stored));
        OperationCancel::Release {
            owner_request_id: live.owner_request_id,
            waiters,
        }
    }
}

/// Default in-memory operation store.
pub struct InMemoryOperationStore {
    inner: SyncMutex<OperationRegistry>,
}

impl InMemoryOperationStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Default for InMemoryOperationStore {
    fn default() -> Self {
        Self {
            inner: SyncMutex::new("driver.operations", OperationRegistry::default()),
        }
    }
}

impl OperationStore for InMemoryOperationStore {
    fn admit(
        &self,
        operation_id: u64,
        method_id: MethodId,
        args: &[u8],
        retry: RetryPolicy,
        request_id: RequestId,
    ) -> OperationAdmit {
        self.inner
            .lock()
            .admit(operation_id, method_id, args, retry, request_id)
    }

    fn seal(
        &self,
        operation_id: u64,
        owner_request_id: RequestId,
        encoded_response: Arc<[u8]>,
    ) -> Vec<RequestId> {
        self.inner
            .lock()
            .seal(operation_id, owner_request_id, encoded_response)
    }

    fn fail_without_reply(&self, operation_id: u64, owner_request_id: RequestId) -> Vec<RequestId> {
        self.inner
            .lock()
            .fail_without_reply(operation_id, owner_request_id)
    }

    fn cancel(&self, request_id: RequestId) -> OperationCancel {
        self.inner.lock().cancel(request_id)
    }
}
