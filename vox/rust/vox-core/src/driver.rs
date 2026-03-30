use std::{
    collections::BTreeMap,
    pin::Pin,
    sync::{
        Arc, Weak,
        atomic::{AtomicU64, Ordering},
    },
};

use moire::sync::{Semaphore, SyncMutex};
use tokio::sync::watch;

use moire::task::FutureExt as _;
use vox_types::{
    BoxFut, CallResult, Caller, ChannelBinder, ChannelBody, ChannelClose, ChannelCreditReplenisher,
    ChannelCreditReplenisherHandle, ChannelId, ChannelItem, ChannelLivenessHandle, ChannelMessage,
    ChannelRetryMode, ChannelSink, CreditSink, Handler, IdAllocator, IncomingChannelMessage,
    Payload, ReplySink, RequestBody, RequestCall, RequestId, RequestMessage, RequestResponse,
    SelfRef, TxError, VoxError, ensure_operation_id, metadata_channel_retry_mode,
    metadata_operation_id,
};

use crate::session::{
    ConnectionHandle, ConnectionMessage, ConnectionSender, DropControlRequest, FailureDisposition,
};
use crate::{InMemoryOperationStore, OperationStore};
use moire::sync::mpsc;
use vox_types::{OperationId, PostcardPayload, SchemaHash, TypeRef};

/// A pending response for one outbound request attempt.
///
/// Carries both the wire response message and the recv tracker that was
/// current when the response was received, so the caller can deserialize
/// the response with the correct schemas.
struct PendingResponse {
    msg: SelfRef<RequestMessage<'static>>,
    schemas: Arc<vox_types::SchemaRecvTracker>,
}

type ResponseSlot = moire::sync::oneshot::Sender<PendingResponse>;

struct InFlightHandler {
    handle: moire::task::JoinHandle<()>,
    method_id: vox_types::MethodId,
    retry: vox_types::RetryPolicy,
    has_channels: bool,
    operation_id: Option<OperationId>,
}

// ============================================================================
// Live operation tracking (driver-local, not persisted)
// ============================================================================

/// Tracks in-flight operations within the current session.
///
/// This is session-scoped state that does NOT survive crashes. The
/// `OperationStore` handles persistence; this handles the live
/// attach/waiter/conflict logic.
struct LiveOperationTracker {
    /// Maps operation_id → live state. Removed when sealed or released.
    live: HashMap<OperationId, LiveOperation>,
    /// Maps request_id → operation_id for cancel routing.
    request_to_operation: HashMap<RequestId, OperationId>,
}

struct LiveOperation {
    method_id: vox_types::MethodId,
    args_hash: u64,
    owner_request_id: RequestId,
    waiters: Vec<RequestId>,
    retry: vox_types::RetryPolicy,
}

enum AdmitResult {
    /// New operation — run the handler.
    Start,
    /// Same operation already in flight — wait for its result.
    Attached,
    /// Same operation ID but different method/args — protocol error.
    Conflict,
}

impl LiveOperationTracker {
    fn new() -> Self {
        Self {
            live: HashMap::new(),
            request_to_operation: HashMap::new(),
        }
    }

    fn admit(
        &mut self,
        operation_id: OperationId,
        method_id: vox_types::MethodId,
        args: &[u8],
        retry: vox_types::RetryPolicy,
        request_id: RequestId,
    ) -> AdmitResult {
        use std::hash::{Hash, Hasher};
        let args_hash = {
            let mut h = std::collections::hash_map::DefaultHasher::new();
            method_id.hash(&mut h);
            args.hash(&mut h);
            h.finish()
        };
        let live_operations = self.live.len();

        if let Some(live) = self.live.get_mut(&operation_id) {
            if live.method_id != method_id || live.args_hash != args_hash {
                let request_bindings = self.request_to_operation.len();
                tracing::trace!(
                    %operation_id,
                    %request_id,
                    ?method_id,
                    live_operations,
                    request_bindings,
                    "live operation conflict"
                );
                return AdmitResult::Conflict;
            }
            live.waiters.push(request_id);
            self.request_to_operation.insert(request_id, operation_id);
            let waiters = live.waiters.len();
            let request_bindings = self.request_to_operation.len();
            tracing::trace!(
                %operation_id,
                %request_id,
                ?method_id,
                waiters,
                live_operations,
                request_bindings,
                "live operation attached"
            );
            return AdmitResult::Attached;
        }

        self.live.insert(
            operation_id,
            LiveOperation {
                method_id,
                args_hash,
                owner_request_id: request_id,
                waiters: vec![request_id],
                retry,
            },
        );
        self.request_to_operation.insert(request_id, operation_id);
        let live_operations = self.live.len();
        let request_bindings = self.request_to_operation.len();
        tracing::trace!(
            %operation_id,
            %request_id,
            ?method_id,
            live_operations,
            request_bindings,
            "live operation admitted"
        );
        AdmitResult::Start
    }

    /// Seal a live operation, returning all waiter request IDs (including the owner).
    fn seal(&mut self, operation_id: OperationId) -> Vec<RequestId> {
        if let Some(live) = self.live.remove(&operation_id) {
            for waiter in &live.waiters {
                self.request_to_operation.remove(waiter);
            }
            let waiters = live.waiters.len();
            let live_operations = self.live.len();
            let request_bindings = self.request_to_operation.len();
            tracing::trace!(
                %operation_id,
                waiters,
                live_operations,
                request_bindings,
                "live operation sealed"
            );
            live.waiters
        } else {
            vec![]
        }
    }

    /// Release a live operation without sealing (handler failed).
    fn release(&mut self, operation_id: OperationId) -> Option<LiveOperation> {
        if let Some(live) = self.live.remove(&operation_id) {
            for waiter in &live.waiters {
                self.request_to_operation.remove(waiter);
            }
            let waiters = live.waiters.len();
            let live_operations = self.live.len();
            let request_bindings = self.request_to_operation.len();
            tracing::trace!(
                %operation_id,
                waiters,
                live_operations,
                request_bindings,
                "live operation released"
            );
            Some(live)
        } else {
            None
        }
    }

    /// Cancel a request. Returns what to do.
    fn cancel(&mut self, request_id: RequestId) -> CancelResult {
        let Some(&operation_id) = self.request_to_operation.get(&request_id) else {
            return CancelResult::NotFound;
        };
        let live_operations = self.live.len();
        let Some(live) = self.live.get_mut(&operation_id) else {
            self.request_to_operation.remove(&request_id);
            return CancelResult::NotFound;
        };

        if live.retry.persist {
            // Persistent operations: only detach non-owner waiters.
            if live.owner_request_id == request_id {
                return CancelResult::NotFound; // Can't cancel the owner of a persistent op
            }
            live.waiters.retain(|w| *w != request_id);
            self.request_to_operation.remove(&request_id);
            let waiters = live.waiters.len();
            let request_bindings = self.request_to_operation.len();
            tracing::trace!(
                %operation_id,
                %request_id,
                waiters,
                live_operations,
                request_bindings,
                "live operation detached waiter"
            );
            CancelResult::Detached
        } else {
            // Non-persistent: abort the whole operation.
            let live = self.live.remove(&operation_id).unwrap();
            for waiter in &live.waiters {
                self.request_to_operation.remove(waiter);
            }
            let waiters = live.waiters.len();
            let live_operations = self.live.len();
            let request_bindings = self.request_to_operation.len();
            tracing::trace!(
                %operation_id,
                %request_id,
                waiters,
                live_operations,
                request_bindings,
                "live operation aborted"
            );
            CancelResult::Abort {
                owner_request_id: live.owner_request_id,
                waiters: live.waiters,
            }
        }
    }
}

enum CancelResult {
    NotFound,
    Detached,
    Abort {
        owner_request_id: RequestId,
        waiters: Vec<RequestId>,
    },
}

use std::collections::HashMap;

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
    channel_senders: SyncMutex<BTreeMap<ChannelId, mpsc::Sender<IncomingChannelMessage>>>,
    /// Buffer for channel messages that arrive before the channel is registered.
    ///
    /// This handles the race between the caller sending items immediately after
    /// channel binding, and the callee's handler task registering the channel
    /// receiver. Items arriving in that window are buffered here and drained
    /// when the channel is registered.
    channel_buffers: SyncMutex<BTreeMap<ChannelId, Vec<IncomingChannelMessage>>>,
    /// Credit semaphores for outbound channels (Tx on our side).
    /// The driver's GrantCredit handler adds permits to these.
    channel_credits: SyncMutex<BTreeMap<ChannelId, Arc<Semaphore>>>,
    /// Channel IDs cleared during session resume. When handler tasks that owned
    /// these channels are aborted, they may trigger `close_channel_on_drop`, which
    /// would send a ChannelClose message for a channel the peer no longer knows about.
    /// We suppress those Close messages by checking this set.
    stale_close_channels: SyncMutex<std::collections::HashSet<ChannelId>>,
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
    use tokio::sync::mpsc::error::TryRecvError;
    use vox_types::{ChannelCreditReplenisher, ChannelId};

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
/// `VoxError::Cancelled` to the caller. This guarantees that every
/// request attempt receives exactly one terminal response
/// (`rpc.response.one-per-request`), even if the handler panics or
/// forgets to reply.
pub struct DriverReplySink {
    sender: Option<ConnectionSender>,
    request_id: RequestId,
    method_id: vox_types::MethodId,
    retry: vox_types::RetryPolicy,
    operation_id: Option<OperationId>,
    operations: Option<Arc<dyn OperationStore>>,
    live_operations: Option<Arc<SyncMutex<LiveOperationTracker>>>,
    binder: DriverChannelBinder,
}

/// Replay a sealed response from the operation store.
///
/// The stored bytes do NOT contain schemas. Schemas are sourced from the
/// operation store via the send tracker, which deduplicates against what
/// was already sent on this connection.
async fn replay_sealed_response(
    sender: ConnectionSender,
    request_id: RequestId,
    method_id: vox_types::MethodId,
    encoded_response: &[u8],
    root_type: TypeRef,
    operations: &dyn OperationStore,
) -> Result<(), ()> {
    let mut response: RequestResponse<'_> =
        vox_postcard::from_slice_borrowed(encoded_response).map_err(|_| ())?;
    sender.prepare_replay_schemas(request_id, method_id, &root_type, operations, &mut response);
    sender.send_response(request_id, response).await
}

/// Extract the root TypeRef from a response's schema CBOR payload.
fn extract_root_type_ref(schemas_cbor: &vox_types::CborPayload) -> TypeRef {
    if schemas_cbor.is_empty() {
        return TypeRef::concrete(SchemaHash(0));
    }
    let payload =
        vox_types::SchemaPayload::from_cbor(&schemas_cbor.0).expect("schema CBOR must be valid");
    payload.root
}

fn incoming_args_bytes<'a>(call: &'a RequestCall<'a>) -> &'a [u8] {
    match &call.args {
        Payload::PostcardBytes(bytes) => bytes,
        Payload::Value { .. } => {
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

        vox_types::dlog!(
            "[driver] send_reply: conn={:?} req={:?} method={:?} payload={} operation_id={:?}",
            sender.connection_id(),
            self.request_id,
            self.method_id,
            match &response.ret {
                Payload::Value { .. } => "Value",
                Payload::PostcardBytes(_) => "PostcardBytes",
            },
            self.operation_id
        );

        if let Payload::Value { shape, .. } = &response.ret
            && let Ok(extracted) = vox_types::extract_schemas(shape)
        {
            vox_types::dlog!(
                "[schema] driver send_reply: method={:?} root={:?}",
                self.method_id,
                extracted.root
            );
        }

        if let (Some(operation_id), Some(operations)) = (self.operation_id, self.operations.take())
        {
            let mut response = response;
            sender.prepare_response_for_method(self.request_id, self.method_id, &mut response);

            // Extract the root type ref before we strip schemas for storage.
            let root_type = extract_root_type_ref(&response.schemas);

            // Serialize the response WITHOUT schemas for the operation store.
            let schemas_for_wire = std::mem::take(&mut response.schemas);
            let encoded_for_store = PostcardPayload(
                vox_postcard::to_vec(&response).expect("serialize operation response for store"),
            );
            response.schemas = schemas_for_wire;

            // Send the full response (with schemas) on the wire.
            vox_types::dlog!(
                "[driver] send_reply wire send: conn={:?} req={:?} method={:?} schemas={}",
                sender.connection_id(),
                self.request_id,
                self.method_id,
                response.schemas.0.len()
            );
            if let Err(_e) = sender.send_response(self.request_id, response).await {
                sender.mark_failure(self.request_id, FailureDisposition::Cancelled);
            }

            // Seal in the persistent store (payload without schemas).
            let registry = sender.schema_registry();
            operations.seal(operation_id, &encoded_for_store, &root_type, &registry);

            // Get waiters from the live tracker and replay to them.
            let waiters = self
                .live_operations
                .as_ref()
                .map(|lo| lo.lock().seal(operation_id))
                .unwrap_or_default();
            for waiter in waiters {
                if waiter == self.request_id {
                    continue;
                }
                if replay_sealed_response(
                    sender.clone(),
                    waiter,
                    self.method_id,
                    encoded_for_store.as_bytes(),
                    root_type.clone(),
                    operations.as_ref(),
                )
                .await
                .is_err()
                {
                    sender.mark_failure(waiter, FailureDisposition::Cancelled);
                }
            }
        } else {
            vox_types::dlog!(
                "[driver] send_reply direct send: conn={:?} req={:?} method={:?}",
                sender.connection_id(),
                self.request_id,
                self.method_id
            );
            if let Err(_e) = sender
                .send_response_for_method(self.request_id, self.method_id, response)
                .await
            {
                sender.mark_failure(self.request_id, FailureDisposition::Cancelled);
            }
        }
    }

    fn channel_binder(&self) -> Option<&dyn ChannelBinder> {
        Some(&self.binder)
    }

    fn request_id(&self) -> Option<RequestId> {
        Some(self.request_id)
    }

    fn connection_id(&self) -> Option<vox_types::ConnectionId> {
        self.sender.as_ref().map(|sender| sender.connection_id())
    }
}

// r[impl rpc.response.one-per-request]
impl Drop for DriverReplySink {
    fn drop(&mut self) {
        if let Some(sender) = self.sender.take() {
            let disposition = if self.retry.persist {
                FailureDisposition::Indeterminate
            } else {
                FailureDisposition::Cancelled
            };

            if let Some(operation_id) = self.operation_id {
                // Don't remove from persistent store — non-idem ops stay as
                // Admitted so future lookups return Indeterminate. Idem ops
                // were never admitted to the store in the first place.

                // Release waiters from the live tracker.
                if let Some(live_ops) = self.live_operations.take()
                    && let Some(live) = live_ops.lock().release(operation_id)
                {
                    for waiter in live.waiters {
                        sender.mark_failure(waiter, disposition);
                    }
                    return;
                }
            }

            sender.mark_failure(self.request_id, disposition);
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
    ) -> Pin<Box<dyn vox_types::MaybeSendFuture<Output = Result<(), TxError>> + 'payload>> {
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
        _metadata: vox_types::Metadata,
    ) -> Pin<Box<dyn vox_types::MaybeSendFuture<Output = Result<(), TxError>> + 'static>> {
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

/// Default initial credit for all channels.
const DEFAULT_CHANNEL_CREDIT: u32 = 16;

fn register_rx_channel_impl(
    shared: &Arc<DriverShared>,
    channel_id: ChannelId,
    queue_name: &'static str,
    liveness: Option<ChannelLivenessHandle>,
    local_control_tx: mpsc::UnboundedSender<DriverLocalControl>,
) -> vox_types::BoundChannelReceiver {
    let (tx, rx) = mpsc::channel(queue_name, 64);

    let mut terminal_buffered = false;
    {
        let mut senders = shared.channel_senders.lock();

        // Publish the live sender and keep the registry locked until any
        // pre-registration backlog has been drained.
        //
        // This makes the handoff lossless and order-preserving:
        // - items that raced with registration cannot create a fresh orphan
        //   buffer entry because the live sender is already visible
        // - newer items cannot bypass older buffered items because
        //   handle_channel() blocks on channel_senders until the drain finishes
        senders.insert(channel_id, tx.clone());

        let buffered = shared.channel_buffers.lock().remove(&channel_id);
        if let Some(buffered) = buffered {
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
            senders.remove(&channel_id);
        }
    }

    if terminal_buffered {
        shared.channel_credits.lock().remove(&channel_id);
        return vox_types::BoundChannelReceiver {
            receiver: rx,
            liveness,
            replenisher: None,
        };
    }

    vox_types::BoundChannelReceiver {
        receiver: rx,
        liveness,
        replenisher: Some(Arc::new(DriverChannelCreditReplenisher::new(
            channel_id,
            DEFAULT_CHANNEL_CREDIT,
            local_control_tx,
        )) as ChannelCreditReplenisherHandle),
    }
}

impl DriverChannelBinder {
    fn create_tx_channel(&self) -> (ChannelId, Arc<CreditSink<DriverChannelSink>>) {
        let channel_id = self.shared.channel_ids.lock().alloc();
        let inner = DriverChannelSink {
            sender: self.sender.clone(),
            channel_id,
            local_control_tx: self.local_control_tx.clone(),
        };
        let sink = Arc::new(CreditSink::new(inner, DEFAULT_CHANNEL_CREDIT));
        self.shared
            .channel_credits
            .lock()
            .insert(channel_id, Arc::clone(sink.credit()));
        (channel_id, sink)
    }

    fn register_rx_channel(&self, channel_id: ChannelId) -> vox_types::BoundChannelReceiver {
        register_rx_channel_impl(
            &self.shared,
            channel_id,
            "driver.register_rx_channel",
            self.channel_liveness(),
            self.local_control_tx.clone(),
        )
    }
}

impl ChannelBinder for DriverChannelBinder {
    fn create_tx(&self) -> (ChannelId, Arc<dyn ChannelSink>) {
        let (id, sink) = self.create_tx_channel();
        (id, sink as Arc<dyn ChannelSink>)
    }

    fn create_rx(&self) -> (ChannelId, vox_types::BoundChannelReceiver) {
        let channel_id = self.shared.channel_ids.lock().alloc();
        let rx = self.register_rx_channel(channel_id);
        (channel_id, rx)
    }

    fn bind_tx(&self, channel_id: ChannelId) -> Arc<dyn ChannelSink> {
        let inner = DriverChannelSink {
            sender: self.sender.clone(),
            channel_id,
            local_control_tx: self.local_control_tx.clone(),
        };
        let sink = Arc::new(CreditSink::new(inner, DEFAULT_CHANNEL_CREDIT));
        self.shared
            .channel_credits
            .lock()
            .insert(channel_id, Arc::clone(sink.credit()));
        sink
    }

    fn register_rx(&self, channel_id: ChannelId) -> vox_types::BoundChannelReceiver {
        self.register_rx_channel(channel_id)
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
    /// The returned sink enforces credit; the semaphore is registered so
    /// `GrantCredit` messages can add permits.
    pub fn create_tx_channel(&self) -> (ChannelId, Arc<CreditSink<DriverChannelSink>>) {
        let channel_id = self.shared.channel_ids.lock().alloc();
        let inner = DriverChannelSink {
            sender: self.sender.clone(),
            channel_id,
            local_control_tx: self.local_control_tx.clone(),
        };
        let sink = Arc::new(CreditSink::new(inner, DEFAULT_CHANNEL_CREDIT));
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
    pub fn register_rx_channel(&self, channel_id: ChannelId) -> vox_types::BoundChannelReceiver {
        register_rx_channel_impl(
            &self.shared,
            channel_id,
            "driver.caller.register_rx_channel",
            self.channel_liveness(),
            self.local_control_tx.clone(),
        )
    }
}

impl ChannelBinder for DriverCaller {
    fn create_tx(&self) -> (ChannelId, Arc<dyn ChannelSink>) {
        let (id, sink) = self.create_tx_channel();
        (id, sink as Arc<dyn ChannelSink>)
    }

    fn create_rx(&self) -> (ChannelId, vox_types::BoundChannelReceiver) {
        let channel_id = self.shared.channel_ids.lock().alloc();
        let rx = self.register_rx_channel(channel_id);
        (channel_id, rx)
    }

    fn bind_tx(&self, channel_id: ChannelId) -> Arc<dyn ChannelSink> {
        let inner = DriverChannelSink {
            sender: self.sender.clone(),
            channel_id,
            local_control_tx: self.local_control_tx.clone(),
        };
        let sink = Arc::new(CreditSink::new(inner, DEFAULT_CHANNEL_CREDIT));
        self.shared
            .channel_credits
            .lock()
            .insert(channel_id, Arc::clone(sink.credit()));
        sink
    }

    fn register_rx(&self, channel_id: ChannelId) -> vox_types::BoundChannelReceiver {
        self.register_rx_channel(channel_id)
    }

    fn channel_liveness(&self) -> Option<ChannelLivenessHandle> {
        self._drop_guard
            .as_ref()
            .map(|guard| guard.clone() as ChannelLivenessHandle)
    }
}

impl Caller for DriverCaller {
    async fn call<'a>(&'a self, mut call: RequestCall<'a>) -> CallResult {
        if self.peer_supports_retry {
            let operation_id = OperationId(
                self.shared
                    .next_operation_id
                    .fetch_add(1, Ordering::Relaxed),
            );
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
        // with Payload::Value — no separate prepare step needed.
        //
        // Channel binding happens during serialization via the thread-local
        // ChannelBinder — no post-hoc walk needed.
        if self
            .sender
            .send_with_binder(
                ConnectionMessage::Request(RequestMessage {
                    id: req_id,
                    body: RequestBody::Call(RequestCall {
                        method_id: call.method_id,
                        args: call.args.reborrow(),
                        metadata: call.metadata.clone(),
                        schemas: Default::default(),
                    }),
                }),
                Some(self),
            )
            .await
            .is_err()
        {
            self.shared.pending_responses.lock().remove(&req_id);
            return Err(VoxError::SendFailed);
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
                            return Err(VoxError::ConnectionClosed);
                        }
                    }
                }
                changed = resumed_rx.changed(), if self.peer_supports_retry => {
                    vox_types::dlog!("[CALLER] resumed_rx fired");
                    if changed.is_err() {
                        self.shared.pending_responses.lock().remove(&req_id);
                        return Err(VoxError::SessionShutdown);
                    }
                    let generation = *resumed_rx.borrow();
                    if generation == seen_resume_generation {
                        continue;
                    }
                    seen_resume_generation = generation;
                    while *resume_processed_rx.borrow() < generation {
                        if resume_processed_rx.changed().await.is_err() {
                            self.shared.pending_responses.lock().remove(&req_id);
                            return Err(VoxError::SessionShutdown);
                        }
                    }
                    match metadata_channel_retry_mode(&call.metadata) {
                        ChannelRetryMode::NonIdem => {
                            self.shared.pending_responses.lock().remove(&req_id);
                            return Err(VoxError::Indeterminate);
                        }
                        ChannelRetryMode::Idem | ChannelRetryMode::None => {}
                    }
                    // Re-send the request after resume.
                    // Channel binding is embedded in the serialized payload,
                    // so no separate re-binding step is needed.
                    let _ = self.sender.send_with_binder(
                        ConnectionMessage::Request(RequestMessage {
                            id: req_id,
                            body: RequestBody::Call(RequestCall {
                                method_id: call.method_id,
                                args: call.args.reborrow(),
                                metadata: call.metadata.clone(),
                                schemas: Default::default(),
                            }),
                        }),
                        Some(self),
                    ).await;
                }
                changed = closed_rx.changed() => {
                    vox_types::dlog!("[CALLER] closed_rx fired, value={}", *closed_rx.borrow());
                    if changed.is_err() || *closed_rx.borrow() {
                        self.shared.pending_responses.lock().remove(&req_id);
                        return Err(VoxError::ConnectionClosed);
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

        Ok(vox_types::WithTracker {
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
    /// Tracks live operations for dedup/attach/conflict within this session.
    /// Shared with DriverReplySink so seal can return waiters.
    live_operations: Arc<SyncMutex<LiveOperationTracker>>,
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
    HandlerCompleted {
        request_id: RequestId,
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
        let mut credits = self.shared.channel_credits.lock();
        for semaphore in credits.values() {
            semaphore.close();
        }
        // Track all outbound channel IDs that are being cleared so we can suppress
        // ChannelClose messages triggered by aborted handler tasks dropping their Tx handles.
        let mut stale = self.shared.stale_close_channels.lock();
        stale.extend(credits.keys().copied());
        credits.clear();
        drop(credits);

        self.shared.channel_senders.lock().clear();
        self.shared.channel_buffers.lock().clear();
    }

    fn close_outbound_channel(&self, channel_id: ChannelId) {
        if let Some(semaphore) = self.shared.channel_credits.lock().remove(&channel_id) {
            semaphore.close();
        }
    }

    fn abort_channel_handlers(&mut self) {
        for (_req_id, in_flight) in &self.in_flight_handlers {
            if in_flight.has_channels {
                if let Some(operation_id) = in_flight.operation_id {
                    self.shared.operations.remove(operation_id);
                    self.live_operations.lock().release(operation_id);
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
                stale_close_channels: SyncMutex::new(
                    "driver.stale_close_channels",
                    std::collections::HashSet::new(),
                ),
            }),
            in_flight_handlers: BTreeMap::new(),
            live_operations: Arc::new(SyncMutex::new(
                "driver.live_operations",
                LiveOperationTracker::new(),
            )),
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
                            self.handle_recv(recv);
                        }
                        None => {
                            tracing::trace!("driver rx closed, exiting loop");
                            break;
                        }
                    }
                }
                Some((req_id, disposition)) = self.failures_rx.recv() => {
                    tracing::trace!(%req_id, ?disposition, "failures_rx fired");
                    let in_flight_found = self.in_flight_handlers.contains_key(&req_id);
                    let in_flight_method_id =
                        self.in_flight_handlers.get(&req_id).map(|in_flight| in_flight.method_id);
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
                    tracing::trace!(%req_id, in_flight_found, ?reply_disposition, "failures_rx computed disposition");
                    // Clean up the handler tracking entry.
                    self.in_flight_handlers.remove(&req_id);
                    tracing::trace!(%req_id, in_flight = self.in_flight_handlers.len(), "handler removed on failure");
                    let had_pending = self.shared.pending_responses.lock().remove(&req_id).is_some();
                    tracing::trace!(%req_id, had_pending, "failures_rx checked pending_responses");
                    if !had_pending {
                        let Some(reply_disposition) = reply_disposition else {
                            tracing::trace!(%req_id, "failures_rx: no reply_disposition, skipping");
                            continue;
                        };
                        tracing::trace!(%req_id, ?reply_disposition, "failures_rx: sending error response");
                        let vox_error = match reply_disposition {
                            FailureDisposition::Cancelled => VoxError::Cancelled,
                            FailureDisposition::Indeterminate => VoxError::Indeterminate,
                        };
                        if let Some(method_id) = in_flight_method_id
                            && let Some(response_shape) = self.handler.response_wire_shape(method_id)
                            && let Ok(extracted) = vox_types::extract_schemas(response_shape)
                        {
                            let registry = vox_types::build_registry(&extracted.schemas);
                            let error: Result<(), VoxError<core::convert::Infallible>> =
                                Err(vox_error);
                            let encoded = vox_postcard::to_vec(&error)
                                .expect("serialize runtime-generated error response");
                            let mut response = RequestResponse {
                                ret: Payload::PostcardBytes(Box::leak(encoded.into_boxed_slice())),
                                metadata: Default::default(),
                                schemas: Default::default(),
                            };
                            self.sender.prepare_response_from_source(
                                req_id,
                                method_id,
                                &extracted.root,
                                &registry,
                                &mut response,
                            );
                            let _ = self.sender.send_response(req_id, response).await;
                        } else {
                            let error: Result<(), VoxError<core::convert::Infallible>> =
                                Err(vox_error);
                            let _ = self.sender.send_response(req_id, RequestResponse {
                                ret: Payload::outgoing(&error),
                                metadata: Default::default(),
                                schemas: Default::default(),
                            }).await;
                        }
                        tracing::trace!(%req_id, "failures_rx: error response sent");
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
                // Don't send Close for channels that were cleared during session resume.
                // When handler tasks are aborted, their dropped Tx handles trigger
                // close_channel_on_drop, but we should not send Close to the peer
                // for channels the peer has also cleared.
                if self.shared.stale_close_channels.lock().remove(&channel_id) {
                    tracing::trace!(%channel_id, "suppressing ChannelClose for stale channel");
                    return;
                }
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
                        body: ChannelBody::GrantCredit(vox_types::ChannelGrantCredit {
                            additional,
                        }),
                    }))
                    .await;
            }
            DriverLocalControl::HandlerCompleted { request_id } => {
                let removed = self.in_flight_handlers.remove(&request_id).is_some();
                tracing::trace!(
                    %request_id,
                    removed,
                    in_flight = self.in_flight_handlers.len(),
                    "handler completion processed"
                );
            }
        }
    }

    fn handle_recv(&mut self, recv: crate::session::RecvMessage) {
        let crate::session::RecvMessage { schemas, msg } = recv;
        let is_request = matches!(&*msg, ConnectionMessage::Request(_));
        if is_request {
            if let ConnectionMessage::Request(req) = &*msg {
                vox_types::dlog!(
                    "[driver] handle_recv request: conn={:?} req={:?} body={} method={:?}",
                    self.sender.connection_id(),
                    req.id,
                    match &req.body {
                        RequestBody::Call(_) => "Call",
                        RequestBody::Response(_) => "Response",
                        RequestBody::Cancel(_) => "Cancel",
                    },
                    match &req.body {
                        RequestBody::Call(call) => Some(call.method_id),
                        RequestBody::Response(_) | RequestBody::Cancel(_) => None,
                    }
                );
                match &req.body {
                    RequestBody::Call(call) => tracing::trace!(
                        conn_id = self.sender.connection_id().0,
                        req_id = req.id.0,
                        method_id = call.method_id.0,
                        "driver received call"
                    ),
                    RequestBody::Response(_) => tracing::trace!(
                        conn_id = self.sender.connection_id().0,
                        req_id = req.id.0,
                        "driver received response message"
                    ),
                    RequestBody::Cancel(_) => tracing::trace!(
                        conn_id = self.sender.connection_id().0,
                        req_id = req.id.0,
                        "driver received cancel message"
                    ),
                }
            }
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
        schemas: Arc<vox_types::SchemaRecvTracker>,
    ) {
        let req_id = msg.id;
        let is_call = matches!(&msg.body, RequestBody::Call(_));
        let is_response = matches!(&msg.body, RequestBody::Response(_));
        let is_cancel = matches!(&msg.body, RequestBody::Cancel(_));

        if is_call {
            let method_id = match &msg.body {
                RequestBody::Call(call) => call.method_id,
                _ => unreachable!(),
            };
            vox_types::dlog!(
                "[driver] inbound call: conn={:?} req={:?} method={:?}",
                self.sender.connection_id(),
                req_id,
                method_id
            );
            // r[impl rpc.request]
            // r[impl rpc.error.scope]
            let call = msg.map(|m| match m.body {
                RequestBody::Call(c) => c,
                _ => unreachable!(),
            });
            let handler = Arc::clone(&self.handler);
            let retry = handler.retry_policy(call.method_id);
            // Idempotent requests can be re-executed safely; skip operation tracking/storage.
            let operation_id = metadata_operation_id(&call.metadata).filter(|_| !retry.idem);
            let method_id = call.method_id;

            if let Some(operation_id) = operation_id {
                // 1. Check live tracker (in-flight operations in this session)
                let admit = self.live_operations.lock().admit(
                    operation_id,
                    call.method_id,
                    incoming_args_bytes(&call),
                    retry,
                    req_id,
                );
                match admit {
                    AdmitResult::Attached => return,
                    AdmitResult::Conflict => {
                        let sender = self.sender.clone();
                        moire::task::spawn(
                            async move {
                                let error: Result<(), VoxError<core::convert::Infallible>> =
                                    Err(VoxError::InvalidPayload("operation ID conflict".into()));
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
                    AdmitResult::Start => {}
                }

                // 2. Check persistent store (sealed/admitted from previous sessions)
                match self.shared.operations.lookup(operation_id) {
                    crate::OperationState::Sealed => {
                        // Replay the sealed response.
                        if let Some(sealed) = self.shared.operations.get_sealed(operation_id) {
                            let sender = self.sender.clone();
                            let method_id = call.method_id;
                            let operations = Arc::clone(&self.shared.operations);
                            // Remove from live tracker — we're replaying, not running a handler.
                            self.live_operations.lock().seal(operation_id);
                            moire::task::spawn(
                                async move {
                                    if replay_sealed_response(
                                        sender.clone(),
                                        req_id,
                                        method_id,
                                        sealed.response.as_bytes(),
                                        sealed.root_type,
                                        operations.as_ref(),
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
                    }
                    crate::OperationState::Admitted => {
                        // Previously admitted but never sealed — indeterminate.
                        self.live_operations.lock().seal(operation_id);
                        let sender = self.sender.clone();
                        moire::task::spawn(
                            async move {
                                let error: Result<(), VoxError<core::convert::Infallible>> =
                                    Err(VoxError::Indeterminate);
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
                    crate::OperationState::Unknown => {
                        // New operation — admit in the persistent store if non-idem.
                        // Idem operations can safely be re-executed, no need to track.
                        if !retry.idem {
                            self.shared.operations.admit(operation_id);
                        }
                    }
                }
            }
            let reply = DriverReplySink {
                sender: Some(self.sender.clone()),
                request_id: req_id,
                method_id: call.method_id,
                retry,
                operation_id,
                operations: operation_id.map(|_| Arc::clone(&self.shared.operations)),
                live_operations: operation_id.map(|_| Arc::clone(&self.live_operations)),
                binder: self.internal_binder(),
            };
            let has_channels = handler.args_have_channels(call.method_id);
            let local_control_tx = self.local_control_tx.clone();
            let join_handle = moire::task::spawn(
                async move {
                    vox_types::dlog!(
                        "[driver] handler start: req={:?} method={:?}",
                        req_id,
                        method_id
                    );
                    handler.handle(call, reply, schemas).await;
                    vox_types::dlog!(
                        "[driver] handler done: req={:?} method={:?}",
                        req_id,
                        method_id
                    );
                    let _ = local_control_tx
                        .send(DriverLocalControl::HandlerCompleted { request_id: req_id });
                }
                .named("handler"),
            );
            self.in_flight_handlers.insert(
                req_id,
                InFlightHandler {
                    handle: join_handle,
                    method_id,
                    retry,
                    has_channels,
                    operation_id,
                },
            );
            tracing::trace!(%req_id, in_flight = self.in_flight_handlers.len(), "handler inserted");
        } else if is_response {
            // r[impl rpc.response.one-per-request]
            vox_types::dlog!(
                "[driver] inbound response: conn={:?} req={:?}",
                self.sender.connection_id(),
                req_id
            );
            tracing::trace!(%req_id, "driver received response");
            if let Some(tx) = self.shared.pending_responses.lock().remove(&req_id) {
                vox_types::dlog!("[driver] routing response to waiter: req={:?}", req_id);
                tracing::trace!(%req_id, "routing response to pending oneshot");
                let _: Result<(), _> = tx.send(PendingResponse { msg, schemas });
            } else {
                vox_types::dlog!("[driver] dropped unmatched response: req={:?}", req_id);
                tracing::trace!(%req_id, "no pending response slot for this req_id");
            }
        } else if is_cancel {
            vox_types::dlog!(
                "[driver] inbound cancel: conn={:?} req={:?}",
                self.sender.connection_id(),
                req_id
            );
            // r[impl rpc.cancel]
            // r[impl rpc.cancel.channels]
            tracing::trace!(%req_id, in_flight = self.in_flight_handlers.contains_key(&req_id), "received cancel");
            match self.live_operations.lock().cancel(req_id) {
                CancelResult::NotFound => {
                    let should_abort = self
                        .in_flight_handlers
                        .get(&req_id)
                        .map(|in_flight| !in_flight.retry.persist)
                        .unwrap_or(false);
                    tracing::trace!(%req_id, should_abort, "cancel: not in live operations");
                    if should_abort && let Some(in_flight) = self.in_flight_handlers.remove(&req_id)
                    {
                        tracing::trace!(%req_id, "aborting handler");
                        in_flight.handle.abort();
                        tracing::trace!(%req_id, in_flight = self.in_flight_handlers.len(), "handler removed on cancel");
                    }
                }
                CancelResult::Detached => {}
                CancelResult::Abort {
                    owner_request_id,
                    waiters,
                } => {
                    if let Some(in_flight) = self.in_flight_handlers.remove(&owner_request_id) {
                        if let Some(op_id) = in_flight.operation_id {
                            self.shared.operations.remove(op_id);
                        }
                        in_flight.handle.abort();
                        tracing::trace!(%owner_request_id, in_flight = self.in_flight_handlers.len(), "owner handler removed on abort");
                    }
                    for waiter in waiters {
                        self.sender
                            .mark_failure(waiter, FailureDisposition::Cancelled);
                    }
                }
            }
            // The response is sent automatically: aborting drops DriverReplySink →
            // mark_failure fires → failures_rx arm sends VoxError::Cancelled.
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
                    tracing::trace!(
                        conn_id = self.sender.connection_id().0,
                        channel_id = chan_id.0,
                        registered = true,
                        "driver received channel item"
                    );
                    let item = msg.map(|m| match m.body {
                        ChannelBody::Item(item) => item,
                        _ => unreachable!(),
                    });
                    // try_send: if the Rx has been dropped or the buffer is full, drop the item.
                    let _ = tx.try_send(IncomingChannelMessage::Item(item));
                } else {
                    tracing::trace!(
                        conn_id = self.sender.connection_id().0,
                        channel_id = chan_id.0,
                        registered = false,
                        "driver buffered channel item before registration"
                    );
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
                    tracing::trace!(
                        conn_id = self.sender.connection_id().0,
                        channel_id = chan_id.0,
                        registered = true,
                        "driver received channel close"
                    );
                    let close = msg.map(|m| match m.body {
                        ChannelBody::Close(close) => close,
                        _ => unreachable!(),
                    });
                    let _ = tx.try_send(IncomingChannelMessage::Close(close));
                } else {
                    tracing::trace!(
                        conn_id = self.sender.connection_id().0,
                        channel_id = chan_id.0,
                        registered = false,
                        "driver buffered channel close before registration"
                    );
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
                    tracing::trace!(
                        conn_id = self.sender.connection_id().0,
                        channel_id = chan_id.0,
                        registered = true,
                        "driver received channel reset"
                    );
                    let reset = msg.map(|m| match m.body {
                        ChannelBody::Reset(reset) => reset,
                        _ => unreachable!(),
                    });
                    let _ = tx.try_send(IncomingChannelMessage::Reset(reset));
                } else {
                    tracing::trace!(
                        conn_id = self.sender.connection_id().0,
                        channel_id = chan_id.0,
                        registered = false,
                        "driver buffered channel reset before registration"
                    );
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
                tracing::trace!(
                    conn_id = self.sender.connection_id().0,
                    channel_id = chan_id.0,
                    additional = grant.additional,
                    "driver received channel credit"
                );
                if let Some(semaphore) = self.shared.channel_credits.lock().get(&chan_id) {
                    semaphore.add_permits(grant.additional as usize);
                }
            }
        }
    }
}
