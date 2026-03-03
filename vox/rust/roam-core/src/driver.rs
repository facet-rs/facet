use std::{
    collections::BTreeMap,
    pin::Pin,
    sync::{Arc, Weak},
};

use moire::sync::SyncMutex;
use tokio::sync::Semaphore;

use moire::task::FutureExt as _;
use roam_types::{
    Caller, ChannelBinder, ChannelBody, ChannelClose, ChannelId, ChannelItem, ChannelMessage,
    ChannelSink, CreditSink, Handler, IdAllocator, IncomingChannelMessage, Payload, ReplySink,
    RequestBody, RequestCall, RequestId, RequestMessage, RequestResponse, RoamError, SelfRef,
    TxError,
};

use crate::session::{ConnectionHandle, ConnectionMessage, ConnectionSender, DropControlRequest};
use moire::sync::mpsc;

type ResponseSlot = moire::sync::oneshot::Sender<SelfRef<RequestMessage<'static>>>;

/// State shared between the driver loop and any DriverCaller/DriverChannelSink handles.
struct DriverShared {
    pending_responses: SyncMutex<BTreeMap<RequestId, ResponseSlot>>,
    request_ids: SyncMutex<IdAllocator<RequestId>>,
    channel_ids: SyncMutex<IdAllocator<ChannelId>>,
    /// Registry mapping inbound channel IDs to the sender that feeds the Rx handle.
    channel_senders:
        SyncMutex<BTreeMap<ChannelId, tokio::sync::mpsc::Sender<IncomingChannelMessage>>>,
    /// Buffer for channel messages that arrive before the channel is registered.
    ///
    /// This handles the race between the caller sending items immediately after
    /// `bind_channels_caller_args` creates the sink, and the callee's handler task
    /// calling `register_rx` via `bind_channels_callee_args`. Items arriving in
    /// that window are buffered here and drained when the channel is registered.
    channel_buffers: SyncMutex<BTreeMap<ChannelId, Vec<IncomingChannelMessage>>>,
    /// Credit semaphores for outbound channels (Tx on our side).
    /// The driver's GrantCredit handler adds permits to these.
    channel_credits: SyncMutex<BTreeMap<ChannelId, Arc<Semaphore>>>,
}

struct CallerDropGuard {
    control_tx: mpsc::UnboundedSender<DropControlRequest>,
    request: DropControlRequest,
}

impl Drop for CallerDropGuard {
    fn drop(&mut self) {
        let _ = self.control_tx.send(self.request);
    }
}

/// Concrete `ReplySink` implementation for the driver.
///
/// If dropped without `send_reply` being called, automatically sends
/// `RoamError::Cancelled` to the caller. This guarantees that every
/// request receives exactly one response (`rpc.response.one-per-request`),
/// even if the handler panics or forgets to reply.
pub struct DriverReplySink {
    sender: Option<ConnectionSender>,
    request_id: RequestId,
    binder: DriverChannelBinder,
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

    fn channel_binder(&self) -> Option<&dyn ChannelBinder> {
        Some(&self.binder)
    }
}

// r[impl rpc.response.one-per-request]
impl Drop for DriverReplySink {
    fn drop(&mut self) {
        if let Some(sender) = self.sender.take() {
            sender.mark_failure(self.request_id, "no reply sent")
        }
    }
}

// r[impl rpc.channel.item]
// r[impl rpc.channel.close]
/// Concrete [`ChannelSink`] backed by a `ConnectionSender`.
///
/// Created by the driver when setting up outbound channels (Tx handles).
/// Sends `ChannelItem` and `ChannelClose` messages through the connection.
/// Wrapped with [`CreditSink`] to enforce credit-based flow control.
pub struct DriverChannelSink {
    sender: ConnectionSender,
    channel_id: ChannelId,
    local_control_tx: mpsc::UnboundedSender<DriverLocalControl>,
}

impl ChannelSink for DriverChannelSink {
    fn send_payload<'payload>(
        &self,
        payload: Payload<'payload>,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<(), TxError>> + Send + 'payload>> {
        let sender = self.sender.clone();
        let channel_id = self.channel_id;
        Box::pin(async move {
            sender
                .send(ConnectionMessage::Channel(ChannelMessage {
                    id: channel_id,
                    body: ChannelBody::Item(ChannelItem { item: payload }),
                }))
                .await
                .map_err(|()| TxError::Transport("connection closed".into()))
        })
    }

    fn close_channel(
        &self,
        _metadata: roam_types::Metadata,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<(), TxError>> + Send + 'static>> {
        // [FIXME] ChannelSink::close_channel takes borrowed Metadata but returns 'static future.
        // We drop the borrowed metadata and send an empty one. This matches the [FIXME] in the
        // trait definition — the signature needs to be fixed to take owned metadata.
        let sender = self.sender.clone();
        let channel_id = self.channel_id;
        Box::pin(async move {
            sender
                .send(ConnectionMessage::Channel(ChannelMessage {
                    id: channel_id,
                    body: ChannelBody::Close(ChannelClose {
                        metadata: Default::default(),
                    }),
                }))
                .await
                .map_err(|()| TxError::Transport("connection closed".into()))
        })
    }

    fn close_channel_on_drop(&self) {
        let _ = self
            .local_control_tx
            .send(DriverLocalControl::CloseChannel {
                channel_id: self.channel_id,
            });
    }
}

/// Cloneable handle for making outgoing calls through a connection.
///
impl From<DriverCaller> for () {
    fn from(_: DriverCaller) {}
}

#[derive(Clone)]
struct DriverChannelBinder {
    sender: ConnectionSender,
    shared: Arc<DriverShared>,
    local_control_tx: mpsc::UnboundedSender<DriverLocalControl>,
}

impl DriverChannelBinder {
    fn create_tx_channel(
        &self,
        initial_credit: u32,
    ) -> (ChannelId, Arc<CreditSink<DriverChannelSink>>) {
        let channel_id = self.shared.channel_ids.lock().alloc();
        let inner = DriverChannelSink {
            sender: self.sender.clone(),
            channel_id,
            local_control_tx: self.local_control_tx.clone(),
        };
        let sink = Arc::new(CreditSink::new(inner, initial_credit));
        self.shared
            .channel_credits
            .lock()
            .insert(channel_id, Arc::clone(sink.credit()));
        (channel_id, sink)
    }

    fn register_rx_channel(
        &self,
        channel_id: ChannelId,
    ) -> tokio::sync::mpsc::Receiver<IncomingChannelMessage> {
        let (tx, rx) = tokio::sync::mpsc::channel(64);
        let mut terminal_buffered = false;
        if let Some(buffered) = self.shared.channel_buffers.lock().remove(&channel_id) {
            for msg in buffered {
                let is_terminal = matches!(
                    msg,
                    IncomingChannelMessage::Close(_) | IncomingChannelMessage::Reset(_)
                );
                let _ = tx.try_send(msg);
                if is_terminal {
                    terminal_buffered = true;
                    break;
                }
            }
        }
        if terminal_buffered {
            self.shared.channel_credits.lock().remove(&channel_id);
            return rx;
        }

        self.shared.channel_senders.lock().insert(channel_id, tx);
        rx
    }
}

impl ChannelBinder for DriverChannelBinder {
    fn create_tx(&self, initial_credit: u32) -> (ChannelId, Arc<dyn ChannelSink>) {
        let (id, sink) = self.create_tx_channel(initial_credit);
        (id, sink as Arc<dyn ChannelSink>)
    }

    fn create_rx(
        &self,
    ) -> (
        ChannelId,
        tokio::sync::mpsc::Receiver<IncomingChannelMessage>,
    ) {
        let channel_id = self.shared.channel_ids.lock().alloc();
        let rx = self.register_rx_channel(channel_id);
        (channel_id, rx)
    }

    fn bind_tx(&self, channel_id: ChannelId, initial_credit: u32) -> Arc<dyn ChannelSink> {
        let inner = DriverChannelSink {
            sender: self.sender.clone(),
            channel_id,
            local_control_tx: self.local_control_tx.clone(),
        };
        let sink = Arc::new(CreditSink::new(inner, initial_credit));
        self.shared
            .channel_credits
            .lock()
            .insert(channel_id, Arc::clone(sink.credit()));
        sink
    }

    fn register_rx(
        &self,
        channel_id: ChannelId,
    ) -> tokio::sync::mpsc::Receiver<IncomingChannelMessage> {
        self.register_rx_channel(channel_id)
    }
}

/// Implements [`Caller`]: allocates a request ID, registers a response slot,
/// sends the call through the connection, and awaits the response.
#[derive(Clone)]
pub struct DriverCaller {
    sender: ConnectionSender,
    shared: Arc<DriverShared>,
    local_control_tx: mpsc::UnboundedSender<DriverLocalControl>,
    _drop_guard: Option<Arc<CallerDropGuard>>,
}

impl DriverCaller {
    /// Allocate a channel ID and create a credit-controlled sink for outbound items.
    ///
    /// `initial_credit` is the const generic `N` from `Tx<T, N>`.
    /// The returned sink enforces credit; the semaphore is registered so
    /// `GrantCredit` messages can add permits.
    pub fn create_tx_channel(
        &self,
        initial_credit: u32,
    ) -> (ChannelId, Arc<CreditSink<DriverChannelSink>>) {
        let channel_id = self.shared.channel_ids.lock().alloc();
        let inner = DriverChannelSink {
            sender: self.sender.clone(),
            channel_id,
            local_control_tx: self.local_control_tx.clone(),
        };
        let sink = Arc::new(CreditSink::new(inner, initial_credit));
        self.shared
            .channel_credits
            .lock()
            .insert(channel_id, Arc::clone(sink.credit()));
        (channel_id, sink)
    }

    /// Returns the underlying connection sender.
    ///
    /// Used by in-crate tests that need to inject raw messages for cancellation
    /// and channel protocol testing.
    #[cfg(test)]
    pub(crate) fn connection_sender(&self) -> &ConnectionSender {
        &self.sender
    }

    /// Register an inbound channel (Rx on our side) and return the receiver.
    ///
    /// The channel ID comes from the peer (e.g. from `RequestCall.channels`).
    /// The returned receiver should be bound to an `Rx` handle via `Rx::bind()`.
    pub fn register_rx_channel(
        &self,
        channel_id: ChannelId,
    ) -> tokio::sync::mpsc::Receiver<IncomingChannelMessage> {
        let (tx, rx) = tokio::sync::mpsc::channel(64);
        let mut terminal_buffered = false;
        // Drain any buffered messages that arrived before registration.
        if let Some(buffered) = self.shared.channel_buffers.lock().remove(&channel_id) {
            for msg in buffered {
                let is_terminal = matches!(
                    msg,
                    IncomingChannelMessage::Close(_) | IncomingChannelMessage::Reset(_)
                );
                let _ = tx.try_send(msg);
                if is_terminal {
                    terminal_buffered = true;
                    break;
                }
            }
        }
        if terminal_buffered {
            self.shared.channel_credits.lock().remove(&channel_id);
            return rx;
        }

        self.shared.channel_senders.lock().insert(channel_id, tx);
        rx
    }
}

impl ChannelBinder for DriverCaller {
    fn create_tx(&self, initial_credit: u32) -> (ChannelId, Arc<dyn ChannelSink>) {
        let (id, sink) = self.create_tx_channel(initial_credit);
        (id, sink as Arc<dyn ChannelSink>)
    }

    fn create_rx(
        &self,
    ) -> (
        ChannelId,
        tokio::sync::mpsc::Receiver<IncomingChannelMessage>,
    ) {
        let channel_id = self.shared.channel_ids.lock().alloc();
        let rx = self.register_rx_channel(channel_id);
        (channel_id, rx)
    }

    fn bind_tx(&self, channel_id: ChannelId, initial_credit: u32) -> Arc<dyn ChannelSink> {
        let inner = DriverChannelSink {
            sender: self.sender.clone(),
            channel_id,
            local_control_tx: self.local_control_tx.clone(),
        };
        let sink = Arc::new(CreditSink::new(inner, initial_credit));
        self.shared
            .channel_credits
            .lock()
            .insert(channel_id, Arc::clone(sink.credit()));
        sink
    }

    fn register_rx(
        &self,
        channel_id: ChannelId,
    ) -> tokio::sync::mpsc::Receiver<IncomingChannelMessage> {
        self.register_rx_channel(channel_id)
    }
}

impl Caller for DriverCaller {
    async fn call<'a>(
        &self,
        call: RequestCall<'a>,
    ) -> Result<SelfRef<RequestResponse<'static>>, RoamError> {
        async {
            // Allocate a request ID.
            let req_id = self.shared.request_ids.lock().alloc();

            // Register the response slot before sending, so the driver can
            // route the response even if it arrives before we start awaiting.
            let (tx, rx) = moire::sync::oneshot::channel("driver.response");
            self.shared.pending_responses.lock().insert(req_id, tx);

            // Send the call. This awaits the conduit permit and serializes
            // the borrowed payload all the way to the link's write buffer.
            let send_result = self
                .sender
                .send(ConnectionMessage::Request(RequestMessage {
                    id: req_id,
                    body: RequestBody::Call(call),
                }))
                .await;

            if send_result.is_err() {
                // Clean up the pending slot.
                self.shared.pending_responses.lock().remove(&req_id);
                return Err(RoamError::Cancelled);
            }

            // Await the response from the driver loop.
            let response_msg: SelfRef<RequestMessage<'static>> = rx
                .named("awaiting_response")
                .await
                .map_err(|_| RoamError::Cancelled)?;

            // Extract the Response variant from the RequestMessage.
            let response = response_msg.map(|m| match m.body {
                RequestBody::Response(r) => r,
                _ => unreachable!("pending_responses only gets Response variants"),
            });

            Ok(response)
        }
        .named("Caller::call")
        .await
    }

    fn channel_binder(&self) -> Option<&dyn ChannelBinder> {
        Some(self)
    }
}

// r[impl rpc.handler]
// r[impl rpc.request]
// r[impl rpc.response]
// r[impl rpc.pipelining]
/// Per-connection driver. Handles in-flight request tracking, dispatches
/// incoming calls to a Handler, and manages channel state/flow control.
pub struct Driver<H: Handler<DriverReplySink>> {
    sender: ConnectionSender,
    rx: mpsc::Receiver<SelfRef<ConnectionMessage<'static>>>,
    failures_rx: mpsc::UnboundedReceiver<(RequestId, &'static str)>,
    local_control_rx: mpsc::UnboundedReceiver<DriverLocalControl>,
    handler: Arc<H>,
    shared: Arc<DriverShared>,
    /// In-flight server-side handler tasks, keyed by request ID.
    /// Used to abort handlers on cancel.
    in_flight_handlers: BTreeMap<RequestId, moire::task::JoinHandle<()>>,
    local_control_tx: mpsc::UnboundedSender<DriverLocalControl>,
    drop_control_seed: Option<mpsc::UnboundedSender<DropControlRequest>>,
    drop_control_request: DropControlRequest,
    drop_guard: SyncMutex<Option<Weak<CallerDropGuard>>>,
}

enum DriverLocalControl {
    CloseChannel { channel_id: ChannelId },
}

impl<H: Handler<DriverReplySink>> Driver<H> {
    pub fn new(handle: ConnectionHandle, handler: H) -> Self {
        let conn_id = handle.connection_id();
        let ConnectionHandle {
            sender,
            rx,
            failures_rx,
            control_tx,
            parity,
        } = handle;
        let drop_control_request = DropControlRequest::Close(conn_id);
        let (local_control_tx, local_control_rx) = mpsc::unbounded_channel("driver.local_control");
        Self {
            sender,
            rx,
            failures_rx,
            local_control_rx,
            handler: Arc::new(handler),
            shared: Arc::new(DriverShared {
                pending_responses: SyncMutex::new("driver.pending_responses", BTreeMap::new()),
                request_ids: SyncMutex::new("driver.request_ids", IdAllocator::new(parity)),
                channel_ids: SyncMutex::new("driver.channel_ids", IdAllocator::new(parity)),
                channel_senders: SyncMutex::new("driver.channel_senders", BTreeMap::new()),
                channel_buffers: SyncMutex::new("driver.channel_buffers", BTreeMap::new()),
                channel_credits: SyncMutex::new("driver.channel_credits", BTreeMap::new()),
            }),
            in_flight_handlers: BTreeMap::new(),
            local_control_tx,
            drop_control_seed: control_tx,
            drop_control_request,
            drop_guard: SyncMutex::new("driver.drop_guard", None),
        }
    }

    /// Get a cloneable caller handle for making outgoing calls.
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
            local_control_tx: self.local_control_tx.clone(),
            _drop_guard: drop_guard,
        }
    }

    fn internal_binder(&self) -> DriverChannelBinder {
        DriverChannelBinder {
            sender: self.sender.clone(),
            shared: Arc::clone(&self.shared),
            local_control_tx: self.local_control_tx.clone(),
        }
    }

    // r[impl rpc.pipelining]
    /// Main loop: receive messages from the session and dispatch them.
    /// Handler calls run as spawned tasks — we don't block the driver
    /// loop waiting for a handler to finish.
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
                    // Clean up the handler tracking entry.
                    self.in_flight_handlers.remove(&req_id);
                    if self.shared.pending_responses.lock().remove(&req_id).is_none() {
                        // Incoming call — handler failed to reply.
                        // Wire format is always Result<T, RoamError<E>>, so encode
                        // Cancelled as Err(...) in that envelope.
                        let error: Result<(), RoamError<core::convert::Infallible>> =
                            Err(RoamError::Cancelled);
                        let _ = self.sender.send_response(req_id, RequestResponse {
                            ret: Payload::outgoing(&error),
                            channels: vec![],
                            metadata: Default::default(),
                        }).await;
                    }
                }
                Some(ctrl) = self.local_control_rx.recv() => {
                    self.handle_local_control(ctrl).await;
                }
            }
        }

        for (_, handle) in std::mem::take(&mut self.in_flight_handlers) {
            handle.abort();
        }
        self.shared.pending_responses.lock().clear();

        // Connection is gone: drop channel runtime state so any registered Rx
        // receivers observe closure instead of hanging on recv().
        self.shared.channel_senders.lock().clear();
        self.shared.channel_buffers.lock().clear();
        self.shared.channel_credits.lock().clear();
    }

    async fn handle_local_control(&mut self, control: DriverLocalControl) {
        match control {
            DriverLocalControl::CloseChannel { channel_id } => {
                let _ = self
                    .sender
                    .send(ConnectionMessage::Channel(ChannelMessage {
                        id: channel_id,
                        body: ChannelBody::Close(ChannelClose {
                            metadata: Default::default(),
                        }),
                    }))
                    .await;
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
        } else {
            let msg = msg.map(|m| match m {
                ConnectionMessage::Channel(c) => c,
                _ => unreachable!(),
            });
            self.handle_channel(msg);
        }
    }

    fn handle_request(&mut self, msg: SelfRef<RequestMessage<'static>>) {
        let req_id = msg.id;
        let is_call = matches!(&msg.body, RequestBody::Call(_));
        let is_response = matches!(&msg.body, RequestBody::Response(_));
        let is_cancel = matches!(&msg.body, RequestBody::Cancel(_));

        if is_call {
            // r[impl rpc.request]
            // r[impl rpc.error.scope]
            let reply = DriverReplySink {
                sender: Some(self.sender.clone()),
                request_id: req_id,
                binder: self.internal_binder(),
            };
            let call = msg.map(|m| match m.body {
                RequestBody::Call(c) => c,
                _ => unreachable!(),
            });
            let handler = Arc::clone(&self.handler);
            let join_handle = moire::task::spawn(
                async move {
                    handler.handle(call, reply).await;
                }
                .named("handler"),
            );
            self.in_flight_handlers.insert(req_id, join_handle);
        } else if is_response {
            // r[impl rpc.response.one-per-request]
            if let Some(tx) = self.shared.pending_responses.lock().remove(&req_id) {
                let _: Result<(), _> = tx.send(msg);
            }
        } else if is_cancel {
            // r[impl rpc.cancel]
            // r[impl rpc.cancel.channels]
            // Abort the in-flight handler task. Channels are intentionally left
            // intact — they have independent lifecycles per spec.
            if let Some(handle) = self.in_flight_handlers.remove(&req_id) {
                handle.abort();
            }
            // The response is sent automatically: aborting drops DriverReplySink →
            // mark_failure fires → failures_rx arm sends RoamError::Cancelled.
        }
    }

    fn handle_channel(&mut self, msg: SelfRef<ChannelMessage<'static>>) {
        let chan_id = msg.id;

        // Look up the channel sender from the shared registry (handles registered
        // by both the driver and any DriverCaller that set up channels).
        let sender = self.shared.channel_senders.lock().get(&chan_id).cloned();

        match &msg.body {
            // r[impl rpc.channel.item]
            ChannelBody::Item(_item) => {
                if let Some(tx) = &sender {
                    let item = msg.map(|m| match m.body {
                        ChannelBody::Item(item) => item,
                        _ => unreachable!(),
                    });
                    // try_send: if the Rx has been dropped or the buffer is full, drop the item.
                    let _ = tx.try_send(IncomingChannelMessage::Item(item));
                } else {
                    // Channel not yet registered — buffer until register_rx_channel is called.
                    let item = msg.map(|m| match m.body {
                        ChannelBody::Item(item) => item,
                        _ => unreachable!(),
                    });
                    self.shared
                        .channel_buffers
                        .lock()
                        .entry(chan_id)
                        .or_default()
                        .push(IncomingChannelMessage::Item(item));
                }
            }
            // r[impl rpc.channel.close]
            ChannelBody::Close(_close) => {
                if let Some(tx) = &sender {
                    let close = msg.map(|m| match m.body {
                        ChannelBody::Close(close) => close,
                        _ => unreachable!(),
                    });
                    let _ = tx.try_send(IncomingChannelMessage::Close(close));
                } else {
                    // Channel not yet registered — buffer the close.
                    let close = msg.map(|m| match m.body {
                        ChannelBody::Close(close) => close,
                        _ => unreachable!(),
                    });
                    self.shared
                        .channel_buffers
                        .lock()
                        .entry(chan_id)
                        .or_default()
                        .push(IncomingChannelMessage::Close(close));
                }
                self.shared.channel_senders.lock().remove(&chan_id);
                self.shared.channel_credits.lock().remove(&chan_id);
            }
            // r[impl rpc.channel.reset]
            ChannelBody::Reset(_reset) => {
                if let Some(tx) = &sender {
                    let reset = msg.map(|m| match m.body {
                        ChannelBody::Reset(reset) => reset,
                        _ => unreachable!(),
                    });
                    let _ = tx.try_send(IncomingChannelMessage::Reset(reset));
                } else {
                    // Channel not yet registered — buffer the reset.
                    let reset = msg.map(|m| match m.body {
                        ChannelBody::Reset(reset) => reset,
                        _ => unreachable!(),
                    });
                    self.shared
                        .channel_buffers
                        .lock()
                        .entry(chan_id)
                        .or_default()
                        .push(IncomingChannelMessage::Reset(reset));
                }
                self.shared.channel_senders.lock().remove(&chan_id);
                self.shared.channel_credits.lock().remove(&chan_id);
            }
            // r[impl rpc.flow-control.credit.grant]
            // r[impl rpc.flow-control.credit.grant.additive]
            ChannelBody::GrantCredit(grant) => {
                if let Some(semaphore) = self.shared.channel_credits.lock().get(&chan_id) {
                    semaphore.add_permits(grant.additional as usize);
                }
            }
        }
    }
}
