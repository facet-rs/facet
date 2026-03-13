/// Wasm-only driver: no channels, no Semaphore, single-threaded.
///
/// On wasm32 the full `driver.rs` is not available (it depends on
/// `tokio::sync::Semaphore` and channel infrastructure that doesn't
/// compile for wasm). This module provides the same public API surface
/// (`Driver`, `DriverReplySink`, `DriverCaller`) but without channel support.
use std::{
    collections::BTreeMap,
    sync::{
        Arc, Weak,
        atomic::{AtomicU64, Ordering},
    },
};

use moire::sync::{SyncMutex, mpsc};
use roam_types::{
    Caller, Handler, IdAllocator, MaybeSend, Payload, ReplySink, RequestBody, RequestCall,
    RequestId, RequestMessage, RequestResponse, RoamError, SelfRef, ensure_operation_id,
    metadata_operation_id,
};
use tokio::sync::watch;

use crate::session::{
    ConnectionHandle, ConnectionMessage, ConnectionSender, DropControlRequest, FailureDisposition,
};

type ResponseSlot = moire::sync::oneshot::Sender<SelfRef<RequestMessage<'static>>>;

#[derive(Clone, PartialEq, Eq)]
struct OperationSignature {
    method_id: roam_types::MethodId,
    args: Arc<[u8]>,
}

impl OperationSignature {
    fn matches_call(&self, method_id: roam_types::MethodId, args: &[u8]) -> bool {
        self.method_id == method_id && self.args.as_ref() == args
    }
}

struct StoredOperation {
    signature: OperationSignature,
    retry: roam_types::RetryPolicy,
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

enum OperationAdmit {
    Start,
    Attached,
    Replay(Arc<[u8]>),
    Conflict,
    Indeterminate,
}

enum OperationCancel {
    None,
    DetachOnly,
    Release {
        owner_request_id: RequestId,
        waiters: Vec<RequestId>,
    },
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
        method_id: roam_types::MethodId,
        args: &[u8],
        retry: roam_types::RetryPolicy,
        request_id: RequestId,
    ) -> OperationAdmit {
        let signature = OperationSignature {
            method_id,
            args: Arc::<[u8]>::from(args.to_vec()),
        };
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
                let matches_call = stored.signature.matches_call(method_id, args);
                if !matches_call || !stored.retry.idem {
                    self.states
                        .insert(operation_id, OperationState::Released(stored));
                    return if matches_call {
                        OperationAdmit::Indeterminate
                    } else {
                        OperationAdmit::Conflict
                    };
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
                let matches_call = stored.signature.matches_call(method_id, args);
                if !matches_call || !stored.retry.idem {
                    self.states
                        .insert(operation_id, OperationState::Indeterminate(stored));
                    return if matches_call {
                        OperationAdmit::Indeterminate
                    } else {
                        OperationAdmit::Conflict
                    };
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
        let Some(OperationState::Live(live)) = self.states.remove(&operation_id) else {
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
        let Some(OperationState::Live(live)) = self.states.remove(&operation_id) else {
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

struct DriverShared {
    pending_responses: SyncMutex<BTreeMap<RequestId, ResponseSlot>>,
    request_ids: SyncMutex<IdAllocator<RequestId>>,
    next_operation_id: AtomicU64,
    operations: Arc<SyncMutex<OperationRegistry>>,
}

struct CallerDropGuard {
    control_tx: mpsc::UnboundedSender<DropControlRequest>,
    request: DropControlRequest,
}

impl Drop for CallerDropGuard {
    fn drop(&mut self) {
        let _ = self.control_tx.try_send(self.request);
    }
}

/// Concrete [`ReplySink`] for the wasm driver. No channel support.
pub struct DriverReplySink {
    sender: Option<ConnectionSender>,
    request_id: RequestId,
    retry: roam_types::RetryPolicy,
    operation_id: Option<u64>,
    operations: Option<Arc<SyncMutex<OperationRegistry>>>,
}

fn send_encoded_response(
    sender: ConnectionSender,
    request_id: RequestId,
    encoded_response: Arc<[u8]>,
) -> impl std::future::Future<Output = Result<(), ()>> {
    async move {
        let response: RequestResponse<'_> =
            facet_postcard::from_slice_borrowed(encoded_response.as_ref()).map_err(|_| ())?;
        sender.send_response(request_id, response).await
    }
}

impl ReplySink for DriverReplySink {
    async fn send_reply(mut self, response: RequestResponse<'_>) {
        let sender = self
            .sender
            .take()
            .expect("unreachable: send_reply takes self by value");
        if let (Some(operation_id), Some(operations)) = (self.operation_id, self.operations.take())
        {
            let encoded_response: Arc<[u8]> = facet_postcard::to_vec(&response)
                .expect("serialize operation response")
                .into();
            let waiters = operations.lock().seal(
                operation_id,
                self.request_id,
                Arc::clone(&encoded_response),
            );
            for waiter in waiters {
                if send_encoded_response(sender.clone(), waiter, Arc::clone(&encoded_response))
                    .await
                    .is_err()
                {
                    sender.mark_failure(waiter, FailureDisposition::Cancelled);
                }
            }
        } else if let Err(_e) = sender.send_response(self.request_id, response).await {
            sender.mark_failure(self.request_id, FailureDisposition::Cancelled);
        }
    }
}

impl Drop for DriverReplySink {
    fn drop(&mut self) {
        if let Some(sender) = self.sender.take() {
            if let (Some(operation_id), Some(operations)) =
                (self.operation_id, self.operations.take())
            {
                let waiters = operations
                    .lock()
                    .fail_without_reply(operation_id, self.request_id);
                let disposition = if self.retry.persist {
                    FailureDisposition::Indeterminate
                } else {
                    FailureDisposition::Cancelled
                };
                for waiter in waiters {
                    sender.mark_failure(waiter, disposition);
                }
            } else {
                let disposition = if self.retry.persist {
                    FailureDisposition::Indeterminate
                } else {
                    FailureDisposition::Cancelled
                };
                sender.mark_failure(self.request_id, disposition)
            }
        }
    }
}

/// Cloneable caller handle for the wasm driver.
#[derive(Clone)]
pub struct DriverCaller {
    sender: ConnectionSender,
    shared: Arc<DriverShared>,
    closed_rx: watch::Receiver<bool>,
    resumed_rx: watch::Receiver<u64>,
    peer_supports_retry: bool,
    _drop_guard: Option<Arc<CallerDropGuard>>,
}

impl Caller for DriverCaller {
    fn call<'a>(
        &'a self,
        mut call: RequestCall<'a>,
    ) -> impl std::future::Future<Output = Result<SelfRef<RequestResponse<'static>>, RoamError>>
    + MaybeSend
    + 'a {
        async move {
            if self.peer_supports_retry {
                let operation_id = self
                    .shared
                    .next_operation_id
                    .fetch_add(1, Ordering::Relaxed);
                ensure_operation_id(&mut call.metadata, operation_id);
            }

            let encoded_call: Arc<[u8]> = facet_postcard::to_vec(&call)
                .map_err(|_| RoamError::InvalidPayload)?
                .into();

            let req_id = self.shared.request_ids.lock().alloc();
            let (tx, rx) = moire::sync::oneshot::channel("driver.response");
            self.shared.pending_responses.lock().insert(req_id, tx);

            let resend_call: RequestCall<'_> =
                facet_postcard::from_slice_borrowed(encoded_call.as_ref())
                    .map_err(|_| RoamError::<core::convert::Infallible>::InvalidPayload)?;
            if self
                .sender
                .send(ConnectionMessage::Request(RequestMessage {
                    id: req_id,
                    body: RequestBody::Call(resend_call),
                }))
                .await
                .is_err()
            {
                self.shared.pending_responses.lock().remove(&req_id);
                return Err(RoamError::Cancelled);
            }

            let mut resumed_rx = self.resumed_rx.clone();
            let mut seen_resume_generation = *resumed_rx.borrow();
            let mut closed_rx = self.closed_rx.clone();
            let mut response = std::pin::pin!(rx);

            let response_msg: SelfRef<RequestMessage<'static>> = loop {
                tokio::select! {
                    result = &mut response => {
                        break result.map_err(|_| RoamError::Cancelled)?;
                    }
                    changed = resumed_rx.changed(), if self.peer_supports_retry => {
                        if changed.is_err() {
                            self.shared.pending_responses.lock().remove(&req_id);
                            return Err(RoamError::Cancelled);
                        }
                        let generation = *resumed_rx.borrow();
                        if generation == seen_resume_generation {
                            continue;
                        }
                        seen_resume_generation = generation;
                        let resend_call: Result<RequestCall<'_>, _> =
                            facet_postcard::from_slice_borrowed(encoded_call.as_ref());
                        if let Ok(resend_call) = resend_call {
                            let _ = self.sender.send(ConnectionMessage::Request(RequestMessage {
                                id: req_id,
                                body: RequestBody::Call(resend_call),
                            })).await;
                        }
                    }
                    changed = closed_rx.changed() => {
                        if changed.is_err() || *closed_rx.borrow() {
                            self.shared.pending_responses.lock().remove(&req_id);
                            return Err(RoamError::Cancelled);
                        }
                    }
                }
            };

            let response = response_msg.map(|m| match m.body {
                RequestBody::Response(r) => r,
                _ => unreachable!("pending_responses only gets Response variants"),
            });

            Ok(response)
        }
    }

    fn closed(&self) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + '_>> {
        Box::pin(async move {
            if *self.closed_rx.borrow() {
                return;
            }
            let mut rx = self.closed_rx.clone();
            while rx.changed().await.is_ok() {
                if *rx.borrow() {
                    return;
                }
            }
        })
    }

    fn is_connected(&self) -> bool {
        !*self.closed_rx.borrow()
    }
}

/// Liveness-only handle for a connection root.
///
/// Keeps the root connection alive but intentionally exposes no outbound RPC API.
#[must_use = "Dropping NoopCaller may close the connection if it is the last caller."]
#[derive(Clone)]
pub struct NoopCaller(#[allow(dead_code)] DriverCaller);

impl From<DriverCaller> for NoopCaller {
    fn from(caller: DriverCaller) -> Self {
        Self(caller)
    }
}

/// Wasm-only driver. No channel support.
struct InFlightHandler {
    handle: moire::task::JoinHandle<()>,
    retry: roam_types::RetryPolicy,
}

pub struct Driver<H: Handler<DriverReplySink>> {
    sender: ConnectionSender,
    rx: mpsc::Receiver<SelfRef<ConnectionMessage<'static>>>,
    failures_rx: mpsc::UnboundedReceiver<(RequestId, FailureDisposition)>,
    closed_rx: watch::Receiver<bool>,
    resumed_rx: watch::Receiver<u64>,
    peer_supports_retry: bool,
    handler: Arc<H>,
    shared: Arc<DriverShared>,
    in_flight_handlers: BTreeMap<RequestId, InFlightHandler>,
    drop_control_seed: Option<mpsc::UnboundedSender<DropControlRequest>>,
    drop_control_request: DropControlRequest,
    drop_guard: SyncMutex<Option<Weak<CallerDropGuard>>>,
}

impl<H: Handler<DriverReplySink>> Driver<H> {
    pub fn new(handle: ConnectionHandle, handler: H) -> Self {
        let conn_id = handle.connection_id();
        let ConnectionHandle {
            sender,
            rx,
            failures_rx,
            control_tx,
            closed_rx,
            resumed_rx,
            parity,
            peer_supports_retry,
        } = handle;
        let drop_control_request = DropControlRequest::Close(conn_id);
        Self {
            sender,
            rx,
            failures_rx,
            closed_rx,
            resumed_rx,
            handler: Arc::new(handler),
            shared: Arc::new(DriverShared {
                pending_responses: SyncMutex::new("driver.pending_responses", BTreeMap::new()),
                request_ids: SyncMutex::new("driver.request_ids", IdAllocator::new(parity)),
                next_operation_id: AtomicU64::new(1),
                operations: Arc::new(SyncMutex::new(
                    "driver.operations",
                    OperationRegistry::default(),
                )),
            }),
            peer_supports_retry,
            in_flight_handlers: BTreeMap::new(),
            drop_control_seed: control_tx,
            drop_control_request,
            drop_guard: SyncMutex::new("driver.drop_guard", None),
        }
    }

    // r[impl rpc.caller.liveness.refcounted]
    // r[impl rpc.caller.liveness.last-drop-closes-connection]
    // r[impl rpc.caller.liveness.root-internal-close]
    // r[impl rpc.caller.liveness.root-teardown-condition]
    pub fn caller(&self) -> DriverCaller {
        let drop_guard = if let Some(seed) = &self.drop_control_seed {
            let mut guard = self.drop_guard.lock();
            if let Some(existing) = guard.as_ref().and_then(Weak::upgrade) {
                Some(existing)
            } else {
                let arc = Arc::new(CallerDropGuard {
                    control_tx: seed.clone(),
                    request: self.drop_control_request,
                });
                *guard = Some(Arc::downgrade(&arc));
                Some(arc)
            }
        } else {
            None
        };
        DriverCaller {
            sender: self.sender.clone(),
            shared: Arc::clone(&self.shared),
            closed_rx: self.closed_rx.clone(),
            resumed_rx: self.resumed_rx.clone(),
            peer_supports_retry: self.peer_supports_retry,
            _drop_guard: drop_guard,
        }
    }

    pub async fn run(&mut self) {
        loop {
            tokio::select! {
                msg = self.rx.recv() => {
                    match msg {
                        Some(msg) => self.handle_msg(msg),
                        None => break,
                    }
                }
                Some((req_id, disposition)) = self.failures_rx.recv() => {
                    self.in_flight_handlers.remove(&req_id);
                    if self.shared.pending_responses.lock().remove(&req_id).is_none() {
                        let roam_error = match disposition {
                            FailureDisposition::Cancelled => RoamError::Cancelled,
                            FailureDisposition::Indeterminate => RoamError::Indeterminate,
                        };
                        let error: Result<(), RoamError<core::convert::Infallible>> =
                            Err(roam_error);
                        let _ = self.sender.send_response(req_id, RequestResponse {
                            ret: Payload::outgoing(&error),
                            channels: vec![],
                            metadata: Default::default(),
                        }).await;
                    }
                }
            }
        }
    }

    fn handle_msg(&mut self, msg: SelfRef<ConnectionMessage<'static>>) {
        let is_request = matches!(&*msg, ConnectionMessage::Request(_));
        if is_request {
            let msg = msg.map(|m| match m {
                ConnectionMessage::Request(r) => r,
                _ => unreachable!(),
            });
            self.handle_request(msg);
        }
        // Channel messages are ignored on wasm (no channel support).
    }

    fn handle_request(&mut self, msg: SelfRef<RequestMessage<'static>>) {
        let req_id = msg.id;
        let is_call = matches!(&msg.body, RequestBody::Call(_));
        let is_response = matches!(&msg.body, RequestBody::Response(_));
        let is_cancel = matches!(&msg.body, RequestBody::Cancel(_));

        if is_call {
            let call = msg.map(|m| match m.body {
                RequestBody::Call(c) => c,
                _ => unreachable!(),
            });
            let handler = Arc::clone(&self.handler);
            let retry = handler.retry_policy(call.method_id);
            let operation_id = metadata_operation_id(&call.metadata);
            if let Some(operation_id) = operation_id {
                let args = match &call.args {
                    Payload::Incoming(bytes) => *bytes,
                    Payload::Outgoing { .. } => {
                        panic!("incoming request payload should always be incoming bytes")
                    }
                };
                match self.shared.operations.lock().admit(
                    operation_id,
                    call.method_id,
                    args,
                    retry,
                    req_id,
                ) {
                    OperationAdmit::Attached => return,
                    OperationAdmit::Replay(encoded_response) => {
                        let sender = self.sender.clone();
                        moire::task::spawn(async move {
                            if send_encoded_response(sender.clone(), req_id, encoded_response)
                                .await
                                .is_err()
                            {
                                sender.mark_failure(req_id, FailureDisposition::Cancelled);
                            }
                        });
                        return;
                    }
                    OperationAdmit::Conflict => {
                        let sender = self.sender.clone();
                        moire::task::spawn(async move {
                            let error: Result<(), RoamError<core::convert::Infallible>> =
                                Err(RoamError::InvalidPayload);
                            let _ = sender
                                .send_response(
                                    req_id,
                                    RequestResponse {
                                        ret: Payload::outgoing(&error),
                                        channels: vec![],
                                        metadata: Default::default(),
                                    },
                                )
                                .await;
                        });
                        return;
                    }
                    OperationAdmit::Indeterminate => {
                        let sender = self.sender.clone();
                        moire::task::spawn(async move {
                            let error: Result<(), RoamError<core::convert::Infallible>> =
                                Err(RoamError::Indeterminate);
                            let _ = sender
                                .send_response(
                                    req_id,
                                    RequestResponse {
                                        ret: Payload::outgoing(&error),
                                        channels: vec![],
                                        metadata: Default::default(),
                                    },
                                )
                                .await;
                        });
                        return;
                    }
                    OperationAdmit::Start => {}
                }
            }
            let reply = DriverReplySink {
                sender: Some(self.sender.clone()),
                request_id: req_id,
                retry,
                operation_id,
                operations: operation_id.map(|_| Arc::clone(&self.shared.operations)),
            };
            let join_handle = moire::task::spawn(async move {
                handler.handle(call, reply).await;
            });
            self.in_flight_handlers.insert(
                req_id,
                InFlightHandler {
                    handle: join_handle,
                    retry,
                },
            );
        } else if is_response {
            if let Some(tx) = self.shared.pending_responses.lock().remove(&req_id) {
                let _: Result<(), _> = tx.send(msg);
            }
        } else if is_cancel {
            match self.shared.operations.lock().cancel(req_id) {
                OperationCancel::None => {
                    let should_abort = self
                        .in_flight_handlers
                        .get(&req_id)
                        .map(|in_flight| !in_flight.retry.persist)
                        .unwrap_or(false);
                    if should_abort {
                        if let Some(in_flight) = self.in_flight_handlers.remove(&req_id) {
                            in_flight.handle.abort();
                        }
                    }
                }
                OperationCancel::DetachOnly => {}
                OperationCancel::Release {
                    owner_request_id,
                    waiters,
                } => {
                    if let Some(in_flight) = self.in_flight_handlers.remove(&owner_request_id) {
                        in_flight.handle.abort();
                    }
                    for waiter in waiters {
                        self.sender
                            .mark_failure(waiter, FailureDisposition::Cancelled);
                    }
                }
            }
        }
    }
}
