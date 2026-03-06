/// Wasm-only driver: no channels, no Semaphore, single-threaded.
///
/// On wasm32 the full `driver.rs` is not available (it depends on
/// `tokio::sync::Semaphore` and channel infrastructure that doesn't
/// compile for wasm). This module provides the same public API surface
/// (`Driver`, `DriverReplySink`, `DriverCaller`) but without channel support.
use std::{
    collections::BTreeMap,
    sync::{Arc, Weak},
};

use moire::sync::{SyncMutex, mpsc};
use roam_types::{
    Caller, Handler, IdAllocator, MaybeSend, Payload, ReplySink, RequestBody, RequestCall,
    RequestId, RequestMessage, RequestResponse, RoamError, SelfRef,
};
use tokio::sync::watch;

use crate::session::{ConnectionHandle, ConnectionMessage, ConnectionSender, DropControlRequest};

type ResponseSlot = moire::sync::oneshot::Sender<SelfRef<RequestMessage<'static>>>;

struct DriverShared {
    pending_responses: SyncMutex<BTreeMap<RequestId, ResponseSlot>>,
    request_ids: SyncMutex<IdAllocator<RequestId>>,
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
}

impl ReplySink for DriverReplySink {
    async fn send_reply(mut self, response: RequestResponse<'_>) {
        let sender = self
            .sender
            .take()
            .expect("unreachable: send_reply takes self by value");
        if let Err(_e) = sender.send_response(self.request_id, response).await {
            sender.mark_failure(self.request_id, "send_response failed");
        }
    }
}

impl Drop for DriverReplySink {
    fn drop(&mut self) {
        if let Some(sender) = self.sender.take() {
            sender.mark_failure(self.request_id, "no reply sent")
        }
    }
}

/// Cloneable caller handle for the wasm driver.
#[derive(Clone)]
pub struct DriverCaller {
    sender: ConnectionSender,
    shared: Arc<DriverShared>,
    closed_rx: watch::Receiver<bool>,
    _drop_guard: Option<Arc<CallerDropGuard>>,
}

impl Caller for DriverCaller {
    fn call<'a>(
        &'a self,
        call: RequestCall<'a>,
    ) -> impl std::future::Future<Output = Result<SelfRef<RequestResponse<'static>>, RoamError>>
    + MaybeSend
    + 'a {
        async move {
            let req_id = self.shared.request_ids.lock().alloc();
            let (tx, rx) = moire::sync::oneshot::channel("driver.response");
            self.shared.pending_responses.lock().insert(req_id, tx);

            let send_result = self
                .sender
                .send(ConnectionMessage::Request(RequestMessage {
                    id: req_id,
                    body: RequestBody::Call(call),
                }))
                .await;

            if send_result.is_err() {
                self.shared.pending_responses.lock().remove(&req_id);
                return Err(RoamError::Cancelled);
            }

            let response_msg: SelfRef<RequestMessage<'static>> =
                rx.await.map_err(|_| RoamError::Cancelled)?;

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
pub struct Driver<H: Handler<DriverReplySink>> {
    sender: ConnectionSender,
    rx: mpsc::Receiver<SelfRef<ConnectionMessage<'static>>>,
    failures_rx: mpsc::UnboundedReceiver<(RequestId, &'static str)>,
    closed_rx: watch::Receiver<bool>,
    handler: Arc<H>,
    shared: Arc<DriverShared>,
    in_flight_handlers: BTreeMap<RequestId, moire::task::JoinHandle<()>>,
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
            parity,
        } = handle;
        let drop_control_request = DropControlRequest::Close(conn_id);
        Self {
            sender,
            rx,
            failures_rx,
            closed_rx,
            handler: Arc::new(handler),
            shared: Arc::new(DriverShared {
                pending_responses: SyncMutex::new("driver.pending_responses", BTreeMap::new()),
                request_ids: SyncMutex::new("driver.request_ids", IdAllocator::new(parity)),
            }),
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
                Some((req_id, _reason)) = self.failures_rx.recv() => {
                    self.in_flight_handlers.remove(&req_id);
                    if self.shared.pending_responses.lock().remove(&req_id).is_none() {
                        let error: Result<(), RoamError<core::convert::Infallible>> =
                            Err(RoamError::Cancelled);
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
            let reply = DriverReplySink {
                sender: Some(self.sender.clone()),
                request_id: req_id,
            };
            let call = msg.map(|m| match m.body {
                RequestBody::Call(c) => c,
                _ => unreachable!(),
            });
            let handler = Arc::clone(&self.handler);
            let join_handle = moire::task::spawn(async move {
                handler.handle(call, reply).await;
            });
            self.in_flight_handlers.insert(req_id, join_handle);
        } else if is_response {
            if let Some(tx) = self.shared.pending_responses.lock().remove(&req_id) {
                let _: Result<(), _> = tx.send(msg);
            }
        } else if is_cancel {
            if let Some(handle) = self.in_flight_handlers.remove(&req_id) {
                handle.abort();
            }
        }
    }
}
