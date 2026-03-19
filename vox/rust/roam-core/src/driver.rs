use std::{
    collections::BTreeMap,
    pin::Pin,
    sync::{
        Arc, Weak,
        atomic::{AtomicU64, Ordering},
    },
};

use moire::sync::SyncMutex;
use tokio::sync::{Semaphore, watch};

use moire::task::FutureExt as _;
use roam_types::{
    BoxFut, CallResult, Caller, ChannelBinder, ChannelBody, ChannelClose, ChannelCreditReplenisher,
    ChannelCreditReplenisherHandle, ChannelId, ChannelItem, ChannelLivenessHandle, ChannelMessage,
    ChannelSink, CreditSink, Handler, IdAllocator, IncomingChannelMessage, Payload, ReplySink,
    RequestBody, RequestCall, RequestId, RequestMessage, RequestResponse, RoamError, RpcPlan,
    SelfRef, TxError, ensure_operation_id, finalize_channels_caller_args, metadata_operation_id,
};

use crate::session::{
    ConnectionHandle, ConnectionMessage, ConnectionSender, DropControlRequest, FailureDisposition,
};
use crate::{InMemoryOperationStore, OperationAdmit, OperationCancel, OperationStore};
use moire::sync::mpsc;

/// A pending response for one outbound request attempt.
///
/// Carries both the wire response message and the recv tracker that was
/// current when the response was received, so the caller can deserialize
/// the response with the correct schemas.
struct PendingResponse {
    msg: SelfRef<RequestMessage<'static>>,
    schemas: Arc<roam_types::SchemaRecvTracker>,
}

type ResponseSlot = moire::sync::oneshot::Sender<PendingResponse>;

struct InFlightHandler {
    handle: moire::task::JoinHandle<()>,
    retry: roam_types::RetryPolicy,
    has_channels: bool,
    operation_id: Option<u64>,
}

/// State shared between the driver loop and any `DriverCaller` / `DriverChannelSink` handles.
///
/// `pending_responses` is keyed by request ID and therefore tracks live
/// request attempts, not logical operations.
struct DriverShared {
    pending_responses: SyncMutex<BTreeMap<RequestId, ResponseSlot>>,
    request_ids: SyncMutex<IdAllocator<RequestId>>,
    next_operation_id: AtomicU64,
    operations: Arc<dyn OperationStore>,
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

fn payload_is_runtime_error(payload: &Payload<'_>) -> bool {
    matches!(payload, Payload::Incoming(bytes) if bytes.len() >= 2 && bytes[0] == 1 && bytes[1] != 0)
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

#[cfg(test)]
mod tests {
    use super::{DriverChannelCreditReplenisher, DriverLocalControl};
    use roam_types::{ChannelCreditReplenisher, ChannelId};
    use tokio::sync::mpsc::error::TryRecvError;

    #[test]
    fn replenisher_batches_at_half_the_initial_window() {
        let (tx, mut rx) = moire::sync::mpsc::unbounded_channel("test.replenisher");
        let replenisher = DriverChannelCreditReplenisher::new(ChannelId(7), 16, tx);

        for _ in 0..7 {
            replenisher.on_item_consumed();
        }
        assert!(
            matches!(rx.try_recv(), Err(TryRecvError::Empty)),
            "should not emit credit before reaching the batch threshold"
        );

        replenisher.on_item_consumed();
        let Ok(DriverLocalControl::GrantCredit {
            channel_id,
            additional,
        }) = rx.try_recv()
        else {
            panic!("expected batched credit grant");
        };
        assert_eq!(channel_id, ChannelId(7));
        assert_eq!(additional, 8);
    }

    #[test]
    fn replenisher_grants_one_by_one_for_single_credit_windows() {
        let (tx, mut rx) = moire::sync::mpsc::unbounded_channel("test.replenisher.single");
        let replenisher = DriverChannelCreditReplenisher::new(ChannelId(9), 1, tx);

        replenisher.on_item_consumed();
        let Ok(DriverLocalControl::GrantCredit {
            channel_id,
            additional,
        }) = rx.try_recv()
        else {
            panic!("expected immediate credit grant");
        };
        assert_eq!(channel_id, ChannelId(9));
        assert_eq!(additional, 1);
    }
}

/// Concrete `ReplySink` implementation for the driver.
///
/// If dropped without `send_reply` being called, automatically sends
/// `RoamError::Cancelled` to the caller. This guarantees that every
/// request attempt receives exactly one terminal response
/// (`rpc.response.one-per-request`), even if the handler panics or
/// forgets to reply.
pub struct DriverReplySink {
    sender: Option<ConnectionSender>,
    request_id: RequestId,
    method_id: roam_types::MethodId,
    retry: roam_types::RetryPolicy,
    operation_id: Option<u64>,
    operations: Option<Arc<dyn OperationStore>>,
    binder: DriverChannelBinder,
}

async fn send_encoded_response(
    sender: ConnectionSender,
    request_id: RequestId,
    method_id: roam_types::MethodId,
    encoded_response: Arc<[u8]>,
) -> Result<(), ()> {
    let response: RequestResponse<'_> =
        roam_postcard::from_slice_borrowed(encoded_response.as_ref()).map_err(|_| ())?;
    sender
        .send_response_for_method(request_id, method_id, response)
        .await
}

fn incoming_args_bytes<'a>(call: &'a RequestCall<'a>) -> &'a [u8] {
    match &call.args {
        Payload::Incoming(bytes) => bytes,
        Payload::Outgoing { .. } => {
            panic!("incoming request payload should always be decoded as incoming bytes")
        }
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
            let mut response = response;
            sender.prepare_response_for_method(self.request_id, self.method_id, &mut response);
            let encoded_response: Arc<[u8]> = roam_postcard::to_vec(&response)
                .expect("serialize operation response")
                .into();
            if let Err(_e) = sender.send_response(self.request_id, response).await {
                sender.mark_failure(self.request_id, FailureDisposition::Cancelled);
            }
            let waiters =
                operations.seal(operation_id, self.request_id, Arc::clone(&encoded_response));
            for waiter in waiters {
                if waiter == self.request_id {
                    continue;
                }
                if send_encoded_response(
                    sender.clone(),
                    waiter,
                    self.method_id,
                    Arc::clone(&encoded_response),
                )
                .await
                .is_err()
                {
                    sender.mark_failure(waiter, FailureDisposition::Cancelled);
                }
            }
        } else if let Err(_e) = sender
            .send_response_for_method(self.request_id, self.method_id, response)
            .await
        {
            sender.mark_failure(self.request_id, FailureDisposition::Cancelled);
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
            if let (Some(operation_id), Some(operations)) =
                (self.operation_id, self.operations.take())
            {
                let waiters = operations.fail_without_reply(operation_id, self.request_id);
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
                sender.mark_failure(self.request_id, disposition);
            }
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

#[derive(Clone)]
struct DriverChannelBinder {
    sender: ConnectionSender,
    shared: Arc<DriverShared>,
    local_control_tx: mpsc::UnboundedSender<DriverLocalControl>,
    drop_guard: Option<Arc<CallerDropGuard>>,
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
        initial_credit: u32,
    ) -> roam_types::BoundChannelReceiver {
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
            return roam_types::BoundChannelReceiver {
                receiver: rx,
                liveness: self.channel_liveness(),
                replenisher: None,
            };
        }

        self.shared.channel_senders.lock().insert(channel_id, tx);
        roam_types::BoundChannelReceiver {
            receiver: rx,
            liveness: self.channel_liveness(),
            replenisher: Some(Arc::new(DriverChannelCreditReplenisher::new(
                channel_id,
                initial_credit,
                self.local_control_tx.clone(),
            )) as ChannelCreditReplenisherHandle),
        }
    }
}

impl ChannelBinder for DriverChannelBinder {
    fn create_tx(&self, initial_credit: u32) -> (ChannelId, Arc<dyn ChannelSink>) {
        let (id, sink) = self.create_tx_channel(initial_credit);
        (id, sink as Arc<dyn ChannelSink>)
    }

    fn create_rx(&self, initial_credit: u32) -> (ChannelId, roam_types::BoundChannelReceiver) {
        let channel_id = self.shared.channel_ids.lock().alloc();
        let rx = self.register_rx_channel(channel_id, initial_credit);
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
        initial_credit: u32,
    ) -> roam_types::BoundChannelReceiver {
        self.register_rx_channel(channel_id, initial_credit)
    }

    fn channel_liveness(&self) -> Option<ChannelLivenessHandle> {
        self.drop_guard
            .as_ref()
            .map(|guard| guard.clone() as ChannelLivenessHandle)
    }
}

/// Implements [`Caller`]: allocates a request ID, registers a response slot,
/// sends one request attempt through the connection, and awaits the
/// corresponding response.
#[derive(Clone)]
pub struct DriverCaller {
    sender: ConnectionSender,
    shared: Arc<DriverShared>,
    local_control_tx: mpsc::UnboundedSender<DriverLocalControl>,
    closed_rx: watch::Receiver<bool>,
    resumed_rx: watch::Receiver<u64>,
    resume_processed_rx: watch::Receiver<u64>,
    peer_supports_retry: bool,
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
        initial_credit: u32,
    ) -> roam_types::BoundChannelReceiver {
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
            return roam_types::BoundChannelReceiver {
                receiver: rx,
                liveness: self.channel_liveness(),
                replenisher: None,
            };
        }

        self.shared.channel_senders.lock().insert(channel_id, tx);
        roam_types::BoundChannelReceiver {
            receiver: rx,
            liveness: self.channel_liveness(),
            replenisher: Some(Arc::new(DriverChannelCreditReplenisher::new(
                channel_id,
                initial_credit,
                self.local_control_tx.clone(),
            )) as ChannelCreditReplenisherHandle),
        }
    }
}

impl ChannelBinder for DriverCaller {
    fn create_tx(&self, initial_credit: u32) -> (ChannelId, Arc<dyn ChannelSink>) {
        let (id, sink) = self.create_tx_channel(initial_credit);
        (id, sink as Arc<dyn ChannelSink>)
    }

    fn create_rx(&self, initial_credit: u32) -> (ChannelId, roam_types::BoundChannelReceiver) {
        let channel_id = self.shared.channel_ids.lock().alloc();
        let rx = self.register_rx_channel(channel_id, initial_credit);
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
        initial_credit: u32,
    ) -> roam_types::BoundChannelReceiver {
        self.register_rx_channel(channel_id, initial_credit)
    }

    fn channel_liveness(&self) -> Option<ChannelLivenessHandle> {
        self._drop_guard
            .as_ref()
            .map(|guard| guard.clone() as ChannelLivenessHandle)
    }
}

impl Caller for DriverCaller {
    async fn call<'a>(&'a self, mut call: RequestCall<'a>) -> CallResult {
        let caller_channel_plan = match &call.args {
            Payload::Outgoing { ptr, shape, .. } => {
                let plan = RpcPlan::for_shape(shape);
                (!plan.channel_locations.is_empty()).then_some((ptr.raw_ptr() as usize, plan))
            }
            Payload::Incoming(_) => None,
        };

        if self.peer_supports_retry {
            let operation_id = self
                .shared
                .next_operation_id
                .fetch_add(1, Ordering::Relaxed);
            ensure_operation_id(&mut call.metadata, operation_id);
        }

        // Allocate a request ID.
        let req_id = self.shared.request_ids.lock().alloc();

        // Register the response slot before sending, so the driver can
        // route the response even if it arrives before we start awaiting.
        let (tx, rx) = moire::sync::oneshot::channel("driver.response");
        self.shared.pending_responses.lock().insert(req_id, tx);

        // r[impl schema.exchange.caller]
        // r[impl schema.exchange.channels]
        // Schemas are attached by SessionCore::send() when it sees a Call
        // with Payload::Outgoing — no separate prepare step needed.
        if self
            .sender
            .send(ConnectionMessage::Request(RequestMessage {
                id: req_id,
                body: RequestBody::Call(RequestCall {
                    method_id: call.method_id,
                    args: call.args.reborrow(),
                    channels: call.channels.clone(),
                    metadata: call.metadata.clone(),
                    schemas: Default::default(),
                }),
            }))
            .await
            .is_err()
        {
            self.shared.pending_responses.lock().remove(&req_id);
            if let Some((args_ptr, plan)) = caller_channel_plan {
                unsafe { finalize_channels_caller_args(args_ptr as *mut u8, plan) };
            }
            return Err(RoamError::SendFailed);
        }

        let mut resumed_rx = self.resumed_rx.clone();
        let mut seen_resume_generation = *resumed_rx.borrow();
        let mut resume_processed_rx = self.resume_processed_rx.clone();
        let mut closed_rx = self.closed_rx.clone();
        let mut response = std::pin::pin!(rx.named("awaiting_response"));

        let pending: PendingResponse = loop {
            tokio::select! {
                result = &mut response => {
                    match result {
                        Ok(pending) => break pending,
                        Err(_) => {
                            if let Some((args_ptr, plan)) = caller_channel_plan {
                                unsafe { finalize_channels_caller_args(args_ptr as *mut u8, plan) };
                            }
                            return Err(RoamError::ConnectionClosed);
                        }
                    }
                }
                changed = resumed_rx.changed(), if self.peer_supports_retry => {
                    roam_types::dlog!("[CALLER] resumed_rx fired");
                    if changed.is_err() {
                        self.shared.pending_responses.lock().remove(&req_id);
                        if let Some((args_ptr, plan)) = caller_channel_plan {
                            unsafe { finalize_channels_caller_args(args_ptr as *mut u8, plan) };
                        }
                        return Err(RoamError::SessionShutdown);
                    }
                    let generation = *resumed_rx.borrow();
                    if generation == seen_resume_generation {
                        continue;
                    }
                    seen_resume_generation = generation;
                    while *resume_processed_rx.borrow() < generation {
                        if resume_processed_rx.changed().await.is_err() {
                            self.shared.pending_responses.lock().remove(&req_id);
                            if let Some((args_ptr, plan)) = caller_channel_plan {
                                unsafe { finalize_channels_caller_args(args_ptr as *mut u8, plan) };
                            }
                            return Err(RoamError::SessionShutdown);
                        }
                    }
                    if let Some((args_ptr, plan)) = caller_channel_plan
                        && let Some(binder) = self.channel_binder()
                    {
                        let channels = unsafe {
                            roam_types::bind_channels_caller_args(
                                args_ptr as *mut u8,
                                plan,
                                binder,
                            )
                        };
                        call.channels = channels;
                    }
                    let _ = self.sender.send(ConnectionMessage::Request(RequestMessage {
                        id: req_id,
                        body: RequestBody::Call(RequestCall {
                            method_id: call.method_id,
                            args: call.args.reborrow(),
                            channels: call.channels.clone(),
                            metadata: call.metadata.clone(),
                            schemas: Default::default(),
                        }),
                    })).await;
                }
                changed = closed_rx.changed() => {
                    roam_types::dlog!("[CALLER] closed_rx fired, value={}", *closed_rx.borrow());
                    if changed.is_err() || *closed_rx.borrow() {
                        self.shared.pending_responses.lock().remove(&req_id);
                        if let Some((args_ptr, plan)) = caller_channel_plan {
                            unsafe { finalize_channels_caller_args(args_ptr as *mut u8, plan) };
                        }
                        return Err(RoamError::ConnectionClosed);
                    }
                }
            }
        };

        // Extract the Response variant from the RequestMessage.
        let PendingResponse {
            msg: response_msg,
            schemas: response_schemas,
        } = pending;
        let response = response_msg.map(|m| match m.body {
            RequestBody::Response(r) => r,
            _ => unreachable!("pending_responses only gets Response variants"),
        });

        if let Some((args_ptr, plan)) = caller_channel_plan
            && payload_is_runtime_error(&response.ret)
        {
            unsafe { finalize_channels_caller_args(args_ptr as *mut u8, plan) };
        }
        Ok(roam_types::WithTracker {
            value: response,
            tracker: response_schemas,
        })
    }

    fn closed(&self) -> BoxFut<'_, ()> {
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

    fn channel_binder(&self) -> Option<&dyn ChannelBinder> {
        Some(self)
    }
}

// r[impl rpc.handler]
// r[impl rpc.request]
// r[impl rpc.response]
// r[impl rpc.pipelining]
/// Per-connection driver. Tracks in-flight request attempts, dispatches
/// incoming requests to a `Handler`, and manages channel state / flow control.
pub struct Driver<H: Handler<DriverReplySink>> {
    sender: ConnectionSender,
    rx: mpsc::Receiver<crate::session::RecvMessage>,
    failures_rx: mpsc::UnboundedReceiver<(RequestId, FailureDisposition)>,
    closed_rx: watch::Receiver<bool>,
    resumed_rx: watch::Receiver<u64>,
    resume_processed_tx: watch::Sender<u64>,
    peer_supports_retry: bool,
    local_control_rx: mpsc::UnboundedReceiver<DriverLocalControl>,
    handler: Arc<H>,
    shared: Arc<DriverShared>,
    /// In-flight server-side handler tasks, keyed by request ID.
    /// Used to abort handlers on cancel.
    in_flight_handlers: BTreeMap<RequestId, InFlightHandler>,
    local_control_tx: mpsc::UnboundedSender<DriverLocalControl>,
    drop_control_seed: Option<mpsc::UnboundedSender<DropControlRequest>>,
    drop_control_request: DropControlRequest,
    drop_guard: SyncMutex<Option<Weak<CallerDropGuard>>>,
}

enum DriverLocalControl {
    CloseChannel {
        channel_id: ChannelId,
    },
    GrantCredit {
        channel_id: ChannelId,
        additional: u32,
    },
}

struct DriverChannelCreditReplenisher {
    channel_id: ChannelId,
    threshold: u32,
    local_control_tx: mpsc::UnboundedSender<DriverLocalControl>,
    pending: std::sync::Mutex<u32>,
}

impl DriverChannelCreditReplenisher {
    fn new(
        channel_id: ChannelId,
        initial_credit: u32,
        local_control_tx: mpsc::UnboundedSender<DriverLocalControl>,
    ) -> Self {
        Self {
            channel_id,
            threshold: (initial_credit / 2).max(1),
            local_control_tx,
            pending: std::sync::Mutex::new(0),
        }
    }
}

impl ChannelCreditReplenisher for DriverChannelCreditReplenisher {
    fn on_item_consumed(&self) {
        let mut pending = self.pending.lock().expect("pending credit mutex poisoned");
        *pending += 1;
        if *pending < self.threshold {
            return;
        }

        let additional = *pending;
        *pending = 0;
        let _ = self.local_control_tx.send(DriverLocalControl::GrantCredit {
            channel_id: self.channel_id,
            additional,
        });
    }
}

impl<H: Handler<DriverReplySink>> Driver<H> {
    fn close_all_channel_runtime_state(&self) {
        for semaphore in self.shared.channel_credits.lock().values() {
            semaphore.close();
        }
        self.shared.channel_senders.lock().clear();
        self.shared.channel_buffers.lock().clear();
        self.shared.channel_credits.lock().clear();
    }

    fn close_outbound_channel(&self, channel_id: ChannelId) {
        if let Some(semaphore) = self.shared.channel_credits.lock().remove(&channel_id) {
            semaphore.close();
        }
    }

    fn abort_channel_handlers(&mut self) {
        for (req_id, in_flight) in &self.in_flight_handlers {
            if in_flight.has_channels {
                if let Some(operation_id) = in_flight.operation_id {
                    let _ = self
                        .shared
                        .operations
                        .fail_without_reply(operation_id, *req_id);
                }
                in_flight.handle.abort();
            }
        }
    }

    pub fn new(handle: ConnectionHandle, handler: H) -> Self {
        Self::with_operation_store(handle, handler, Arc::new(InMemoryOperationStore::default()))
    }

    pub fn with_operation_store(
        handle: ConnectionHandle,
        handler: H,
        operation_store: Arc<dyn OperationStore>,
    ) -> Self {
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
        let (local_control_tx, local_control_rx) = mpsc::unbounded_channel("driver.local_control");
        let (resume_processed_tx, _resume_processed_rx) = watch::channel(0_u64);
        Self {
            sender,
            rx,
            failures_rx,
            closed_rx,
            resumed_rx,
            resume_processed_tx,
            peer_supports_retry,
            local_control_rx,
            handler: Arc::new(handler),
            shared: Arc::new(DriverShared {
                pending_responses: SyncMutex::new("driver.pending_responses", BTreeMap::new()),
                request_ids: SyncMutex::new("driver.request_ids", IdAllocator::new(parity)),
                next_operation_id: AtomicU64::new(1),
                operations: operation_store,
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
    fn existing_drop_guard(&self) -> Option<Arc<CallerDropGuard>> {
        self.drop_guard.lock().as_ref().and_then(Weak::upgrade)
    }

    fn connection_drop_guard(&self) -> Option<Arc<CallerDropGuard>> {
        if let Some(existing) = self.existing_drop_guard() {
            Some(existing)
        } else if let Some(seed) = &self.drop_control_seed {
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
        }
    }

    pub fn caller(&self) -> DriverCaller {
        let drop_guard = self.connection_drop_guard();
        DriverCaller {
            sender: self.sender.clone(),
            shared: Arc::clone(&self.shared),
            local_control_tx: self.local_control_tx.clone(),
            closed_rx: self.closed_rx.clone(),
            resumed_rx: self.resumed_rx.clone(),
            resume_processed_rx: self.resume_processed_tx.subscribe(),
            peer_supports_retry: self.peer_supports_retry,
            _drop_guard: drop_guard,
        }
    }

    fn internal_binder(&self) -> DriverChannelBinder {
        DriverChannelBinder {
            sender: self.sender.clone(),
            shared: Arc::clone(&self.shared),
            local_control_tx: self.local_control_tx.clone(),
            drop_guard: self.existing_drop_guard(),
        }
    }

    // r[impl rpc.pipelining]
    /// Main loop: receive messages from the session and dispatch them.
    /// Handler calls run as spawned tasks — we don't block the driver
    /// loop waiting for a handler to finish.
    pub async fn run(&mut self) {
        let mut resumed_rx = self.resumed_rx.clone();
        let mut seen_resume_generation = *resumed_rx.borrow();
        loop {
            tracing::trace!("driver select loop top");
            tokio::select! {
                biased;
                changed = resumed_rx.changed() => {
                    if changed.is_err() {
                        break;
                    }
                    let generation = *resumed_rx.borrow();
                    if generation != seen_resume_generation {
                        seen_resume_generation = generation;
                        self.close_all_channel_runtime_state();
                        self.abort_channel_handlers();
                        let _ = self.resume_processed_tx.send(generation);
                    }
                }
                recv = self.rx.recv() => {
                    match recv {
                        Some(recv) => {
                            tracing::debug!("driver rx received message");
                            self.handle_recv(recv);
                        }
                        None => {
                            tracing::debug!("driver rx closed, exiting loop");
                            break;
                        }
                    }
                }
                Some((req_id, disposition)) = self.failures_rx.recv() => {
                    tracing::debug!(%req_id, ?disposition, "failures_rx fired");
                    let in_flight_found = self.in_flight_handlers.contains_key(&req_id);
                    let reply_disposition = self
                        .in_flight_handlers
                        .get(&req_id)
                        .map(|in_flight| {
                            if in_flight.has_channels && !in_flight.retry.idem {
                                Some(FailureDisposition::Indeterminate)
                            } else if in_flight.has_channels && in_flight.retry.idem {
                                None
                            } else {
                                Some(disposition)
                            }
                        })
                        .unwrap_or(Some(disposition));
                    tracing::debug!(%req_id, in_flight_found, ?reply_disposition, "failures_rx computed disposition");
                    // Clean up the handler tracking entry.
                    self.in_flight_handlers.remove(&req_id);
                    let had_pending = self.shared.pending_responses.lock().remove(&req_id).is_some();
                    tracing::debug!(%req_id, had_pending, "failures_rx checked pending_responses");
                    if !had_pending {
                        let Some(reply_disposition) = reply_disposition else {
                            tracing::debug!(%req_id, "failures_rx: no reply_disposition, skipping");
                            continue;
                        };
                        tracing::debug!(%req_id, ?reply_disposition, "failures_rx: sending error response");
                        let roam_error = match reply_disposition {
                            FailureDisposition::Cancelled => RoamError::Cancelled,
                            FailureDisposition::Indeterminate => RoamError::Indeterminate,
                        };
                        let error: Result<(), RoamError<core::convert::Infallible>> =
                            Err(roam_error);
                        let _ = self.sender.send_response(req_id, RequestResponse {
                            ret: Payload::outgoing(&error),
                            metadata: Default::default(),
                            schemas: Default::default(),
                        }).await;
                        tracing::debug!(%req_id, "failures_rx: error response sent");
                    }
                }
                Some(ctrl) = self.local_control_rx.recv() => {
                    self.handle_local_control(ctrl).await;
                }
            }
        }

        for (_, in_flight) in std::mem::take(&mut self.in_flight_handlers) {
            if !in_flight.retry.persist {
                in_flight.handle.abort();
            }
        }
        self.shared.pending_responses.lock().clear();

        // Connection is gone: drop channel runtime state so any registered Rx
        // receivers observe closure instead of hanging on recv(), and wake any
        // outbound Tx handles waiting for grant-credit.
        self.close_all_channel_runtime_state();
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
            DriverLocalControl::GrantCredit {
                channel_id,
                additional,
            } => {
                let _ = self
                    .sender
                    .send(ConnectionMessage::Channel(ChannelMessage {
                        id: channel_id,
                        body: ChannelBody::GrantCredit(roam_types::ChannelGrantCredit {
                            additional,
                        }),
                    }))
                    .await;
            }
        }
    }

    fn handle_recv(&mut self, recv: crate::session::RecvMessage) {
        let crate::session::RecvMessage { schemas, msg } = recv;
        let is_request = matches!(&*msg, ConnectionMessage::Request(_));
        if is_request {
            let msg = msg.map(|m| match m {
                ConnectionMessage::Request(r) => r,
                _ => unreachable!(),
            });
            self.handle_request(msg, schemas);
        } else {
            let msg = msg.map(|m| match m {
                ConnectionMessage::Channel(c) => c,
                _ => unreachable!(),
            });
            self.handle_channel(msg);
        }
    }

    fn handle_request(
        &mut self,
        msg: SelfRef<RequestMessage<'static>>,
        schemas: Arc<roam_types::SchemaRecvTracker>,
    ) {
        let req_id = msg.id;
        let is_call = matches!(&msg.body, RequestBody::Call(_));
        let is_response = matches!(&msg.body, RequestBody::Response(_));
        let is_cancel = matches!(&msg.body, RequestBody::Cancel(_));

        if is_call {
            // r[impl rpc.request]
            // r[impl rpc.error.scope]
            let call = msg.map(|m| match m.body {
                RequestBody::Call(c) => c,
                _ => unreachable!(),
            });
            let handler = Arc::clone(&self.handler);
            let retry = handler.retry_policy(call.method_id);
            let operation_id = metadata_operation_id(&call.metadata);

            if let Some(operation_id) = operation_id {
                let admit = self.shared.operations.admit(
                    operation_id,
                    call.method_id,
                    incoming_args_bytes(&call),
                    retry,
                    req_id,
                );
                match admit {
                    OperationAdmit::Attached => return,
                    OperationAdmit::Replay(encoded_response) => {
                        let sender = self.sender.clone();
                        let method_id = call.method_id;
                        moire::task::spawn(
                            async move {
                                if send_encoded_response(
                                    sender.clone(),
                                    req_id,
                                    method_id,
                                    encoded_response,
                                )
                                .await
                                .is_err()
                                {
                                    sender.mark_failure(req_id, FailureDisposition::Cancelled);
                                }
                            }
                            .named("operation_replay"),
                        );
                        return;
                    }
                    OperationAdmit::Conflict => {
                        let sender = self.sender.clone();
                        moire::task::spawn(
                            async move {
                                let error: Result<(), RoamError<core::convert::Infallible>> =
                                    Err(RoamError::InvalidPayload("request ID conflict".into()));
                                let _ = sender
                                    .send_response(
                                        req_id,
                                        RequestResponse {
                                            ret: Payload::outgoing(&error),
                                            metadata: Default::default(),
                                            schemas: Default::default(),
                                        },
                                    )
                                    .await;
                            }
                            .named("operation_reject"),
                        );
                        return;
                    }
                    OperationAdmit::Indeterminate => {
                        let sender = self.sender.clone();
                        moire::task::spawn(
                            async move {
                                let error: Result<(), RoamError<core::convert::Infallible>> =
                                    Err(RoamError::Indeterminate);
                                let _ = sender
                                    .send_response(
                                        req_id,
                                        RequestResponse {
                                            ret: Payload::outgoing(&error),
                                            metadata: Default::default(),
                                            schemas: Default::default(),
                                        },
                                    )
                                    .await;
                            }
                            .named("operation_indeterminate"),
                        );
                        return;
                    }
                    OperationAdmit::Start => {}
                }
            }
            let reply = DriverReplySink {
                sender: Some(self.sender.clone()),
                request_id: req_id,
                method_id: call.method_id,
                retry,
                operation_id,
                operations: operation_id.map(|_| Arc::clone(&self.shared.operations)),
                binder: self.internal_binder(),
            };
            let has_channels = !call.channels.is_empty();
            let join_handle = moire::task::spawn(
                async move {
                    handler.handle(call, reply, schemas).await;
                }
                .named("handler"),
            );
            self.in_flight_handlers.insert(
                req_id,
                InFlightHandler {
                    handle: join_handle,
                    retry,
                    has_channels,
                    operation_id,
                },
            );
        } else if is_response {
            // r[impl rpc.response.one-per-request]
            tracing::debug!(%req_id, "driver received response");
            if let Some(tx) = self.shared.pending_responses.lock().remove(&req_id) {
                tracing::debug!(%req_id, "routing response to pending oneshot");
                let _: Result<(), _> = tx.send(PendingResponse { msg, schemas });
            } else {
                tracing::debug!(%req_id, "no pending response slot for this req_id");
            }
        } else if is_cancel {
            // r[impl rpc.cancel]
            // r[impl rpc.cancel.channels]
            tracing::debug!(%req_id, in_flight = self.in_flight_handlers.contains_key(&req_id), "received cancel");
            match self.shared.operations.cancel(req_id) {
                OperationCancel::None => {
                    let should_abort = self
                        .in_flight_handlers
                        .get(&req_id)
                        .map(|in_flight| !in_flight.retry.persist)
                        .unwrap_or(false);
                    tracing::debug!(%req_id, should_abort, "cancel OperationCancel::None");
                    if should_abort && let Some(in_flight) = self.in_flight_handlers.remove(&req_id)
                    {
                        tracing::debug!(%req_id, "aborting handler");
                        in_flight.handle.abort();
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
                self.close_outbound_channel(chan_id);
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
                self.close_outbound_channel(chan_id);
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
