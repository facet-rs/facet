use std::{
    collections::{BTreeMap, HashMap, HashSet},
    pin::Pin,
    sync::{
        Arc, Weak,
        atomic::{AtomicU64, Ordering},
    },
    time::Instant,
};

use futures_util::future::{AbortHandle, Abortable};
use futures_util::stream::{FuturesUnordered, StreamExt as _};
use moire::sync::{Semaphore, SyncMutex};
use tokio::sync::watch;

use moire::task::FutureExt as _;
use vox_types::{
    BoxFut, CallResult, ChannelBinder, ChannelBody, ChannelClose, ChannelCreditReplenisher,
    ChannelCreditReplenisherHandle, ChannelEventContext, ChannelId, ChannelItem,
    ChannelLivenessHandle, ChannelMessage, ChannelRetryMode, ChannelSink, ConnectionId, CreditSink,
    Handler, IdAllocator, IncomingChannelMessage, MaybeSend, MaybeSync, Payload, ReplySink,
    RequestBody, RequestCall, RequestId, RequestMessage, RequestResponse, SelfRef, TrySendError,
    TxError, VoxError, ensure_operation_id, metadata_channel_retry_mode, metadata_operation_id,
};
use vox_types::{
    ChannelCloseReason, ChannelDebugContext, ChannelDirection, ChannelEvent, ChannelResetReason,
    ChannelSendOutcome, ChannelTrySendOutcome, DriverEvent, RpcOutcome, VoxObserverHandle,
};
use vox_types::{
    ChannelDebugSnapshot, ChannelReceiverState, ConnectionCloseReason, ConnectionDebugSnapshot,
    ConnectionDebugState, DriverTaskStatus, RequestDebugSnapshot, RequestDebugState,
    VoxDebugSnapshot,
};

use crate::session::{
    ConnectionHandle, ConnectionMessage, ConnectionSender, DropControlRequest, FailureDisposition,
};
use crate::{InMemoryOperationStore, OperationStore};
use moire::sync::mpsc;
use vox_types::{OperationId, PostcardPayload};

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
    /// Aborts the handler future hosted on `Driver::handler_futs`. Triggered
    /// by `Cancel`-style flows; the FuturesUnordered will yield an `Aborted`
    /// item on its next poll, and the request will be removed from
    /// `in_flight_handlers` (if not already gone).
    abort: AbortHandle,
    method_id: vox_types::MethodId,
    retry: vox_types::RetryPolicy,
    operation_id: Option<OperationId>,
}

/// Boxed handler future hosted on `Driver::handler_futs`. The future yields
/// the `RequestId` it was attached to so the driver can clean up the
/// `in_flight_handlers` entry when it completes.
///
/// We `Box::pin` because the handler returns an unnameable `async move {}`
/// and we want `FuturesUnordered` to hold a single concrete element type.
/// Total alloc footprint per request is one `Box<dyn Future>` plus one
/// `Arc<Task>` (allocated by `FuturesUnordered::push`). Compared to
/// `tokio::spawn` (which allocates a `Cell<T, S>` containing
/// `Stage<Future, Output>` plus does scheduler registration), this drops
/// the `Stage` overhead and the `set_stage` memcpy that fires on
/// `Running → Finished` transitions.
type HandlerFut = Abortable<Pin<Box<dyn Future<Output = RequestId> + Send + 'static>>>;

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

#[derive(Clone)]
struct RequestRuntimeDebug {
    method_id: vox_types::MethodId,
    service: Option<&'static str>,
    method: Option<&'static str>,
    started_at: Instant,
    state: RequestDebugState,
    response_sender_blocked: Option<bool>,
    associated_channels: Vec<ChannelId>,
}

impl RequestRuntimeDebug {
    fn snapshot(&self, request_id: RequestId, now: Instant) -> RequestDebugSnapshot {
        RequestDebugSnapshot {
            request_id,
            service: self.service,
            method: self.method,
            method_id: self.method_id,
            age: now.saturating_duration_since(self.started_at),
            state: self.state,
            response_sender_blocked: self.response_sender_blocked,
            associated_channels: self.associated_channels.clone(),
        }
    }
}

#[derive(Clone)]
struct ChannelRuntimeDebug {
    direction: ChannelDirection,
    debug: Option<ChannelDebugContext>,
    initial_credit: u32,
    inbound_queue_len: usize,
    inbound_queue_capacity: Option<usize>,
    receiver_state: ChannelReceiverState,
    last_item_sent_at: Option<Instant>,
    last_item_received_at: Option<Instant>,
    last_item_consumed_at: Option<Instant>,
    last_credit_granted_at: Option<Instant>,
    last_credit_received_at: Option<Instant>,
    last_credit_granted_amount: Option<u32>,
    last_credit_received_amount: Option<u32>,
    pending_local_grant_credit: u32,
    total_credit_granted: u64,
    total_credit_received: u64,
    sent: u64,
    sends_started: u64,
    sends_completed: u64,
    sends_waited_for_credit: u64,
    try_send_full_credit: u64,
    try_send_full_runtime_queue: u64,
    closed: u64,
    reset: u64,
    dropped: u64,
    items_received: u64,
    items_consumed: u64,
    credit_granted: u64,
    credit_received: u64,
    close_reason: Option<ChannelCloseReason>,
    reset_reason: Option<ChannelResetReason>,
}

impl ChannelRuntimeDebug {
    fn new(
        direction: ChannelDirection,
        initial_credit: u32,
        debug: Option<ChannelDebugContext>,
    ) -> Self {
        Self {
            direction,
            debug,
            initial_credit,
            inbound_queue_len: 0,
            inbound_queue_capacity: match direction {
                ChannelDirection::Rx => Some(initial_credit as usize),
                ChannelDirection::Tx => None,
            },
            receiver_state: ChannelReceiverState::Present,
            last_item_sent_at: None,
            last_item_received_at: None,
            last_item_consumed_at: None,
            last_credit_granted_at: None,
            last_credit_received_at: None,
            last_credit_granted_amount: None,
            last_credit_received_amount: None,
            pending_local_grant_credit: 0,
            total_credit_granted: 0,
            total_credit_received: 0,
            sent: 0,
            sends_started: 0,
            sends_completed: 0,
            sends_waited_for_credit: 0,
            try_send_full_credit: 0,
            try_send_full_runtime_queue: 0,
            closed: 0,
            reset: 0,
            dropped: 0,
            items_received: 0,
            items_consumed: 0,
            credit_granted: 0,
            credit_received: 0,
            close_reason: None,
            reset_reason: None,
        }
    }

    fn merge_debug(&mut self, debug: Option<ChannelDebugContext>) {
        if self.debug.is_none() {
            self.debug = debug;
        }
    }

    fn mark_item_received(&mut self, now: Instant) {
        self.items_received = self.items_received.saturating_add(1);
        self.inbound_queue_len = self.inbound_queue_len.saturating_add(1);
        self.last_item_received_at = Some(now);
    }

    fn mark_closed(&mut self, reason: ChannelCloseReason) {
        self.closed = self.closed.saturating_add(1);
        self.close_reason = Some(reason);
        self.receiver_state = ChannelReceiverState::Closed;
        if reason == ChannelCloseReason::Dropped {
            self.dropped = self.dropped.saturating_add(1);
            self.receiver_state = ChannelReceiverState::Dropped;
        }
    }

    fn mark_reset(&mut self, reason: ChannelResetReason) {
        self.reset = self.reset.saturating_add(1);
        self.reset_reason = Some(reason);
        self.receiver_state = ChannelReceiverState::Reset;
    }

    fn mark_send_started(&mut self) {
        self.sends_started = self.sends_started.saturating_add(1);
    }

    fn mark_send_waiting_for_credit(&mut self) {
        self.sends_waited_for_credit = self.sends_waited_for_credit.saturating_add(1);
    }

    fn mark_send_finished(&mut self, outcome: ChannelSendOutcome, now: Instant) {
        self.sends_completed = self.sends_completed.saturating_add(1);
        if outcome == ChannelSendOutcome::Sent {
            self.sent = self.sent.saturating_add(1);
            self.last_item_sent_at = Some(now);
        }
    }

    fn mark_try_send_outcome(&mut self, outcome: ChannelTrySendOutcome, now: Instant) {
        match outcome {
            ChannelTrySendOutcome::Sent => {
                self.sent = self.sent.saturating_add(1);
                self.last_item_sent_at = Some(now);
            }
            ChannelTrySendOutcome::FullCredit => {
                self.try_send_full_credit = self.try_send_full_credit.saturating_add(1);
            }
            ChannelTrySendOutcome::FullRuntimeQueue => {
                self.try_send_full_runtime_queue =
                    self.try_send_full_runtime_queue.saturating_add(1);
            }
            ChannelTrySendOutcome::Unbound | ChannelTrySendOutcome::Closed => {}
        }
    }

    fn mark_item_consumed(&mut self, now: Instant) {
        self.items_consumed = self.items_consumed.saturating_add(1);
        self.inbound_queue_len = self.inbound_queue_len.saturating_sub(1);
        self.last_item_consumed_at = Some(now);
    }

    fn mark_inbound_item_not_enqueued(&mut self) {
        self.inbound_queue_len = self.inbound_queue_len.saturating_sub(1);
    }

    fn mark_credit_granted(&mut self, amount: u32, now: Instant) {
        self.credit_granted = self.credit_granted.saturating_add(1);
        self.total_credit_granted = self.total_credit_granted.saturating_add(amount as u64);
        self.last_credit_granted_at = Some(now);
        self.last_credit_granted_amount = Some(amount);
        self.pending_local_grant_credit = 0;
    }

    fn mark_credit_received(&mut self, amount: u32, now: Instant) {
        self.credit_received = self.credit_received.saturating_add(1);
        self.total_credit_received = self.total_credit_received.saturating_add(amount as u64);
        self.last_credit_received_at = Some(now);
        self.last_credit_received_amount = Some(amount);
    }

    fn mark_receiver_dropped(&mut self) {
        self.reset = self.reset.saturating_add(1);
        self.reset_reason = Some(ChannelResetReason::ReceiverDropped);
        self.receiver_state = ChannelReceiverState::Dropped;
        self.dropped = self.dropped.saturating_add(1);
    }

    fn snapshot(
        &self,
        connection_id: ConnectionId,
        channel_id: ChannelId,
        available_send_credit: Option<u32>,
    ) -> ChannelDebugSnapshot {
        ChannelDebugSnapshot {
            connection_id,
            channel_id,
            direction: self.direction,
            debug: self.debug,
            initial_credit: self.initial_credit,
            available_send_credit,
            inbound_queue_len: Some(self.inbound_queue_len),
            inbound_queue_capacity: self.inbound_queue_capacity,
            outbound_runtime_queue_len: None,
            outbound_runtime_queue_capacity: None,
            send_waiters_count: None,
            receiver_state: self.receiver_state,
            last_item_sent_at: self.last_item_sent_at,
            last_item_received_at: self.last_item_received_at,
            last_item_consumed_at: self.last_item_consumed_at,
            last_credit_granted_at: self.last_credit_granted_at,
            last_credit_received_at: self.last_credit_received_at,
            last_credit_granted_amount: self.last_credit_granted_amount,
            last_credit_received_amount: self.last_credit_received_amount,
            pending_local_grant_credit: self.pending_local_grant_credit,
            total_credit_granted: self.total_credit_granted,
            total_credit_received: self.total_credit_received,
            current_permit_count: available_send_credit,
            zero_credit_with_blocked_senders: available_send_credit == Some(0)
                && self.sends_waited_for_credit > 0,
            sent: self.sent,
            sends_started: self.sends_started,
            sends_completed: self.sends_completed,
            sends_waited_for_credit: self.sends_waited_for_credit,
            try_send_full_credit: self.try_send_full_credit,
            try_send_full_runtime_queue: self.try_send_full_runtime_queue,
            closed: self.closed,
            reset: self.reset,
            dropped: self.dropped,
            items_received: self.items_received,
            items_consumed: self.items_consumed,
            credit_granted: self.credit_granted,
            credit_received: self.credit_received,
            close_reason: self.close_reason,
            reset_reason: self.reset_reason,
        }
    }
}

/// State shared between the driver loop and any `DriverCaller` / `DriverChannelSink` handles.
///
/// `pending_responses` is keyed by request ID and therefore tracks live
/// request attempts, not logical operations.
struct DriverShared {
    connection_id: ConnectionId,
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
    // r[impl rpc.observability.channel.context]
    channel_contexts: SyncMutex<BTreeMap<ChannelId, ChannelDebugContext>>,
    // r[impl rpc.debug.snapshot]
    request_debug: SyncMutex<BTreeMap<RequestId, RequestRuntimeDebug>>,
    // r[impl rpc.debug.snapshot]
    channel_debug: SyncMutex<BTreeMap<ChannelId, ChannelRuntimeDebug>>,
    last_inbound_message_at: SyncMutex<Option<Instant>>,
    last_outbound_message_at: SyncMutex<Option<Instant>>,
    close_reason: SyncMutex<Option<ConnectionCloseReason>>,
    /// Channel IDs that have reached a terminal local state. Once a channel is
    /// closed/reset, outbound sinks must reject further sends and inbound items
    /// must not be buffered forever.
    terminal_channels: SyncMutex<HashSet<ChannelId>>,
    /// Channel IDs cleared during session resume. When handler tasks that owned
    /// these channels are aborted, they may trigger `close_channel_on_drop`, which
    /// would send a ChannelClose message for a channel the peer no longer knows about.
    /// We suppress those Close messages by checking this set.
    stale_close_channels: SyncMutex<std::collections::HashSet<ChannelId>>,
    // r[impl rpc.flow-control.credit.initial]
    local_initial_channel_credit: u32,
    // r[impl rpc.flow-control.credit.initial]
    peer_initial_channel_credit: u32,
    observer: Option<VoxObserverHandle>,
}

impl DriverShared {
    fn remember_channel_context(
        &self,
        channel_id: ChannelId,
        debug_context: Option<ChannelDebugContext>,
    ) {
        if let Some(debug_context) = debug_context.and_then(ChannelDebugContext::into_option) {
            self.channel_contexts
                .lock()
                .insert(channel_id, debug_context);
            if let Some(channel) = self.channel_debug.lock().get_mut(&channel_id) {
                channel.debug = Some(debug_context);
            }
        }
    }

    fn channel_event_context(
        &self,
        channel_id: ChannelId,
        debug_context: Option<ChannelDebugContext>,
    ) -> ChannelEventContext {
        let debug = debug_context
            .and_then(ChannelDebugContext::into_option)
            .or_else(|| self.channel_contexts.lock().get(&channel_id).copied());
        ChannelEventContext {
            connection_id: Some(self.connection_id),
            channel_id,
            debug,
        }
    }

    fn emit_channel_event(
        &self,
        channel_id: ChannelId,
        debug_context: Option<ChannelDebugContext>,
        event: impl FnOnce(ChannelEventContext) -> ChannelEvent,
    ) {
        if let Some(observer) = &self.observer {
            observer.channel_event(event(self.channel_event_context(channel_id, debug_context)));
        }
    }

    fn observe_channel(
        &self,
        channel_id: ChannelId,
        debug_context: Option<ChannelDebugContext>,
        event: impl FnOnce(ChannelEventContext) -> ChannelEvent,
    ) {
        let event = event(self.channel_event_context(channel_id, debug_context));
        self.record_channel_event(event);
        if let Some(observer) = &self.observer {
            observer.channel_event(event);
        }
    }

    fn update_channel_debug(
        &self,
        channel: ChannelEventContext,
        default_direction: ChannelDirection,
        default_initial_credit: u32,
        update: impl FnOnce(&mut ChannelRuntimeDebug),
    ) {
        let mut channels = self.channel_debug.lock();
        let entry = channels.entry(channel.channel_id).or_insert_with(|| {
            ChannelRuntimeDebug::new(default_direction, default_initial_credit, channel.debug)
        });
        entry.merge_debug(channel.debug);
        update(entry);
    }

    fn update_existing_channel_debug(
        &self,
        channel_id: ChannelId,
        update: impl FnOnce(&mut ChannelRuntimeDebug),
    ) {
        if let Some(channel) = self.channel_debug.lock().get_mut(&channel_id) {
            update(channel);
        }
    }

    fn record_channel_event(&self, event: ChannelEvent) {
        let now = Instant::now();
        match event {
            ChannelEvent::Opened {
                channel,
                direction,
                initial_credit,
            } => {
                self.channel_debug.lock().insert(
                    channel.channel_id,
                    ChannelRuntimeDebug::new(direction, initial_credit, channel.debug),
                );
            }
            ChannelEvent::ItemReceived { channel } => {
                self.update_channel_debug(channel, ChannelDirection::Rx, 0, |entry| {
                    entry.mark_item_received(now);
                });
            }
            ChannelEvent::Closed { channel, reason } => {
                self.update_channel_debug(channel, ChannelDirection::Rx, 0, |entry| {
                    entry.mark_closed(reason);
                });
            }
            ChannelEvent::Reset { channel, reason } => {
                self.update_channel_debug(channel, ChannelDirection::Rx, 0, |entry| {
                    entry.mark_reset(reason);
                });
            }
            ChannelEvent::CreditGranted { channel, amount } => {
                self.record_credit_granted_at(channel.channel_id, amount, now);
            }
            ChannelEvent::SendStarted { channel } => {
                self.record_send_started(channel.channel_id);
            }
            ChannelEvent::SendWaitingForCredit { channel } => {
                self.record_send_waiting_for_credit(channel.channel_id);
            }
            ChannelEvent::SendFinished {
                channel, outcome, ..
            } => {
                self.record_send_finished(channel.channel_id, outcome);
            }
            ChannelEvent::TrySend { channel, outcome } => {
                self.record_try_send_outcome(channel.channel_id, outcome);
            }
            ChannelEvent::ItemConsumed { channel } => {
                self.record_item_consumed(channel.channel_id);
            }
        }
    }

    fn mark_inbound_progress(&self) {
        *self.last_inbound_message_at.lock() = Some(Instant::now());
    }

    fn mark_outbound_progress(&self) {
        *self.last_outbound_message_at.lock() = Some(Instant::now());
    }

    fn start_request(
        &self,
        request_id: RequestId,
        method_id: vox_types::MethodId,
        service: Option<&'static str>,
        method: Option<&'static str>,
        state: RequestDebugState,
    ) {
        self.request_debug.lock().insert(
            request_id,
            RequestRuntimeDebug {
                method_id,
                service,
                method,
                started_at: Instant::now(),
                state,
                response_sender_blocked: Some(false),
                associated_channels: Vec::new(),
            },
        );
    }

    fn finish_request(&self, request_id: RequestId, state: RequestDebugState) {
        if let Some(request) = self.request_debug.lock().get_mut(&request_id) {
            request.state = state;
        }
        self.request_debug.lock().remove(&request_id);
    }

    fn record_send_started(&self, channel_id: ChannelId) {
        self.update_existing_channel_debug(channel_id, ChannelRuntimeDebug::mark_send_started);
    }

    fn record_send_waiting_for_credit(&self, channel_id: ChannelId) {
        self.update_existing_channel_debug(
            channel_id,
            ChannelRuntimeDebug::mark_send_waiting_for_credit,
        );
    }

    fn record_send_finished(&self, channel_id: ChannelId, outcome: ChannelSendOutcome) {
        let now = Instant::now();
        self.update_existing_channel_debug(channel_id, |channel| {
            channel.mark_send_finished(outcome, now);
        });
    }

    fn record_try_send_outcome(&self, channel_id: ChannelId, outcome: ChannelTrySendOutcome) {
        let now = Instant::now();
        self.update_existing_channel_debug(channel_id, |channel| {
            channel.mark_try_send_outcome(outcome, now);
        });
    }

    fn record_item_consumed(&self, channel_id: ChannelId) {
        let now = Instant::now();
        self.update_existing_channel_debug(channel_id, |channel| {
            channel.mark_item_consumed(now);
        });
    }

    fn record_inbound_item_not_enqueued(&self, channel_id: ChannelId) {
        self.update_existing_channel_debug(
            channel_id,
            ChannelRuntimeDebug::mark_inbound_item_not_enqueued,
        );
    }

    fn record_pending_local_grant(&self, channel_id: ChannelId, pending: u32) {
        self.update_existing_channel_debug(channel_id, |channel| {
            channel.pending_local_grant_credit = pending;
        });
    }

    fn record_credit_granted_at(&self, channel_id: ChannelId, amount: u32, now: Instant) {
        self.update_existing_channel_debug(channel_id, |channel| {
            channel.mark_credit_granted(amount, now);
        });
    }

    fn record_credit_received(&self, channel_id: ChannelId, amount: u32) {
        let now = Instant::now();
        self.update_existing_channel_debug(channel_id, |channel| {
            channel.mark_credit_received(amount, now);
        });
    }

    fn record_receiver_dropped(&self, channel_id: ChannelId) {
        self.update_existing_channel_debug(channel_id, ChannelRuntimeDebug::mark_receiver_dropped);
    }

    fn debug_snapshot(
        &self,
        sender: &ConnectionSender,
        state: ConnectionDebugState,
        driver_task_status: DriverTaskStatus,
    ) -> VoxDebugSnapshot {
        let now = Instant::now();
        let requests: Vec<_> = self
            .request_debug
            .lock()
            .iter()
            .map(|(request_id, request)| request.snapshot(*request_id, now))
            .collect();
        let credits = self.shared_channel_credit_snapshot();
        let open_channels: Vec<_> = self
            .channel_debug
            .lock()
            .iter()
            .map(|(channel_id, channel)| {
                channel.snapshot(
                    self.connection_id,
                    *channel_id,
                    credits.get(channel_id).copied().flatten(),
                )
            })
            .collect();
        let last_inbound_message_at = *self.last_inbound_message_at.lock();
        let last_outbound_message_at = *self.last_outbound_message_at.lock();
        let last_progress_at = match (last_inbound_message_at, last_outbound_message_at) {
            (Some(inbound), Some(outbound)) => Some(inbound.max(outbound)),
            (Some(inbound), None) => Some(inbound),
            (None, Some(outbound)) => Some(outbound),
            (None, None) => None,
        };
        let (outbound_queue_depth, outbound_queue_capacity) =
            sender.sess_core.outbound_queue_stats();
        VoxDebugSnapshot {
            connections: vec![ConnectionDebugSnapshot {
                connection_id: self.connection_id,
                endpoint: None,
                surface: None,
                component: None,
                state,
                outstanding_requests: requests.len(),
                requests,
                open_channels,
                outbound_queue_depth: Some(outbound_queue_depth),
                outbound_queue_capacity: Some(outbound_queue_capacity),
                local_control_queue_depth: None,
                local_control_queue_capacity: None,
                last_inbound_message_at,
                last_outbound_message_at,
                last_progress_at,
                close_reason: *self.close_reason.lock(),
                driver_task_status,
            }],
        }
    }

    fn shared_channel_credit_snapshot(&self) -> BTreeMap<ChannelId, Option<u32>> {
        self.channel_credits
            .lock()
            .iter()
            .map(|(channel_id, semaphore)| {
                (
                    *channel_id,
                    Some(semaphore.available_permits().min(u32::MAX as usize) as u32),
                )
            })
            .collect()
    }

    fn set_connection_closed(&self, reason: ConnectionCloseReason) {
        *self.close_reason.lock() = Some(reason);
    }

    fn connection_debug_state(&self, closed: bool) -> ConnectionDebugState {
        if closed {
            ConnectionDebugState::Closed
        } else {
            ConnectionDebugState::Open
        }
    }
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
    use vox_types::{ChannelCreditReplenisher, ChannelId};

    #[tokio::test]
    async fn replenisher_batches_at_half_the_initial_window() {
        let (tx, mut rx) = moire::sync::mpsc::unbounded_channel("test.replenisher");
        let replenisher = DriverChannelCreditReplenisher::new(
            vox_types::ConnectionId::ROOT,
            ChannelId(7),
            None,
            std::sync::Weak::new(),
            16,
            tx,
            None,
        );

        for _ in 0..7 {
            replenisher.on_item_consumed();
        }
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(20), rx.recv())
                .await
                .is_err(),
            "should not emit credit before reaching the batch threshold"
        );

        replenisher.on_item_consumed();
        let Some(DriverLocalControl::GrantCredit {
            channel_id,
            additional,
        }) = rx.recv().await
        else {
            panic!("expected batched credit grant");
        };
        assert_eq!(channel_id, ChannelId(7));
        assert_eq!(additional, 8);
    }

    #[tokio::test]
    async fn replenisher_grants_one_by_one_for_single_credit_windows() {
        let (tx, mut rx) = moire::sync::mpsc::unbounded_channel("test.replenisher.single");
        let replenisher = DriverChannelCreditReplenisher::new(
            vox_types::ConnectionId::ROOT,
            ChannelId(9),
            None,
            std::sync::Weak::new(),
            1,
            tx,
            None,
        );

        replenisher.on_item_consumed();
        let Some(DriverLocalControl::GrantCredit {
            channel_id,
            additional,
        }) = rx.recv().await
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
    /// Static `&'static Shape` of the method's response type. Used on
    /// replay to derive the schemas to attach to the wire response —
    /// the same source of truth that fresh responses use.
    handler_response_shape: Option<&'static facet_core::Shape>,
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
    response_shape: Option<&'static facet_core::Shape>,
) -> Result<(), ()> {
    let mut response: RequestResponse<'_> =
        vox_postcard::from_slice_borrowed(encoded_response).map_err(|_| ())?;
    if let Some(shape) = response_shape {
        sender.prepare_replay_schemas(request_id, method_id, shape, &mut response);
    } else {
        response.schemas = Default::default();
    }
    sender.send_response(request_id, response).await
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
        self.binder.shared.mark_outbound_progress();

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

            let schemas_for_wire = std::mem::take(&mut response.schemas);
            let encoded_bytes: Vec<u8> =
                vox_jit::encode!(&response).expect("JIT encode failed for response store");
            let encoded_for_store: PostcardPayload = encoded_bytes.into();
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

            // Seal: just the (op_id, method_id, bytes) tuple. No schemas
            // — they come from the running code at replay time.
            operations.seal(operation_id, self.method_id, &encoded_for_store);

            // Get waiters from the live tracker and replay to them.
            let waiters = self
                .live_operations
                .as_ref()
                .map(|lo| lo.lock().seal(operation_id))
                .unwrap_or_default();
            let response_shape = self.handler_response_shape;
            for waiter in waiters {
                if waiter == self.request_id {
                    continue;
                }
                if replay_sealed_response(
                    sender.clone(),
                    waiter,
                    self.method_id,
                    encoded_for_store.as_bytes(),
                    response_shape,
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
    shared: Arc<DriverShared>,
    channel_id: ChannelId,
    debug_context: Option<ChannelDebugContext>,
    local_control_tx: mpsc::UnboundedSender<DriverLocalControl>,
}

impl ChannelSink for DriverChannelSink {
    fn send_payload<'payload>(
        &self,
        payload: Payload<'payload>,
    ) -> Pin<Box<dyn vox_types::MaybeSendFuture<Output = Result<(), TxError>> + 'payload>> {
        let sender = self.sender.clone();
        let shared = Arc::clone(&self.shared);
        let channel_id = self.channel_id;
        Box::pin(async move {
            if shared.terminal_channels.lock().contains(&channel_id) {
                return Err(TxError::Transport("channel closed".into()));
            }

            shared.mark_outbound_progress();
            sender
                .send(ConnectionMessage::Channel(ChannelMessage {
                    id: channel_id,
                    body: ChannelBody::Item(ChannelItem { item: payload }),
                }))
                .await
                .map_err(|()| TxError::Transport("connection closed".into()))
        })
    }

    fn channel_id(&self) -> Option<ChannelId> {
        Some(self.channel_id)
    }

    fn connection_id(&self) -> Option<vox_types::ConnectionId> {
        Some(self.sender.connection_id())
    }

    fn debug_context(&self) -> Option<ChannelDebugContext> {
        self.debug_context
            .and_then(ChannelDebugContext::into_option)
            .or_else(|| {
                self.shared
                    .channel_contexts
                    .lock()
                    .get(&self.channel_id)
                    .copied()
            })
    }

    fn observer(&self) -> Option<VoxObserverHandle> {
        self.shared.observer.clone()
    }

    fn note_send_started(&self) {
        self.shared.record_send_started(self.channel_id);
    }

    fn note_send_waiting_for_credit(&self) {
        self.shared.record_send_waiting_for_credit(self.channel_id);
    }

    fn note_send_finished(&self, outcome: ChannelSendOutcome) {
        self.shared.record_send_finished(self.channel_id, outcome);
    }

    fn note_try_send_outcome(&self, outcome: ChannelTrySendOutcome) {
        self.shared
            .record_try_send_outcome(self.channel_id, outcome);
    }

    // r[impl rpc.flow-control.credit.try-send]
    // r[impl rpc.observability.channel.try-send-detail]
    fn try_send_payload_with_outcome<'payload>(
        &self,
        payload: Payload<'payload>,
    ) -> Result<(), ChannelTrySendOutcome> {
        if self
            .shared
            .terminal_channels
            .lock()
            .contains(&self.channel_id)
        {
            return Err(ChannelTrySendOutcome::Closed);
        }

        self.shared.mark_outbound_progress();
        self.sender
            .try_send(ConnectionMessage::Channel(ChannelMessage {
                id: self.channel_id,
                body: ChannelBody::Item(ChannelItem { item: payload }),
            }))
            .map_err(|err| match err {
                TrySendError::Closed(()) => ChannelTrySendOutcome::Closed,
                TrySendError::Full(()) => ChannelTrySendOutcome::FullRuntimeQueue,
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
        let shared = Arc::clone(&self.shared);
        let channel_id = self.channel_id;
        let debug_context = self.debug_context;
        Box::pin(async move {
            shared.terminal_channels.lock().insert(channel_id);
            shared.observe_channel(channel_id, debug_context, |channel| ChannelEvent::Closed {
                channel,
                reason: ChannelCloseReason::Local,
            });

            shared.mark_outbound_progress();
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
        self.shared.terminal_channels.lock().insert(self.channel_id);
        self.shared
            .observe_channel(self.channel_id, self.debug_context, |channel| {
                ChannelEvent::Closed {
                    channel,
                    reason: ChannelCloseReason::Dropped,
                }
            });
        let _ = self
            .local_control_tx
            .send(DriverLocalControl::CloseChannel {
                channel_id: self.channel_id,
            });
    }
}

/// Object-safe version of [`Handler<DriverReplySink>`].
///
/// Boxes the future returned by `handle()` so the trait is dyn-safe.
/// Implemented automatically for any `Handler<DriverReplySink>`.
pub trait ErasedHandler: MaybeSend + MaybeSync + 'static {
    fn retry_policy(&self, method_id: vox_types::MethodId) -> vox_types::RetryPolicy {
        let _ = method_id;
        vox_types::RetryPolicy::VOLATILE
    }

    fn args_have_channels(&self, method_id: vox_types::MethodId) -> bool {
        let _ = method_id;
        false
    }

    fn response_wire_shape(&self, method_id: vox_types::MethodId) -> Option<&'static facet::Shape> {
        let _ = method_id;
        None
    }

    fn handle_erased(
        &self,
        call: SelfRef<RequestCall<'static>>,
        reply: DriverReplySink,
        schemas: std::sync::Arc<vox_types::SchemaRecvTracker>,
    ) -> BoxFut<'_, ()>;
}

impl<H: Handler<DriverReplySink>> ErasedHandler for H {
    fn retry_policy(&self, method_id: vox_types::MethodId) -> vox_types::RetryPolicy {
        Handler::retry_policy(self, method_id)
    }

    fn args_have_channels(&self, method_id: vox_types::MethodId) -> bool {
        Handler::args_have_channels(self, method_id)
    }

    fn response_wire_shape(&self, method_id: vox_types::MethodId) -> Option<&'static facet::Shape> {
        Handler::response_wire_shape(self, method_id)
    }

    fn handle_erased(
        &self,
        call: SelfRef<RequestCall<'static>>,
        reply: DriverReplySink,
        schemas: std::sync::Arc<vox_types::SchemaRecvTracker>,
    ) -> BoxFut<'_, ()> {
        Box::pin(Handler::handle(self, call, reply, schemas))
    }
}

impl Handler<DriverReplySink> for Box<dyn ErasedHandler> {
    fn retry_policy(&self, method_id: vox_types::MethodId) -> vox_types::RetryPolicy {
        (**self).retry_policy(method_id)
    }

    fn args_have_channels(&self, method_id: vox_types::MethodId) -> bool {
        (**self).args_have_channels(method_id)
    }

    fn response_wire_shape(&self, method_id: vox_types::MethodId) -> Option<&'static facet::Shape> {
        (**self).response_wire_shape(method_id)
    }

    async fn handle(
        &self,
        call: SelfRef<RequestCall<'static>>,
        reply: DriverReplySink,
        schemas: std::sync::Arc<vox_types::SchemaRecvTracker>,
    ) {
        (**self).handle_erased(call, reply, schemas).await
    }
}

/// Concrete caller type wrapping a [`DriverCaller`] with optional middleware.
///
/// This is the primary type for making outbound RPC calls. Generated `*Client`
/// types store a `Caller` as a public field. Use `with_middleware()` to add
/// client middleware to the call chain.
#[must_use = "Dropping this caller may close the connection if it is the last caller."]
#[derive(Clone)]
pub struct Caller {
    inner: Arc<DriverCaller>,
    service: Option<&'static vox_types::ServiceDescriptor>,
    middlewares: Vec<Arc<dyn vox_types::ClientMiddleware>>,
}

impl Caller {
    /// Create a new `Caller` wrapping a [`DriverCaller`].
    pub fn new(driver: DriverCaller) -> Self {
        Self {
            inner: Arc::new(driver),
            service: None,
            middlewares: vec![],
        }
    }

    /// Access the underlying [`DriverCaller`] for low-level operations.
    #[cfg(test)]
    pub(crate) fn driver(&self) -> &DriverCaller {
        &self.inner
    }

    /// Append a client middleware to this caller's chain.
    pub fn with_middleware(
        mut self,
        service: &'static vox_types::ServiceDescriptor,
        middleware: impl vox_types::ClientMiddleware,
    ) -> Self {
        if let Some(existing_service) = self.service {
            assert_eq!(
                existing_service.service_name, service.service_name,
                "Caller middleware service mismatch"
            );
        } else {
            self.service = Some(service);
        }
        self.middlewares.push(Arc::new(middleware));
        self
    }

    /// Start one outgoing request attempt and wait for its response,
    /// running any registered middleware around the call.
    pub async fn call(&self, mut call: RequestCall<'_>) -> CallResult {
        use vox_types::{
            ClientCallOutcome, ClientContext, ClientRequest, Extensions, OwnedMetadata,
        };

        let Some(service) = self.service else {
            return self.inner.call_inner(call, None).await;
        };

        let extensions = Extensions::new();
        let method = service.by_id(call.method_id);
        let context = ClientContext::new(method, call.method_id, &extensions);
        let mut owned_metadata = OwnedMetadata::default();

        if !self.middlewares.is_empty() {
            for middleware in &self.middlewares {
                let mut request = ClientRequest::new(&mut call, &mut owned_metadata);
                middleware.pre(&context, &mut request).await;
            }
        }

        let request_debug = method.map(|method| (method.service_name, method.method_name));
        let result = self.inner.call_inner(call, request_debug).await;
        if !self.middlewares.is_empty() {
            let outcome = match &result {
                Ok(_) => ClientCallOutcome::Response,
                Err(error) => ClientCallOutcome::Error(error),
            };
            for middleware in self.middlewares.iter().rev() {
                middleware.post(&context, outcome).await;
            }
        }
        result
    }

    /// Resolve when the underlying connection closes.
    pub async fn closed(&self) {
        if *self.inner.closed_rx.borrow() {
            return;
        }
        let mut rx = self.inner.closed_rx.clone();
        while rx.changed().await.is_ok() {
            if *rx.borrow() {
                return;
            }
        }
    }

    /// Return whether the underlying connection is still considered connected.
    pub fn is_connected(&self) -> bool {
        !*self.inner.closed_rx.borrow()
    }

    /// Return a channel binder for binding Tx/Rx handles in args before sending.
    pub fn channel_binder(&self) -> Option<&dyn ChannelBinder> {
        Some(self.inner.as_ref())
    }

    // r[impl rpc.debug.snapshot]
    pub fn debug_snapshot(&self) -> VoxDebugSnapshot {
        self.inner.debug_snapshot()
    }

    pub fn dump_debug_snapshot(&self) -> VoxDebugSnapshot {
        let snapshot = self.debug_snapshot();
        tracing::info!(?snapshot, "vox debug snapshot");
        snapshot
    }
}

/// Trait for constructing a typed client from a vox session.
///
/// Generated `*Client` types implement this to receive both the caller
/// and an optional session handle. Root connections pass `Some(handle)`;
/// virtual connections pass `None`.
pub trait FromVoxSession {
    /// The service name for this client, used for automatic `vox-service` metadata.
    /// Generated clients return `Some("ServiceName")`. `NoopClient` returns `None`.
    const SERVICE_NAME: &'static str;

    fn from_vox_session(
        caller: Caller,
        session_handle: Option<crate::session::SessionHandle>,
    ) -> Self;
}

/// Liveness-only client for a connection root.
///
/// Keeps the root connection alive but intentionally exposes no outbound RPC API.
/// Use this as the type parameter to `establish()` when you don't need a typed client.
#[must_use = "Dropping NoopClient may close the connection if it is the last caller."]
#[derive(Clone)]
pub struct NoopClient {
    /// The underlying caller keeping the connection alive.
    pub caller: Caller,
    /// The session handle, if this client is on a root connection.
    pub session: Option<crate::session::SessionHandle>,
}

impl FromVoxSession for NoopClient {
    const SERVICE_NAME: &'static str = "Noop";

    fn from_vox_session(caller: Caller, session: Option<crate::session::SessionHandle>) -> Self {
        Self { caller, session }
    }
}

#[derive(Clone)]
struct DriverChannelBinder {
    sender: ConnectionSender,
    shared: Arc<DriverShared>,
    local_control_tx: mpsc::UnboundedSender<DriverLocalControl>,
    drop_guard: Option<Arc<CallerDropGuard>>,
}

fn register_rx_channel_impl(
    shared: &Arc<DriverShared>,
    channel_id: ChannelId,
    queue_name: &'static str,
    initial_channel_credit: u32,
    debug_context: Option<ChannelDebugContext>,
    liveness: Option<ChannelLivenessHandle>,
    local_control_tx: mpsc::UnboundedSender<DriverLocalControl>,
) -> vox_types::BoundChannelReceiver {
    observe_channel_opened(
        shared,
        channel_id,
        ChannelDirection::Rx,
        initial_channel_credit,
        debug_context,
    );
    let (tx, rx) = mpsc::channel(queue_name, initial_channel_credit as usize);

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
            shared.connection_id,
            channel_id,
            debug_context,
            Arc::downgrade(shared),
            initial_channel_credit,
            local_control_tx,
            shared.observer.clone(),
        )) as ChannelCreditReplenisherHandle),
    }
}

// r[impl rpc.observability.channel]
fn observe_channel_opened(
    shared: &DriverShared,
    channel_id: ChannelId,
    direction: ChannelDirection,
    initial_credit: u32,
    debug_context: Option<ChannelDebugContext>,
) {
    shared.remember_channel_context(channel_id, debug_context);
    shared.observe_channel(channel_id, debug_context, |channel| ChannelEvent::Opened {
        channel,
        direction,
        initial_credit,
    });
}

fn make_tx_channel_sink(
    sender: &ConnectionSender,
    shared: &Arc<DriverShared>,
    local_control_tx: &mpsc::UnboundedSender<DriverLocalControl>,
    channel_id: ChannelId,
    debug_context: Option<ChannelDebugContext>,
) -> Arc<CreditSink<DriverChannelSink>> {
    observe_channel_opened(
        shared,
        channel_id,
        ChannelDirection::Tx,
        shared.peer_initial_channel_credit,
        debug_context,
    );
    let inner = DriverChannelSink {
        sender: sender.clone(),
        shared: Arc::clone(shared),
        channel_id,
        debug_context: debug_context.and_then(ChannelDebugContext::into_option),
        local_control_tx: local_control_tx.clone(),
    };
    let sink = Arc::new(CreditSink::new(inner, shared.peer_initial_channel_credit));
    shared
        .channel_credits
        .lock()
        .insert(channel_id, Arc::clone(sink.credit()));
    sink
}

trait DriverChannelEndpoint {
    fn endpoint_sender(&self) -> &ConnectionSender;
    fn endpoint_shared(&self) -> &Arc<DriverShared>;
    fn endpoint_local_control_tx(&self) -> &mpsc::UnboundedSender<DriverLocalControl>;
    fn endpoint_liveness(&self) -> Option<ChannelLivenessHandle>;
    fn endpoint_rx_queue_name(&self) -> &'static str;

    fn create_tx_credit_sink(
        &self,
        debug_context: Option<ChannelDebugContext>,
    ) -> (ChannelId, Arc<CreditSink<DriverChannelSink>>) {
        let shared = self.endpoint_shared();
        let channel_id = shared.channel_ids.lock().alloc();
        let sink = make_tx_channel_sink(
            self.endpoint_sender(),
            shared,
            self.endpoint_local_control_tx(),
            channel_id,
            debug_context,
        );
        (channel_id, sink)
    }

    fn create_tx_dyn(
        &self,
        debug_context: Option<ChannelDebugContext>,
    ) -> (ChannelId, Arc<dyn ChannelSink>) {
        let (id, sink) = self.create_tx_credit_sink(debug_context);
        (id, sink as Arc<dyn ChannelSink>)
    }

    fn create_rx_bound(
        &self,
        debug_context: Option<ChannelDebugContext>,
    ) -> (ChannelId, vox_types::BoundChannelReceiver) {
        let channel_id = self.endpoint_shared().channel_ids.lock().alloc();
        let rx = self.register_rx_bound(channel_id, debug_context);
        (channel_id, rx)
    }

    fn bind_tx_dyn(
        &self,
        channel_id: ChannelId,
        debug_context: Option<ChannelDebugContext>,
    ) -> Arc<dyn ChannelSink> {
        make_tx_channel_sink(
            self.endpoint_sender(),
            self.endpoint_shared(),
            self.endpoint_local_control_tx(),
            channel_id,
            debug_context,
        )
    }

    fn register_rx_bound(
        &self,
        channel_id: ChannelId,
        debug_context: Option<ChannelDebugContext>,
    ) -> vox_types::BoundChannelReceiver {
        let shared = self.endpoint_shared();
        register_rx_channel_impl(
            shared,
            channel_id,
            self.endpoint_rx_queue_name(),
            shared.local_initial_channel_credit,
            debug_context,
            self.endpoint_liveness(),
            self.endpoint_local_control_tx().clone(),
        )
    }
}

impl DriverChannelEndpoint for DriverChannelBinder {
    fn endpoint_sender(&self) -> &ConnectionSender {
        &self.sender
    }

    fn endpoint_shared(&self) -> &Arc<DriverShared> {
        &self.shared
    }

    fn endpoint_local_control_tx(&self) -> &mpsc::UnboundedSender<DriverLocalControl> {
        &self.local_control_tx
    }

    fn endpoint_liveness(&self) -> Option<ChannelLivenessHandle> {
        self.drop_guard
            .as_ref()
            .map(|guard| guard.clone() as ChannelLivenessHandle)
    }

    fn endpoint_rx_queue_name(&self) -> &'static str {
        "driver.register_rx_channel"
    }
}

impl ChannelBinder for DriverChannelBinder {
    fn create_tx(&self) -> (ChannelId, Arc<dyn ChannelSink>) {
        self.create_tx_dyn(None)
    }

    fn create_tx_with_context(
        &self,
        debug_context: Option<ChannelDebugContext>,
    ) -> (ChannelId, Arc<dyn ChannelSink>) {
        self.create_tx_dyn(debug_context)
    }

    fn create_rx(&self) -> (ChannelId, vox_types::BoundChannelReceiver) {
        self.create_rx_bound(None)
    }

    fn create_rx_with_context(
        &self,
        debug_context: Option<ChannelDebugContext>,
    ) -> (ChannelId, vox_types::BoundChannelReceiver) {
        self.create_rx_bound(debug_context)
    }

    fn bind_tx(&self, channel_id: ChannelId) -> Arc<dyn ChannelSink> {
        self.bind_tx_dyn(channel_id, None)
    }

    fn bind_tx_with_context(
        &self,
        channel_id: ChannelId,
        debug_context: Option<ChannelDebugContext>,
    ) -> Arc<dyn ChannelSink> {
        self.bind_tx_dyn(channel_id, debug_context)
    }

    fn register_rx(&self, channel_id: ChannelId) -> vox_types::BoundChannelReceiver {
        self.register_rx_bound(channel_id, None)
    }

    fn register_rx_with_context(
        &self,
        channel_id: ChannelId,
        debug_context: Option<ChannelDebugContext>,
    ) -> vox_types::BoundChannelReceiver {
        self.register_rx_bound(channel_id, debug_context)
    }

    fn channel_liveness(&self) -> Option<ChannelLivenessHandle> {
        self.endpoint_liveness()
    }
}

/// Allocates a request ID, registers a response slot,
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
        self.create_tx_credit_sink(None)
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
        self.register_rx_bound(channel_id, None)
    }
}

impl DriverChannelEndpoint for DriverCaller {
    fn endpoint_sender(&self) -> &ConnectionSender {
        &self.sender
    }

    fn endpoint_shared(&self) -> &Arc<DriverShared> {
        &self.shared
    }

    fn endpoint_local_control_tx(&self) -> &mpsc::UnboundedSender<DriverLocalControl> {
        &self.local_control_tx
    }

    fn endpoint_liveness(&self) -> Option<ChannelLivenessHandle> {
        self._drop_guard
            .as_ref()
            .map(|guard| guard.clone() as ChannelLivenessHandle)
    }

    fn endpoint_rx_queue_name(&self) -> &'static str {
        "driver.caller.register_rx_channel"
    }
}

impl ChannelBinder for DriverCaller {
    fn create_tx(&self) -> (ChannelId, Arc<dyn ChannelSink>) {
        self.create_tx_dyn(None)
    }

    fn create_tx_with_context(
        &self,
        debug_context: Option<ChannelDebugContext>,
    ) -> (ChannelId, Arc<dyn ChannelSink>) {
        self.create_tx_dyn(debug_context)
    }

    fn create_rx(&self) -> (ChannelId, vox_types::BoundChannelReceiver) {
        self.create_rx_bound(None)
    }

    fn create_rx_with_context(
        &self,
        debug_context: Option<ChannelDebugContext>,
    ) -> (ChannelId, vox_types::BoundChannelReceiver) {
        self.create_rx_bound(debug_context)
    }

    fn bind_tx(&self, channel_id: ChannelId) -> Arc<dyn ChannelSink> {
        self.bind_tx_dyn(channel_id, None)
    }

    fn bind_tx_with_context(
        &self,
        channel_id: ChannelId,
        debug_context: Option<ChannelDebugContext>,
    ) -> Arc<dyn ChannelSink> {
        self.bind_tx_dyn(channel_id, debug_context)
    }

    fn register_rx(&self, channel_id: ChannelId) -> vox_types::BoundChannelReceiver {
        self.register_rx_bound(channel_id, None)
    }

    fn register_rx_with_context(
        &self,
        channel_id: ChannelId,
        debug_context: Option<ChannelDebugContext>,
    ) -> vox_types::BoundChannelReceiver {
        self.register_rx_bound(channel_id, debug_context)
    }

    fn channel_liveness(&self) -> Option<ChannelLivenessHandle> {
        self.endpoint_liveness()
    }
}

impl DriverCaller {
    // r[impl rpc.debug.snapshot]
    pub fn debug_snapshot(&self) -> VoxDebugSnapshot {
        self.shared.debug_snapshot(
            &self.sender,
            self.shared.connection_debug_state(*self.closed_rx.borrow()),
            if *self.closed_rx.borrow() {
                DriverTaskStatus::Dead
            } else {
                DriverTaskStatus::Alive
            },
        )
    }

    pub fn dump_debug_snapshot(&self) -> VoxDebugSnapshot {
        let snapshot = self.debug_snapshot();
        tracing::info!(?snapshot, "vox debug snapshot");
        snapshot
    }

    /// Internal: perform a single outbound RPC call attempt (no middleware).
    async fn call_inner(
        &self,
        mut call: RequestCall<'_>,
        request_debug: Option<(&'static str, &'static str)>,
    ) -> CallResult {
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
        let request_started_at = std::time::Instant::now();
        if let Some(observer) = &self.shared.observer {
            observer.driver_event(DriverEvent::RequestStarted {
                connection_id: self.sender.connection_id(),
                request_id: req_id,
                method_id: call.method_id,
            });
        }
        let finish_request = |outcome: RpcOutcome| {
            self.shared.finish_request(
                req_id,
                if outcome == RpcOutcome::Ok {
                    RequestDebugState::Finished
                } else {
                    RequestDebugState::Failed
                },
            );
            if let Some(observer) = &self.shared.observer {
                observer.driver_event(DriverEvent::RequestFinished {
                    connection_id: self.sender.connection_id(),
                    request_id: req_id,
                    outcome,
                    elapsed: request_started_at.elapsed(),
                });
            }
        };

        // Register the response slot before sending, so the driver can
        // route the response even if it arrives before we start awaiting.
        let (tx, rx) = moire::sync::oneshot::channel("driver.response");
        self.shared.pending_responses.lock().insert(req_id, tx);
        self.shared.start_request(
            req_id,
            call.method_id,
            request_debug.map(|(service, _)| service),
            request_debug.map(|(_, method)| method),
            RequestDebugState::WaitingForResponse,
        );

        // r[impl schema.exchange.caller]
        // r[impl schema.exchange.channels]
        // Schemas are attached by SessionCore::send() when it sees a Call
        // with Payload::Value — no separate prepare step needed.
        //
        // Channel binding happens during serialization via the thread-local
        // ChannelBinder — no post-hoc walk needed.
        self.shared.mark_outbound_progress();
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
            finish_request(RpcOutcome::SendFailed);
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
                            finish_request(RpcOutcome::Closed);
                            return Err(VoxError::ConnectionClosed);
                        }
                    }
                }
                changed = resumed_rx.changed(), if self.peer_supports_retry => {
                    vox_types::dlog!("[CALLER] resumed_rx fired");
                    if changed.is_err() {
                        self.shared.pending_responses.lock().remove(&req_id);
                        finish_request(RpcOutcome::Closed);
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
                            finish_request(RpcOutcome::Closed);
                            return Err(VoxError::SessionShutdown);
                        }
                    }
                    match metadata_channel_retry_mode(&call.metadata) {
                        ChannelRetryMode::NonIdem => {
                            self.shared.pending_responses.lock().remove(&req_id);
                            finish_request(RpcOutcome::Indeterminate);
                            return Err(VoxError::Indeterminate);
                        }
                        ChannelRetryMode::Idem | ChannelRetryMode::None => {}
                    }
                    // Re-send the request after resume.
                    // Channel binding is embedded in the serialized payload,
                    // so no separate re-binding step is needed.
                    self.shared.mark_outbound_progress();
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
                        finish_request(RpcOutcome::Closed);
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

        finish_request(RpcOutcome::Ok);
        Ok(vox_types::WithTracker {
            value: response,
            tracker: response_schemas,
        })
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
    /// In-flight server-side handlers, keyed by request ID. Holds the
    /// `AbortHandle` for the corresponding entry in `handler_futs`. Used to
    /// abort handlers on cancel.
    in_flight_handlers: BTreeMap<RequestId, InFlightHandler>,
    /// Handler futures driven directly by the driver's `run` loop instead
    /// of being `tokio::spawn`'d. One alloc per request (the `Box<dyn
    /// Future>` plus the `Arc<Task>` inside `FuturesUnordered::push`),
    /// versus the `tokio::spawn` path which allocates a `Cell<T, S>` and
    /// memcpy's `Stage<Future, Output>` on every state transition.
    handler_futs: FuturesUnordered<HandlerFut>,
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
    ResetChannel {
        channel_id: ChannelId,
    },
    GrantCredit {
        channel_id: ChannelId,
        additional: u32,
    },
}

struct DriverChannelCreditReplenisher {
    connection_id: ConnectionId,
    channel_id: ChannelId,
    debug_context: Option<ChannelDebugContext>,
    shared: Weak<DriverShared>,
    threshold: u32,
    local_control_tx: mpsc::UnboundedSender<DriverLocalControl>,
    observer: Option<VoxObserverHandle>,
    pending: std::sync::Mutex<u32>,
}

impl DriverChannelCreditReplenisher {
    fn new(
        connection_id: ConnectionId,
        channel_id: ChannelId,
        debug_context: Option<ChannelDebugContext>,
        shared: Weak<DriverShared>,
        initial_credit: u32,
        local_control_tx: mpsc::UnboundedSender<DriverLocalControl>,
        observer: Option<VoxObserverHandle>,
    ) -> Self {
        Self {
            connection_id,
            channel_id,
            debug_context,
            shared,
            threshold: (initial_credit / 2).max(1),
            local_control_tx,
            observer,
            pending: std::sync::Mutex::new(0),
        }
    }
}

impl ChannelCreditReplenisher for DriverChannelCreditReplenisher {
    fn on_item_consumed(&self) {
        let mut pending = self.pending.lock().expect("pending credit mutex poisoned");
        *pending += 1;
        if let Some(shared) = self.shared.upgrade() {
            shared.record_item_consumed(self.channel_id);
            shared.record_pending_local_grant(self.channel_id, *pending);
        }
        if *pending < self.threshold {
            return;
        }

        let additional = *pending;
        *pending = 0;
        if let Some(shared) = self.shared.upgrade() {
            shared.record_pending_local_grant(self.channel_id, additional);
        }
        let _ = self.local_control_tx.send(DriverLocalControl::GrantCredit {
            channel_id: self.channel_id,
            additional,
        });
    }

    fn on_receiver_dropped(&self) {
        if let Some(shared) = self.shared.upgrade() {
            shared.record_receiver_dropped(self.channel_id);
        }
        let _ = self
            .local_control_tx
            .send(DriverLocalControl::ResetChannel {
                channel_id: self.channel_id,
            });
    }

    fn channel_id(&self) -> Option<ChannelId> {
        Some(self.channel_id)
    }

    fn connection_id(&self) -> Option<ConnectionId> {
        Some(self.connection_id)
    }

    fn debug_context(&self) -> Option<ChannelDebugContext> {
        self.debug_context
    }

    fn observer(&self) -> Option<VoxObserverHandle> {
        self.observer.clone()
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
        self.shared.terminal_channels.lock().clear();
    }

    fn close_outbound_channel(&self, channel_id: ChannelId) {
        self.shared.terminal_channels.lock().insert(channel_id);
        if let Some(semaphore) = self.shared.channel_credits.lock().remove(&channel_id) {
            semaphore.close();
        }
    }

    fn abort_channel_handlers(&mut self) {
        for in_flight in self.in_flight_handlers.values() {
            if self.handler.args_have_channels(in_flight.method_id) {
                if let Some(operation_id) = in_flight.operation_id {
                    self.shared.operations.remove(operation_id);
                    self.live_operations.lock().release(operation_id);
                }
                in_flight.abort.abort();
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
            local_settings,
            peer_settings,
            parity,
            peer_supports_retry,
            observer,
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
                connection_id: conn_id,
                pending_responses: SyncMutex::new("driver.pending_responses", BTreeMap::new()),
                request_ids: SyncMutex::new("driver.request_ids", IdAllocator::new(parity)),
                next_operation_id: AtomicU64::new(1),
                operations: operation_store,
                channel_ids: SyncMutex::new("driver.channel_ids", IdAllocator::new(parity)),
                channel_senders: SyncMutex::new("driver.channel_senders", BTreeMap::new()),
                channel_buffers: SyncMutex::new("driver.channel_buffers", BTreeMap::new()),
                channel_credits: SyncMutex::new("driver.channel_credits", BTreeMap::new()),
                channel_contexts: SyncMutex::new("driver.channel_contexts", BTreeMap::new()),
                request_debug: SyncMutex::new("driver.request_debug", BTreeMap::new()),
                channel_debug: SyncMutex::new("driver.channel_debug", BTreeMap::new()),
                last_inbound_message_at: SyncMutex::new("driver.last_inbound_message_at", None),
                last_outbound_message_at: SyncMutex::new("driver.last_outbound_message_at", None),
                close_reason: SyncMutex::new("driver.close_reason", None),
                terminal_channels: SyncMutex::new("driver.terminal_channels", HashSet::new()),
                stale_close_channels: SyncMutex::new(
                    "driver.stale_close_channels",
                    std::collections::HashSet::new(),
                ),
                local_initial_channel_credit: local_settings.initial_channel_credit,
                peer_initial_channel_credit: peer_settings.initial_channel_credit,
                observer,
            }),
            in_flight_handlers: BTreeMap::new(),
            handler_futs: FuturesUnordered::new(),
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

    // r[impl rpc.debug.snapshot]
    pub fn debug_snapshot(&self) -> VoxDebugSnapshot {
        self.shared.debug_snapshot(
            &self.sender,
            self.shared.connection_debug_state(*self.closed_rx.borrow()),
            DriverTaskStatus::Alive,
        )
    }

    pub fn dump_debug_snapshot(&self) -> VoxDebugSnapshot {
        let snapshot = self.debug_snapshot();
        tracing::info!(?snapshot, "vox debug snapshot");
        snapshot
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
                            let has_channels =
                                self.handler.args_have_channels(in_flight.method_id);
                            if has_channels && !in_flight.retry.idem {
                                Some(FailureDisposition::Indeterminate)
                            } else if has_channels && in_flight.retry.idem {
                                None
                            } else {
                                Some(disposition)
                            }
                        })
                        .unwrap_or(Some(disposition));
                    tracing::trace!(%req_id, in_flight_found, ?reply_disposition, "failures_rx computed disposition");
                    // Clean up the handler tracking entry.
                    self.in_flight_handlers.remove(&req_id);
                    self.shared.finish_request(req_id, RequestDebugState::Failed);
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
                // The handler-future arm only fires when at least one
                // handler is in flight. The guard is essential:
                // `FuturesUnordered::next` on an empty stream returns
                // `Poll::Ready(None)` immediately, which would spin the
                // select loop.
                Some(item) = self.handler_futs.next(), if !self.handler_futs.is_empty() => {
                    match item {
                        Ok(req_id) => {
                            let removed = self.in_flight_handlers.remove(&req_id).is_some();
                            self.shared.finish_request(req_id, RequestDebugState::Finished);
                            tracing::trace!(
                                %req_id,
                                removed,
                                in_flight = self.in_flight_handlers.len(),
                                "handler completion processed",
                            );
                        }
                        Err(_aborted) => {
                            // Cancel/abort paths already removed the entry
                            // before flipping the AbortHandle. Nothing to do.
                        }
                    }
                }
            }
        }

        for (_, in_flight) in std::mem::take(&mut self.in_flight_handlers) {
            if !in_flight.retry.persist {
                in_flight.abort.abort();
            }
        }
        self.shared.pending_responses.lock().clear();
        self.shared.request_debug.lock().clear();
        self.shared
            .set_connection_closed(ConnectionCloseReason::SessionShutdown);

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
                self.close_outbound_channel(channel_id);
                self.shared
                    .observe_channel(channel_id, None, |channel| ChannelEvent::Closed {
                        channel,
                        reason: ChannelCloseReason::Local,
                    });
                self.shared.mark_outbound_progress();
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
            DriverLocalControl::ResetChannel { channel_id } => {
                self.shared.channel_senders.lock().remove(&channel_id);
                self.shared.channel_buffers.lock().remove(&channel_id);
                self.close_outbound_channel(channel_id);
                self.shared
                    .observe_channel(channel_id, None, |channel| ChannelEvent::Reset {
                        channel,
                        reason: ChannelResetReason::Local,
                    });
                self.shared.mark_outbound_progress();
                let _ = self
                    .sender
                    .send(ConnectionMessage::Channel(ChannelMessage {
                        id: channel_id,
                        body: ChannelBody::Reset(vox_types::ChannelReset {
                            metadata: Default::default(),
                        }),
                    }))
                    .await;
            }
            DriverLocalControl::GrantCredit {
                channel_id,
                additional,
            } => {
                self.shared.observe_channel(channel_id, None, |channel| {
                    ChannelEvent::CreditGranted {
                        channel,
                        amount: additional,
                    }
                });
                self.shared.mark_outbound_progress();
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
        }
    }

    fn handle_recv(&mut self, recv: crate::session::RecvMessage) {
        self.shared.mark_inbound_progress();
        let crate::session::RecvMessage { schemas, msg } = recv;
        let msg_ref = msg.get();
        let is_request = matches!(msg_ref, ConnectionMessage::Request(_));
        if is_request {
            if let ConnectionMessage::Request(req) = msg_ref {
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
        let msg_ref = msg.get();
        let req_id = msg_ref.id;
        let is_call = matches!(&msg_ref.body, RequestBody::Call(_));
        let is_response = matches!(&msg_ref.body, RequestBody::Response(_));
        let is_cancel = matches!(&msg_ref.body, RequestBody::Cancel(_));

        if is_call {
            let method_id = match &msg_ref.body {
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
            let call_ref = call.get();
            let handler = Arc::clone(&self.handler);
            let retry = handler.retry_policy(call_ref.method_id);
            // Idempotent requests can be re-executed safely; skip operation tracking/storage.
            let operation_id = metadata_operation_id(&call_ref.metadata).filter(|_| !retry.idem);
            let method_id = call_ref.method_id;

            if let Some(operation_id) = operation_id {
                // 1. Check live tracker (in-flight operations in this session)
                let admit = self.live_operations.lock().admit(
                    operation_id,
                    call_ref.method_id,
                    incoming_args_bytes(call_ref),
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
                            let method_id = call_ref.method_id;
                            let response_shape = self.handler.response_wire_shape(method_id);
                            // Remove from live tracker — we're replaying, not running a handler.
                            self.live_operations.lock().seal(operation_id);
                            moire::task::spawn(
                                async move {
                                    if replay_sealed_response(
                                        sender.clone(),
                                        req_id,
                                        method_id,
                                        sealed.response.as_bytes(),
                                        response_shape,
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
                method_id: call_ref.method_id,
                retry,
                operation_id,
                operations: operation_id.map(|_| Arc::clone(&self.shared.operations)),
                live_operations: operation_id.map(|_| Arc::clone(&self.live_operations)),
                binder: self.internal_binder(),
                handler_response_shape: handler.response_wire_shape(call_ref.method_id),
            };
            self.shared.start_request(
                req_id,
                method_id,
                None,
                None,
                RequestDebugState::Dispatching,
            );
            let (abort, abort_reg) = AbortHandle::new_pair();
            let handler_fut: Pin<Box<dyn Future<Output = RequestId> + Send + 'static>> =
                Box::pin(async move {
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
                    req_id
                });
            self.handler_futs
                .push(Abortable::new(handler_fut, abort_reg));
            self.in_flight_handlers.insert(
                req_id,
                InFlightHandler {
                    abort,
                    method_id,
                    retry,
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
                        in_flight.abort.abort();
                        self.shared
                            .finish_request(req_id, RequestDebugState::Failed);
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
                        in_flight.abort.abort();
                        self.shared
                            .finish_request(owner_request_id, RequestDebugState::Failed);
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
        let msg_ref = msg.get();
        let chan_id = msg_ref.id;

        // Look up the channel sender from the shared registry (handles registered
        // by both the driver and any DriverCaller that set up channels).
        let sender = self.shared.channel_senders.lock().get(&chan_id).cloned();

        match &msg_ref.body {
            // r[impl rpc.channel.item]
            ChannelBody::Item(_item) => {
                self.shared
                    .observe_channel(chan_id, None, |channel| ChannelEvent::ItemReceived {
                        channel,
                    });
                if self.shared.terminal_channels.lock().contains(&chan_id) {
                    self.shared.record_inbound_item_not_enqueued(chan_id);
                    tracing::trace!(
                        conn_id = self.sender.connection_id().0,
                        channel_id = chan_id.0,
                        "driver dropped item for terminal channel"
                    );
                    return;
                }

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
                    match tx.try_send(IncomingChannelMessage::Item(item)) {
                        Ok(()) => {}
                        Err(mpsc::error::TrySendError::Closed(_)) => {
                            self.shared.record_inbound_item_not_enqueued(chan_id);
                            self.shared.channel_senders.lock().remove(&chan_id);
                            self.shared.channel_buffers.lock().remove(&chan_id);
                            self.close_outbound_channel(chan_id);
                            let _ = self
                                .local_control_tx
                                .send(DriverLocalControl::ResetChannel {
                                    channel_id: chan_id,
                                });
                        }
                        Err(mpsc::error::TrySendError::Full(_)) => {
                            self.shared.record_inbound_item_not_enqueued(chan_id);
                            // Preserve the old backpressure-overflow behavior:
                            // if the Rx queue is full, drop this item without
                            // treating the channel as abandoned.
                        }
                    }
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
                self.shared
                    .observe_channel(chan_id, None, |channel| ChannelEvent::Closed {
                        channel,
                        reason: ChannelCloseReason::Remote,
                    });
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
                self.shared.terminal_channels.lock().insert(chan_id);
                self.close_outbound_channel(chan_id);
            }
            // r[impl rpc.channel.reset]
            ChannelBody::Reset(_reset) => {
                self.shared
                    .observe_channel(chan_id, None, |channel| ChannelEvent::Reset {
                        channel,
                        reason: ChannelResetReason::Remote,
                    });
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
                self.shared.terminal_channels.lock().insert(chan_id);
                self.close_outbound_channel(chan_id);
            }
            // r[impl rpc.flow-control.credit.grant]
            // r[impl rpc.flow-control.credit.grant.additive]
            ChannelBody::GrantCredit(grant) => {
                self.shared
                    .record_credit_received(chan_id, grant.additional);
                self.shared.emit_channel_event(chan_id, None, |channel| {
                    ChannelEvent::CreditGranted {
                        channel,
                        amount: grant.additional,
                    }
                });
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
