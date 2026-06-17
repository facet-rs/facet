use std::{
    collections::{BTreeMap, HashMap, HashSet},
    panic::AssertUnwindSafe,
    pin::Pin,
    sync::{Arc, Weak},
    time::Duration,
};

use vox_types::time::Instant;

use futures_util::future::{AbortHandle, Abortable, FutureExt as FuturesFutureExt};
use futures_util::stream::{FuturesUnordered, StreamExt as _};
use tokio::sync::watch;
use vox_rt::sync::{Semaphore, SyncMutex};

use vox_rt::task::FutureExt as _;
use vox_types::{
    BoxFut, CallResult, ChannelBinder, ChannelBody, ChannelClose, ChannelCreditReplenisher,
    ChannelCreditReplenisherHandle, ChannelEventContext, ChannelId, ChannelItem,
    ChannelMailboxReceiver, ChannelMailboxSender, ChannelMessage, ChannelSink, CreditSink, Handler,
    IdAllocator, IncomingChannelMessage, LaneId, MaybeSend, MaybeSendFuture, MaybeSync, Parity,
    Payload, ReplySink, RequestBody, RequestCall, RequestCancel, RequestId, RequestMessage,
    RequestResponse, RequestTerminationReason, SelfRef, TrySendError, TxError, VoxError,
    channel_mailbox,
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

use crate::connection::{
    ConnectionMessage, ConnectionSender, DropControlRequest, FailureDisposition, LaneHandle,
};
use vox_rt::sync::mpsc;

/// A pending response for one outbound request attempt.
///
/// Carries both the wire response message and the recv tracker that was
/// current when the response was received, so the caller can deserialize
/// the response with the correct schemas.
struct PendingResponse {
    msg: SelfRef<RequestMessage<'static>>,
    schemas: Arc<vox_types::SchemaRecvTracker>,
    /// Descriptors that arrived with the response frame, surfaced to the
    /// caller via [`WithTracker::fds`](vox_types::WithTracker) and installed
    /// at the typed-return decode site. `()` off-Unix.
    fds: vox_types::FrameFds,
}

type ResponseSlot = vox_rt::sync::oneshot::Sender<Result<PendingResponse, VoxError>>;

async fn send_vox_error_response(
    sender: ConnectionSender,
    req_id: RequestId,
    response_shape: Option<(vox_types::MethodId, &'static facet::Shape)>,
    vox_error: VoxError<core::convert::Infallible>,
) {
    if let Some((method_id, response_shape)) = response_shape {
        let error: Result<(), VoxError<core::convert::Infallible>> = Err(vox_error);
        let mut response = RequestResponse {
            ret: Payload::outgoing(&error),
            metadata: Default::default(),
            schemas: Default::default(),
        };
        sender.prepare_response_for_shape(req_id, method_id, response_shape, &mut response);
        let _ = sender.send_response(req_id, response).await;
    } else {
        let error: Result<(), VoxError<core::convert::Infallible>> = Err(vox_error);
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
}

// r[impl rpc.timeout.idle-progress]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RequestTimeoutPolicy {
    idle_timeout: Option<Duration>,
}

impl RequestTimeoutPolicy {
    pub const fn disabled() -> Self {
        Self { idle_timeout: None }
    }

    pub const fn idle(timeout: Duration) -> Self {
        Self {
            idle_timeout: Some(timeout),
        }
    }

    pub const fn idle_timeout(self) -> Option<Duration> {
        self.idle_timeout
    }
}

struct InFlightHandler {
    /// Aborts the handler future hosted on `Driver::handler_futs`. Triggered
    /// by `Cancel`-style flows; the FuturesUnordered will yield an `Aborted`
    /// item on its next poll, and the request will be removed from
    /// `in_flight_handlers` (if not already gone).
    abort: AbortHandle,
    method_id: vox_types::MethodId,
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
enum HandlerCompletion {
    Finished(RequestId),
    Panicked {
        request_id: RequestId,
        method_id: vox_types::MethodId,
    },
}

type HandlerFut = Abortable<Pin<Box<dyn MaybeSendFuture<Output = HandlerCompletion> + 'static>>>;

#[derive(Clone)]
// r[impl rpc.request.scope]
struct RequestScope {
    method_id: vox_types::MethodId,
    service: Option<&'static str>,
    method: Option<&'static str>,
    started_at: Instant,
    last_progress_at: Instant,
    state: RequestDebugState,
    response_sender_blocked: Option<bool>,
    associated_channels: Vec<ChannelId>,
}

impl RequestScope {
    fn new(
        method_id: vox_types::MethodId,
        service: Option<&'static str>,
        method: Option<&'static str>,
        state: RequestDebugState,
        associated_channels: Vec<ChannelId>,
    ) -> Self {
        let now = Instant::now();
        Self {
            method_id,
            service,
            method,
            started_at: now,
            last_progress_at: now,
            state,
            response_sender_blocked: Some(false),
            associated_channels,
        }
    }

    fn snapshot(&self, request_id: RequestId, now: Instant) -> RequestDebugSnapshot {
        RequestDebugSnapshot {
            request_id,
            service: self.service,
            method: self.method,
            method_id: self.method_id,
            age: now.saturating_duration_since(self.started_at),
            idle_for: now.saturating_duration_since(self.last_progress_at),
            last_progress_at: self.last_progress_at,
            state: self.state,
            response_sender_blocked: self.response_sender_blocked,
            associated_channels: self.associated_channels.clone(),
        }
    }

    fn mark_progress(&mut self, now: Instant) {
        self.last_progress_at = now;
    }

    // r[impl rpc.request.scope.channels]
    fn associate_channels(&mut self, channels: &[ChannelId]) {
        for channel_id in channels {
            if !self.associated_channels.contains(channel_id) {
                self.associated_channels.push(*channel_id);
            }
        }
    }

    // r[impl rpc.request.scope.terminal]
    fn finish(mut self, state: RequestDebugState) -> Vec<ChannelId> {
        self.state = state;
        self.associated_channels
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
        connection_id: LaneId,
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
/// request attempts.
struct DriverShared {
    connection_id: LaneId,
    pending_responses: SyncMutex<BTreeMap<RequestId, ResponseSlot>>,
    request_ids: SyncMutex<IdAllocator<RequestId>>,
    channel_ids: SyncMutex<IdAllocator<ChannelId>>,
    /// Registry mapping inbound channel IDs to the sender that feeds the Rx handle.
    channel_senders: SyncMutex<BTreeMap<ChannelId, ChannelMailboxSender<IncomingChannelMessage>>>,
    /// Receivers for channels that received messages before application code
    /// deserialized/registered the corresponding `Rx` handle.
    channel_receivers:
        SyncMutex<BTreeMap<ChannelId, ChannelMailboxReceiver<IncomingChannelMessage>>>,
    /// Credit semaphores for outbound channels (Tx on our side).
    /// The driver's GrantCredit handler adds permits to these.
    channel_credits: SyncMutex<BTreeMap<ChannelId, Arc<Semaphore>>>,
    // r[impl rpc.observability.channel.context]
    channel_contexts: SyncMutex<BTreeMap<ChannelId, ChannelDebugContext>>,
    // r[impl rpc.debug.snapshot]
    request_scopes: SyncMutex<BTreeMap<RequestId, RequestScope>>,
    request_timeout: RequestTimeoutPolicy,
    // r[impl rpc.debug.snapshot]
    channel_debug: SyncMutex<BTreeMap<ChannelId, ChannelRuntimeDebug>>,
    last_inbound_message_at: SyncMutex<Option<Instant>>,
    last_outbound_message_at: SyncMutex<Option<Instant>>,
    close_reason: SyncMutex<Option<ConnectionCloseReason>>,
    /// Channel IDs that have reached a terminal local state. Once a channel is
    /// closed/reset, outbound sinks must reject further sends and inbound items
    /// must not be buffered forever.
    terminal_channels: SyncMutex<HashSet<ChannelId>>,
    channel_schema_roles: SyncMutex<
        HashMap<(vox_types::MethodId, vox_types::BindingDirection, String), Vec<ChannelId>>,
    >,
    // r[impl rpc.flow-control.credit.initial]
    local_initial_channel_credit: u32,
    // r[impl rpc.flow-control.credit.initial]
    peer_initial_channel_credit: u32,
    // r[impl rpc.flow-control.max-concurrent-requests.outbound]
    outbound_request_limit: Semaphore,
    // r[impl rpc.flow-control.max-concurrent-requests.inbound]
    local_max_concurrent_requests: u32,
    peer_request_parity: Parity,
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

    fn note_channel_schema_role(
        &self,
        channel_id: ChannelId,
        method_id: vox_types::MethodId,
        direction: vox_types::BindingDirection,
        role: &str,
    ) {
        let mut roles = self.channel_schema_roles.lock();
        let channels = roles
            .entry((method_id, direction, role.to_string()))
            .or_default();
        if !channels.contains(&channel_id) {
            channels.push(channel_id);
        }
    }

    fn channel_schema_roles_for(
        &self,
        method_id: vox_types::MethodId,
        direction: vox_types::BindingDirection,
    ) -> Vec<(String, Vec<ChannelId>)> {
        self.channel_schema_roles
            .lock()
            .iter()
            .filter(|((stored_method, stored_direction, _role), _channels)| {
                *stored_method == method_id && *stored_direction == direction
            })
            .map(|((_method, _direction, role), channels)| (role.clone(), channels.clone()))
            .collect()
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
                let channel_id = channel.channel_id;
                self.update_channel_debug(channel, ChannelDirection::Rx, 0, |entry| {
                    entry.mark_item_received(now);
                });
                self.mark_channel_request_progress(channel_id);
            }
            ChannelEvent::Closed { channel, reason } => {
                let channel_id = channel.channel_id;
                self.update_channel_debug(channel, ChannelDirection::Rx, 0, |entry| {
                    entry.mark_closed(reason);
                });
                self.mark_channel_request_progress(channel_id);
            }
            ChannelEvent::Reset { channel, reason } => {
                let channel_id = channel.channel_id;
                self.update_channel_debug(channel, ChannelDirection::Rx, 0, |entry| {
                    entry.mark_reset(reason);
                });
                self.mark_channel_request_progress(channel_id);
            }
            ChannelEvent::CreditGranted { channel, amount } => {
                self.record_credit_granted_at(channel.channel_id, amount, now);
                self.mark_channel_request_progress(channel.channel_id);
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

    // r[impl rpc.timeout.idle-progress]
    fn mark_request_progress(&self, request_id: RequestId) {
        if let Some(scope) = self.request_scopes.lock().get_mut(&request_id) {
            scope.mark_progress(Instant::now());
        }
    }

    // r[impl rpc.timeout.idle-progress]
    fn mark_channel_request_progress(&self, channel_id: ChannelId) {
        let now = Instant::now();
        for scope in self.request_scopes.lock().values_mut() {
            if scope.associated_channels.contains(&channel_id) {
                scope.mark_progress(now);
            }
        }
    }

    // r[impl rpc.timeout.idle-progress]
    fn next_request_idle_sleep_duration(&self) -> Option<Duration> {
        let timeout = self.request_timeout.idle_timeout()?;
        let now = Instant::now();
        self.request_scopes
            .lock()
            .values()
            .map(|scope| {
                timeout.saturating_sub(now.saturating_duration_since(scope.last_progress_at))
            })
            .min()
    }

    // r[impl rpc.timeout.idle-progress]
    fn expired_idle_request_ids(&self) -> Vec<RequestId> {
        let Some(timeout) = self.request_timeout.idle_timeout() else {
            return Vec::new();
        };
        let now = Instant::now();
        self.request_scopes
            .lock()
            .iter()
            .filter_map(|(request_id, scope)| {
                (now.saturating_duration_since(scope.last_progress_at) >= timeout)
                    .then_some(*request_id)
            })
            .collect()
    }

    fn start_request(
        &self,
        request_id: RequestId,
        method_id: vox_types::MethodId,
        service: Option<&'static str>,
        method: Option<&'static str>,
        state: RequestDebugState,
        associated_channels: Vec<ChannelId>,
    ) {
        self.request_scopes.lock().insert(
            request_id,
            RequestScope::new(method_id, service, method, state, associated_channels),
        );
    }

    // r[impl rpc.request.scope.channels]
    // r[impl rpc.channel.lifecycle]
    fn associate_request_channels(&self, request_id: RequestId, channels: &[ChannelId]) {
        if channels.is_empty() {
            return;
        }
        if let Some(scope) = self.request_scopes.lock().get_mut(&request_id) {
            scope.associate_channels(channels);
            scope.mark_progress(Instant::now());
        }
    }

    // r[impl rpc.request.scope.terminal]
    // r[impl rpc.request.scope.channels]
    // r[impl rpc.channel.lifecycle]
    fn finish_request(
        &self,
        request_id: RequestId,
        state: RequestDebugState,
        termination: RequestTerminationReason,
    ) {
        let associated_channels = {
            let mut scopes = self.request_scopes.lock();
            let Some(scope) = scopes.remove(&request_id) else {
                return;
            };
            scope.finish(state)
        };
        self.terminate_request_channels(associated_channels, termination);
    }

    fn terminate_request_channels(
        &self,
        channels: Vec<ChannelId>,
        termination: RequestTerminationReason,
    ) {
        for channel_id in channels {
            self.terminate_request_channel(channel_id, termination);
        }
    }

    fn terminate_request_channel(
        &self,
        channel_id: ChannelId,
        termination: RequestTerminationReason,
    ) {
        if !self.terminal_channels.lock().insert(channel_id) {
            return;
        }

        if let Some(semaphore) = self.channel_credits.lock().remove(&channel_id) {
            semaphore.close();
        }

        if let Some(sender) = self.channel_senders.lock().remove(&channel_id) {
            let _ = sender.force_send(IncomingChannelMessage::RequestTerminated(termination));
        }

        self.observe_channel(channel_id, None, |channel| ChannelEvent::Closed {
            channel,
            reason: ChannelCloseReason::RequestTerminated,
        });
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
        if outcome == ChannelSendOutcome::Sent {
            self.mark_channel_request_progress(channel_id);
        }
    }

    fn record_try_send_outcome(&self, channel_id: ChannelId, outcome: ChannelTrySendOutcome) {
        let now = Instant::now();
        self.update_existing_channel_debug(channel_id, |channel| {
            channel.mark_try_send_outcome(outcome, now);
        });
        if outcome == ChannelTrySendOutcome::Sent {
            self.mark_channel_request_progress(channel_id);
        }
    }

    fn record_item_consumed(&self, channel_id: ChannelId) {
        let now = Instant::now();
        self.update_existing_channel_debug(channel_id, |channel| {
            channel.mark_item_consumed(now);
        });
        self.mark_channel_request_progress(channel_id);
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
        self.mark_channel_request_progress(channel_id);
    }

    fn record_receiver_dropped(&self, channel_id: ChannelId) {
        self.update_existing_channel_debug(channel_id, ChannelRuntimeDebug::mark_receiver_dropped);
    }

    fn new_channel_mailbox(
        &self,
    ) -> (
        ChannelMailboxSender<IncomingChannelMessage>,
        ChannelMailboxReceiver<IncomingChannelMessage>,
    ) {
        channel_mailbox(
            "driver.channel_mailbox",
            self.local_initial_channel_credit as usize,
        )
    }

    fn inbound_channel_sender(
        &self,
        channel_id: ChannelId,
    ) -> ChannelMailboxSender<IncomingChannelMessage> {
        let mut senders = self.channel_senders.lock();
        if let Some(sender) = senders.get(&channel_id) {
            return sender.clone();
        }

        let (sender, receiver) = self.new_channel_mailbox();
        senders.insert(channel_id, sender.clone());
        self.channel_receivers.lock().insert(channel_id, receiver);
        sender
    }

    fn register_inbound_channel_receiver(
        &self,
        channel_id: ChannelId,
    ) -> (ChannelMailboxReceiver<IncomingChannelMessage>, bool) {
        let terminal = self.terminal_channels.lock().contains(&channel_id);
        let mut senders = self.channel_senders.lock();
        let mut receivers = self.channel_receivers.lock();

        if let Some(receiver) = receivers.remove(&channel_id) {
            return (receiver, terminal);
        }

        let (sender, receiver) = self.new_channel_mailbox();
        if terminal {
            drop(sender);
        } else {
            senders.insert(channel_id, sender);
        }
        (receiver, terminal)
    }

    fn debug_snapshot(
        &self,
        sender: &ConnectionSender,
        state: ConnectionDebugState,
        driver_task_status: DriverTaskStatus,
    ) -> VoxDebugSnapshot {
        let now = Instant::now();
        let requests: Vec<_> = self
            .request_scopes
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

#[cfg(test)]
mod tests {
    use super::{DriverChannelCreditReplenisher, DriverLocalControl};
    use vox_types::{ChannelCreditReplenisher, ChannelId};

    #[tokio::test]
    async fn replenisher_batches_at_half_the_initial_window() {
        let (tx, mut rx) = vox_rt::sync::mpsc::unbounded_channel("test.replenisher");
        let replenisher = DriverChannelCreditReplenisher::new(
            vox_types::LaneId::ROOT,
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
            vox_types::time::tokio::timeout(std::time::Duration::from_millis(20), rx.recv())
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
        let (tx, mut rx) = vox_rt::sync::mpsc::unbounded_channel("test.replenisher.single");
        let replenisher = DriverChannelCreditReplenisher::new(
            vox_types::LaneId::ROOT,
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
/// If dropped without `send_reply` being called, automatically records the
/// request as cancelled so the caller observes a terminal call outcome even if
/// the handler panics or forgets to reply.
pub struct DriverReplySink {
    sender: Option<ConnectionSender>,
    request_id: RequestId,
    method_id: vox_types::MethodId,
    binder: DriverChannelBinder,
}

impl ReplySink for DriverReplySink {
    async fn send_reply(mut self, response: RequestResponse<'_>) {
        let sender = self
            .sender
            .take()
            .expect("unreachable: send_reply takes self by value");

        vox_types::dlog!(
            "[driver] send_reply: conn={:?} req={:?} method={:?} payload={}",
            sender.connection_id(),
            self.request_id,
            self.method_id,
            match &response.ret {
                Payload::Value { .. } => "Value",
                Payload::Encoded(_) => "Encoded",
            },
        );
        tracing::debug!(
            conn_id = ?sender.connection_id(),
            req_id = ?self.request_id,
            method_id = ?self.method_id,
            payload = match &response.ret {
                Payload::Value { .. } => "value",
                Payload::Encoded(_) => "encoded-bytes",
            },
            "vox driver sending reply"
        );
        self.binder.shared.mark_outbound_progress();
        self.binder.shared.mark_request_progress(self.request_id);

        if let Payload::Value { shape, .. } = &response.ret
            && let Ok(extracted) = vox_types::extract_schemas(shape)
        {
            vox_types::dlog!(
                "[schema] driver send_reply: method={:?} root={:?}",
                self.method_id,
                extracted.root
            );
        }

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
            tracing::debug!(
                conn_id = ?sender.connection_id(),
                req_id = ?self.request_id,
                method_id = ?self.method_id,
                "vox driver reply send failed"
            );
            sender.mark_failure(self.request_id, FailureDisposition::Cancelled);
        }
    }

    fn channel_binder(&self) -> Option<&dyn ChannelBinder> {
        Some(&self.binder)
    }

    fn request_id(&self) -> Option<RequestId> {
        Some(self.request_id)
    }

    fn connection_id(&self) -> Option<vox_types::LaneId> {
        self.sender.as_ref().map(|sender| sender.connection_id())
    }
}

impl Drop for DriverReplySink {
    fn drop(&mut self) {
        if let Some(sender) = self.sender.take() {
            sender.mark_failure(self.request_id, FailureDisposition::Cancelled);
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
    writer_schema: Option<vox_types::ChannelWriterSchemaPlan>,
}

impl ChannelSink for DriverChannelSink {
    fn send_payload<'payload>(
        &self,
        payload: Payload<'payload>,
    ) -> Pin<Box<dyn vox_types::MaybeSendFuture<Output = Result<(), TxError>> + 'payload>> {
        let sender = self.sender.clone();
        let shared = Arc::clone(&self.shared);
        let channel_id = self.channel_id;
        let writer_schema = self.writer_schema.clone();
        Box::pin(async move {
            if shared.terminal_channels.lock().contains(&channel_id) {
                return Err(TxError::Transport("channel closed".into()));
            }

            shared.mark_outbound_progress();
            // r[impl schema.exchange.channels.tx-args]
            sender
                .send_channel_with_writer_schema(
                    ChannelMessage {
                        id: channel_id,
                        body: ChannelBody::Item(ChannelItem { item: payload }),
                    },
                    writer_schema,
                )
                .await
                .map_err(|()| TxError::Transport("connection closed".into()))
        })
    }

    fn channel_id(&self) -> Option<ChannelId> {
        Some(self.channel_id)
    }

    fn connection_id(&self) -> Option<vox_types::LaneId> {
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
        // r[impl schema.exchange.channels.tx-args]
        self.sender
            .try_send_channel_with_writer_schema(
                ChannelMessage {
                    id: self.channel_id,
                    body: ChannelBody::Item(ChannelItem { item: payload }),
                },
                self.writer_schema.clone(),
            )
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
// r[impl rpc.caller.liveness.refcounted]
// r[impl rpc.caller.liveness.last-drop-closes-connection]
// r[impl rpc.caller.liveness.public-handle-drop]
// r[impl rpc.caller.liveness.explicit-shutdown-required]
#[must_use = "Dropping this caller does not close the connection; shut down explicitly with ConnectionHandle when needed."]
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

    /// Attach a generated service descriptor to this caller.
    pub fn with_service(mut self, service: &'static vox_types::ServiceDescriptor) -> Self {
        if let Some(existing_service) = self.service {
            assert_eq!(
                existing_service.service_name, service.service_name,
                "Caller service mismatch"
            );
        } else {
            self.service = Some(service);
        }
        self
    }

    /// Append a client middleware to this caller's chain.
    pub fn with_middleware(
        mut self,
        service: &'static vox_types::ServiceDescriptor,
        middleware: impl vox_types::ClientMiddleware,
    ) -> Self {
        self = self.with_service(service);
        self.middlewares.push(Arc::new(middleware));
        self
    }

    /// Start one outgoing request attempt and wait for its response,
    /// running any registered middleware around the call.
    pub async fn call(&self, mut call: RequestCall<'_>) -> CallResult {
        use vox_types::{ClientCallOutcome, ClientContext, ClientRequest, Extensions};

        let Some(service) = self.service else {
            return self.inner.call_inner(call, None).await;
        };

        let extensions = Extensions::new();
        let method = service.by_id(call.method_id);
        if call.schemas.is_empty()
            && let Some(method) = method
        {
            match vox_types::SchemaSendTracker::plan_for_method_args(method) {
                Ok(prepared) => call.schemas = prepared.to_payload(),
                Err(error) => tracing::error!(
                    method_id = ?call.method_id,
                    "schema attachment failed: {error}"
                ),
            }
        }
        let context = ClientContext::new(method, call.method_id, &extensions);

        if !self.middlewares.is_empty() {
            for middleware in &self.middlewares {
                let mut request = ClientRequest::new(&mut call);
                middleware.pre(&context, &mut request).await;
            }
        }

        let result = self.inner.call_inner(call, method).await;
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
        if self.inner.closed_rx.borrow().is_some() {
            return;
        }
        let mut rx = self.inner.closed_rx.clone();
        while rx.changed().await.is_ok() {
            if rx.borrow().is_some() {
                return;
            }
        }
    }

    /// Return whether the underlying connection is still considered connected.
    pub fn is_connected(&self) -> bool {
        self.inner.closed_rx.borrow().is_none()
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

/// Trait for constructing a typed client from a Vox service lane.
///
/// Generated `*Client` types implement this to receive both the caller
/// and an optional connection handle.
pub trait FromVoxLane {
    /// The service name for this client, used for automatic `vox-service` metadata.
    const SERVICE_NAME: &'static str;

    fn from_vox_lane(
        caller: Caller,
        connection_handle: Option<crate::connection::ConnectionHandle>,
    ) -> Self;
}

#[derive(Clone)]
struct DriverChannelBinder {
    sender: ConnectionSender,
    shared: Arc<DriverShared>,
    local_control_tx: mpsc::UnboundedSender<DriverLocalControl>,
}

fn register_rx_channel_impl(
    shared: &Arc<DriverShared>,
    channel_id: ChannelId,
    initial_channel_credit: u32,
    debug_context: Option<ChannelDebugContext>,
    local_control_tx: mpsc::UnboundedSender<DriverLocalControl>,
) -> vox_types::BoundChannelReceiver {
    observe_channel_opened(
        shared,
        channel_id,
        ChannelDirection::Rx,
        initial_channel_credit,
        debug_context,
    );
    let (rx, terminal) = shared.register_inbound_channel_receiver(channel_id);

    if terminal {
        shared.channel_credits.lock().remove(&channel_id);
        return vox_types::BoundChannelReceiver {
            receiver: rx,
            replenisher: None,
            writer_schema: None,
        };
    }

    vox_types::BoundChannelReceiver {
        receiver: rx,
        replenisher: Some(Arc::new(DriverChannelCreditReplenisher::new(
            shared.connection_id,
            channel_id,
            debug_context,
            Arc::downgrade(shared),
            initial_channel_credit,
            local_control_tx,
            shared.observer.clone(),
        )) as ChannelCreditReplenisherHandle),
        writer_schema: None,
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
    writer_schema: Option<vox_types::ChannelWriterSchemaPlan>,
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
        writer_schema,
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

    fn create_tx_credit_sink(
        &self,
        debug_context: Option<ChannelDebugContext>,
        gate_until_declaring_call: bool,
    ) -> (ChannelId, Arc<CreditSink<DriverChannelSink>>) {
        let shared = self.endpoint_shared();
        let channel_id = shared.channel_ids.lock().alloc();
        if gate_until_declaring_call {
            // r[impl rpc.channel.item] Register a CLOSED send-gate for this
            // freshly-opened outbound channel BEFORE binding its sink, so an
            // application `tx.send` that wakes when the sink binds parks until the
            // declaring Call is enqueued — the Call must reach the wire before any
            // item on the channel it opens.
            self.endpoint_sender()
                .sess_core
                .register_channel_gate(channel_id);
        }
        let sink = make_tx_channel_sink(
            self.endpoint_sender(),
            shared,
            self.endpoint_local_control_tx(),
            channel_id,
            debug_context,
            None,
        );
        (channel_id, sink)
    }

    fn create_tx_dyn(
        &self,
        debug_context: Option<ChannelDebugContext>,
    ) -> (ChannelId, Arc<dyn ChannelSink>) {
        let (id, sink) = self.create_tx_credit_sink(debug_context, true);
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
        writer_schema: Option<vox_types::ChannelWriterSchemaPlan>,
    ) -> Arc<dyn ChannelSink> {
        make_tx_channel_sink(
            self.endpoint_sender(),
            self.endpoint_shared(),
            self.endpoint_local_control_tx(),
            channel_id,
            debug_context,
            writer_schema,
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
            shared.local_initial_channel_credit,
            debug_context,
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
        self.bind_tx_dyn(channel_id, None, None)
    }

    fn bind_tx_with_context(
        &self,
        channel_id: ChannelId,
        debug_context: Option<ChannelDebugContext>,
    ) -> Arc<dyn ChannelSink> {
        self.bind_tx_dyn(channel_id, debug_context, None)
    }

    fn bind_tx_with_context_and_writer_schema(
        &self,
        channel_id: ChannelId,
        debug_context: Option<ChannelDebugContext>,
        writer_schema: Option<vox_types::ChannelWriterSchemaPlan>,
    ) -> Arc<dyn ChannelSink> {
        self.bind_tx_dyn(channel_id, debug_context, writer_schema)
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

    fn note_channel_schema_role(
        &self,
        channel_id: ChannelId,
        method_id: vox_types::MethodId,
        direction: vox_types::BindingDirection,
        role: &str,
    ) {
        self.shared
            .note_channel_schema_role(channel_id, method_id, direction, role);
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
    closed_rx: watch::Receiver<Option<ConnectionCloseReason>>,
}

impl DriverCaller {
    /// Allocate a channel ID and create a credit-controlled sink for outbound items.
    ///
    /// The returned sink enforces credit; the semaphore is registered so
    /// `GrantCredit` messages can add permits.
    pub fn create_tx_channel(&self) -> (ChannelId, Arc<CreditSink<DriverChannelSink>>) {
        self.create_tx_credit_sink(None, false)
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
        self.bind_tx_dyn(channel_id, None, None)
    }

    fn bind_tx_with_context(
        &self,
        channel_id: ChannelId,
        debug_context: Option<ChannelDebugContext>,
    ) -> Arc<dyn ChannelSink> {
        self.bind_tx_dyn(channel_id, debug_context, None)
    }

    fn bind_tx_with_context_and_writer_schema(
        &self,
        channel_id: ChannelId,
        debug_context: Option<ChannelDebugContext>,
        writer_schema: Option<vox_types::ChannelWriterSchemaPlan>,
    ) -> Arc<dyn ChannelSink> {
        self.bind_tx_dyn(channel_id, debug_context, writer_schema)
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

    fn note_channel_schema_role(
        &self,
        channel_id: ChannelId,
        method_id: vox_types::MethodId,
        direction: vox_types::BindingDirection,
        role: &str,
    ) {
        self.shared
            .note_channel_schema_role(channel_id, method_id, direction, role);
    }
}

impl DriverCaller {
    // r[impl rpc.debug.snapshot]
    pub fn debug_snapshot(&self) -> VoxDebugSnapshot {
        self.shared.debug_snapshot(
            &self.sender,
            self.shared
                .connection_debug_state(self.closed_rx.borrow().is_some()),
            if self.closed_rx.borrow().is_some() {
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
        call: RequestCall<'_>,
        method: Option<&'static vox_types::MethodDescriptor>,
    ) -> CallResult {
        // r[impl rpc.flow-control.max-concurrent-requests.outbound]
        // r[impl rpc.flow-control.max-concurrent-requests.counting]
        let _request_permit = self
            .shared
            .outbound_request_limit
            .acquire_owned()
            .await
            .map_err(|_| VoxError::ConnectionClosed)?;

        // Allocate a request ID.
        let req_id = self.shared.request_ids.lock().alloc();
        let request_started_at = Instant::now();
        let (service_name, method_name) = method
            .map(|method| (method.service_name, method.method_name))
            .unwrap_or(("<unknown>", "<unknown>"));
        tracing::debug!(
            conn_id = ?self.sender.connection_id(),
            ?req_id,
            method_id = ?call.method_id,
            service = service_name,
            method = method_name,
            "vox caller starting request"
        );
        if let Some(observer) = &self.shared.observer {
            observer.driver_event(DriverEvent::RequestStarted {
                connection_id: self.sender.connection_id(),
                request_id: req_id,
                method_id: call.method_id,
            });
        }
        let finish_request = |outcome: RpcOutcome| {
            let (state, termination) = match outcome {
                RpcOutcome::Ok => (
                    RequestDebugState::Finished,
                    RequestTerminationReason::ResponseDelivered,
                ),
                RpcOutcome::Cancelled => (
                    RequestDebugState::Failed,
                    RequestTerminationReason::Cancelled,
                ),
                RpcOutcome::TimedOut => (
                    RequestDebugState::TimedOut,
                    RequestTerminationReason::TimedOut,
                ),
                _ => (RequestDebugState::Failed, RequestTerminationReason::Failed),
            };
            self.shared.finish_request(req_id, state, termination);
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
        let (tx, rx) = vox_rt::sync::oneshot::channel("driver.response");
        self.shared.pending_responses.lock().insert(req_id, tx);
        self.shared.start_request(
            req_id,
            call.method_id,
            Some(service_name),
            Some(method_name),
            RequestDebugState::WaitingForResponse,
            Vec::new(),
        );

        // r[depends schema.exchange.channels]
        // Generated clients attach their service descriptor to the caller so
        // channel element roots can be advertised before middleware observes
        // the call. SessionCore::send() still decides whether this peer has
        // already seen the per-method binding.
        //
        // Channel binding happens during serialization via the thread-local
        // ChannelBinder. Channel element schemas are recorded in method
        // descriptors, but channel item compatibility still needs those
        // writer element roots threaded into Rx construction.
        self.shared.mark_outbound_progress();
        tracing::debug!(
            conn_id = ?self.sender.connection_id(),
            ?req_id,
            method_id = ?call.method_id,
            service = service_name,
            method = method_name,
            "vox caller sending request"
        );
        let shared = Arc::clone(&self.shared);
        if self
            .sender
            .send_with_binder_and_method_observing_channels(
                ConnectionMessage::Request(RequestMessage {
                    id: req_id,
                    body: RequestBody::Call(RequestCall {
                        method_id: call.method_id,
                        // Populated by the session's outbound pre-encode when args
                        // carry channels (r[rpc.request]).
                        channels: call.channels.clone(),
                        args: call.args.reborrow(),
                        metadata: call.metadata.clone(),
                        schemas: call.schemas.clone(),
                    }),
                }),
                Some(self),
                method,
                move |channels| shared.associate_request_channels(req_id, channels),
            )
            .await
            .is_err()
        {
            tracing::debug!(
                conn_id = ?self.sender.connection_id(),
                ?req_id,
                method_id = ?call.method_id,
                service = service_name,
                method = method_name,
                "vox caller request send failed"
            );
            self.shared.pending_responses.lock().remove(&req_id);
            finish_request(RpcOutcome::SendFailed);
            return Err(VoxError::SendFailed);
        }
        self.shared.mark_request_progress(req_id);
        tracing::debug!(
            conn_id = ?self.sender.connection_id(),
            ?req_id,
            method_id = ?call.method_id,
            service = service_name,
            method = method_name,
            "vox caller request sent; waiting for response"
        );

        let mut closed_rx = self.closed_rx.clone();
        let mut response = std::pin::pin!(rx.named("awaiting_response"));

        let pending: PendingResponse = loop {
            tokio::select! {
                result = &mut response => {
                    match result {
                        Ok(Ok(pending)) => {
                            tracing::debug!(
                                conn_id = ?self.sender.connection_id(),
                                ?req_id,
                                method_id = ?call.method_id,
                                service = service_name,
                                method = method_name,
                                "vox caller received response"
                            );
                            break pending;
                        }
                        Ok(Err(error)) => {
                            let outcome = match &error {
                                VoxError::Cancelled => RpcOutcome::Cancelled,
                                VoxError::TimedOut => RpcOutcome::TimedOut,
                                VoxError::ConnectionClosed | VoxError::ConnectionShutdown => RpcOutcome::Closed,
                                VoxError::SendFailed => RpcOutcome::SendFailed,
                                VoxError::Indeterminate => RpcOutcome::Indeterminate,
                                VoxError::User(_) | VoxError::UnknownMethod | VoxError::InvalidPayload(_) => {
                                    RpcOutcome::Error
                                }
                            };
                            finish_request(outcome);
                            return Err(error);
                        }
                        Err(_) => {
                            finish_request(RpcOutcome::Closed);
                            return Err(VoxError::ConnectionClosed);
                        }
                    }
                }
                changed = closed_rx.changed() => {
                    vox_types::dlog!("[CALLER] closed_rx fired, value={:?}", *closed_rx.borrow());
                    if changed.is_err() || closed_rx.borrow().is_some() {
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
            fds: response_fds,
        } = pending;
        let response = response_msg.map(|m| match m.body {
            RequestBody::Response(r) => r,
            _ => unreachable!("pending_responses only gets Response variants"),
        });

        finish_request(RpcOutcome::Ok);
        Ok(vox_types::WithTracker {
            value: response,
            tracker: response_schemas,
            fds: response_fds,
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
    rx: mpsc::Receiver<crate::connection::RecvMessage>,
    failures_rx: mpsc::UnboundedReceiver<(RequestId, FailureDisposition)>,
    closed_rx: watch::Receiver<Option<ConnectionCloseReason>>,
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
    local_control_tx: mpsc::UnboundedSender<DriverLocalControl>,
    drop_control_seed: Option<mpsc::UnboundedSender<DropControlRequest>>,
    suppressed_failures: HashSet<RequestId>,
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
    connection_id: LaneId,
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
        connection_id: LaneId,
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

    fn connection_id(&self) -> Option<LaneId> {
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
    // r[impl rpc.channel.connection-closure]
    fn close_all_channel_runtime_state(&self, close_reason: ConnectionCloseReason) {
        let mut credits = self.shared.channel_credits.lock();
        for semaphore in credits.values() {
            semaphore.close();
        }
        credits.clear();
        drop(credits);

        let channel_senders = {
            let mut senders = self.shared.channel_senders.lock();
            std::mem::take(&mut *senders)
        };
        for (channel_id, sender) in channel_senders {
            let _ = sender.force_send(IncomingChannelMessage::ConnectionClosed(close_reason));
            self.shared
                .observe_channel(channel_id, None, |channel| ChannelEvent::Closed {
                    channel,
                    reason: ChannelCloseReason::ConnectionClosed,
                });
        }
        self.shared.channel_receivers.lock().clear();
        self.shared.channel_schema_roles.lock().clear();
        self.shared.terminal_channels.lock().clear();
    }

    fn close_outbound_channel(&self, channel_id: ChannelId) {
        self.shared.terminal_channels.lock().insert(channel_id);
        if let Some(semaphore) = self.shared.channel_credits.lock().remove(&channel_id) {
            semaphore.close();
        }
    }

    fn protocol_violation(&self, description: String) {
        tracing::warn!(
            conn_id = ?self.sender.connection_id(),
            %description,
            "closing connection after protocol violation"
        );
        if let Some(control_tx) = &self.drop_control_seed {
            let _ = control_tx.send(DropControlRequest::ProtocolClose {
                conn_id: self.sender.connection_id(),
                description,
            });
        }
    }

    pub fn new(handle: LaneHandle, handler: H) -> Self {
        Self::with_request_timeout_policy(handle, handler, RequestTimeoutPolicy::disabled())
    }

    // r[impl rpc.timeout.idle-progress]
    pub fn with_request_timeout_policy(
        handle: LaneHandle,
        handler: H,
        request_timeout: RequestTimeoutPolicy,
    ) -> Self {
        let conn_id = handle.connection_id();
        let LaneHandle {
            sender,
            rx,
            failures_rx,
            control_tx,
            closed_rx,
            local_settings,
            peer_settings,
            parity,
            observer,
        } = handle;
        let (local_control_tx, local_control_rx) = mpsc::unbounded_channel("driver.local_control");
        Self {
            sender,
            rx,
            failures_rx,
            closed_rx,
            local_control_rx,
            handler: Arc::new(handler),
            shared: Arc::new(DriverShared {
                connection_id: conn_id,
                pending_responses: SyncMutex::new("driver.pending_responses", BTreeMap::new()),
                request_ids: SyncMutex::new("driver.request_ids", IdAllocator::new(parity)),
                channel_ids: SyncMutex::new("driver.channel_ids", IdAllocator::new(parity)),
                channel_senders: SyncMutex::new("driver.channel_senders", BTreeMap::new()),
                channel_receivers: SyncMutex::new("driver.channel_receivers", BTreeMap::new()),
                channel_credits: SyncMutex::new("driver.channel_credits", BTreeMap::new()),
                channel_contexts: SyncMutex::new("driver.channel_contexts", BTreeMap::new()),
                request_scopes: SyncMutex::new("driver.request_scopes", BTreeMap::new()),
                request_timeout,
                channel_debug: SyncMutex::new("driver.channel_debug", BTreeMap::new()),
                last_inbound_message_at: SyncMutex::new("driver.last_inbound_message_at", None),
                last_outbound_message_at: SyncMutex::new("driver.last_outbound_message_at", None),
                close_reason: SyncMutex::new("driver.close_reason", None),
                terminal_channels: SyncMutex::new("driver.terminal_channels", HashSet::new()),
                channel_schema_roles: SyncMutex::new("driver.channel_schema_roles", HashMap::new()),
                local_initial_channel_credit: local_settings.initial_channel_credit,
                peer_initial_channel_credit: peer_settings.initial_channel_credit,
                outbound_request_limit: Semaphore::new(
                    "driver.outbound_request_limit",
                    peer_settings.max_concurrent_requests as usize,
                ),
                local_max_concurrent_requests: local_settings.max_concurrent_requests,
                peer_request_parity: peer_settings.parity,
                observer,
            }),
            in_flight_handlers: BTreeMap::new(),
            handler_futs: FuturesUnordered::new(),
            local_control_tx,
            drop_control_seed: control_tx,
            suppressed_failures: HashSet::new(),
        }
    }

    /// Get a cloneable caller handle for making outgoing calls.
    pub fn caller(&self) -> DriverCaller {
        DriverCaller {
            sender: self.sender.clone(),
            shared: Arc::clone(&self.shared),
            local_control_tx: self.local_control_tx.clone(),
            closed_rx: self.closed_rx.clone(),
        }
    }

    // r[impl rpc.debug.snapshot]
    pub fn debug_snapshot(&self) -> VoxDebugSnapshot {
        self.shared.debug_snapshot(
            &self.sender,
            self.shared
                .connection_debug_state(self.closed_rx.borrow().is_some()),
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
        }
    }

    // r[impl rpc.timeout.idle-progress]
    async fn expire_idle_requests(&mut self) {
        let expired = self.shared.expired_idle_request_ids();
        for req_id in expired {
            let pending_response = { self.shared.pending_responses.lock().remove(&req_id) };
            if let Some(tx) = pending_response {
                self.shared.finish_request(
                    req_id,
                    RequestDebugState::TimedOut,
                    RequestTerminationReason::TimedOut,
                );
                let _ = self
                    .sender
                    .send(ConnectionMessage::Request(RequestMessage {
                        id: req_id,
                        body: RequestBody::Cancel(RequestCancel {
                            metadata: Default::default(),
                        }),
                    }))
                    .await;
                let _ = tx.send(Err(VoxError::TimedOut));
                continue;
            }

            let Some(in_flight) = self.in_flight_handlers.remove(&req_id) else {
                self.shared.finish_request(
                    req_id,
                    RequestDebugState::TimedOut,
                    RequestTerminationReason::TimedOut,
                );
                continue;
            };

            self.suppressed_failures.insert(req_id);
            in_flight.abort.abort();
            self.shared.finish_request(
                req_id,
                RequestDebugState::TimedOut,
                RequestTerminationReason::TimedOut,
            );
            let response_shape = self
                .handler
                .response_wire_shape(in_flight.method_id)
                .map(|shape| (in_flight.method_id, shape));
            send_vox_error_response(
                self.sender.clone(),
                req_id,
                response_shape,
                VoxError::TimedOut,
            )
            .await;
        }
    }

    // r[impl rpc.pipelining]
    /// Main loop: receive messages from the session and dispatch them.
    /// Handler calls run as spawned tasks — we don't block the driver
    /// loop waiting for a handler to finish.
    pub async fn run(&mut self) {
        loop {
            tracing::trace!("driver select loop top");
            let idle_sleep_duration = self.shared.next_request_idle_sleep_duration();
            let has_idle_sleep = idle_sleep_duration.is_some();
            tokio::select! {
                biased;
                Some(ctrl) = self.local_control_rx.recv() => {
                    self.handle_local_control(ctrl).await;
                }
                _ = async {
                    if let Some(duration) = idle_sleep_duration {
                        vox_types::time::tokio::sleep(duration).await;
                    }
                }, if has_idle_sleep => {
                    self.expire_idle_requests().await;
                }
                Some((req_id, disposition)) = self.failures_rx.recv() => {
                    tracing::trace!(%req_id, ?disposition, "failures_rx fired");
                    if self.suppressed_failures.remove(&req_id) {
                        tracing::trace!(%req_id, "suppressing post-timeout reply-sink failure");
                        continue;
                    }
                    let in_flight_found = self.in_flight_handlers.contains_key(&req_id);
                    let in_flight_method_id =
                        self.in_flight_handlers.get(&req_id).map(|in_flight| in_flight.method_id);
                    let reply_disposition = self
                        .in_flight_handlers
                        .get(&req_id)
                        .map(|in_flight| {
                            let has_channels =
                                self.handler.args_have_channels(in_flight.method_id);
                            if has_channels {
                                Some(FailureDisposition::Indeterminate)
                            } else {
                                Some(disposition)
                            }
                        })
                        .unwrap_or(Some(disposition));
                    tracing::trace!(%req_id, in_flight_found, ?reply_disposition, "failures_rx computed disposition");
                    // Clean up the handler tracking entry.
                    self.in_flight_handlers.remove(&req_id);
                    let termination = match disposition {
                        FailureDisposition::Cancelled => RequestTerminationReason::Cancelled,
                        FailureDisposition::Indeterminate => RequestTerminationReason::Failed,
                    };
                    self.shared.finish_request(
                        req_id,
                        RequestDebugState::Failed,
                        termination,
                    );
                    tracing::trace!(%req_id, in_flight = self.in_flight_handlers.len(), "handler removed on failure");
                    let pending = self.shared.pending_responses.lock().remove(&req_id);
                    let had_pending = pending.is_some();
                    tracing::trace!(%req_id, had_pending, "failures_rx checked pending_responses");
                    let Some(reply_disposition) = reply_disposition else {
                        tracing::trace!(%req_id, "failures_rx: no reply_disposition, skipping");
                        continue;
                    };
                    tracing::trace!(%req_id, ?reply_disposition, "failures_rx: sending error response");
                    let vox_error = match reply_disposition {
                        FailureDisposition::Cancelled => VoxError::Cancelled,
                        FailureDisposition::Indeterminate => VoxError::Indeterminate,
                    };
                    if let Some(tx) = pending {
                        let _ = tx.send(Err(vox_error));
                    } else {
                        let response_shape = in_flight_method_id.and_then(|method_id| {
                            self.handler
                                .response_wire_shape(method_id)
                                .map(|shape| (method_id, shape))
                        });
                        send_vox_error_response(
                            self.sender.clone(),
                            req_id,
                            response_shape,
                            vox_error,
                        )
                        .await;
                    }
                    tracing::trace!(%req_id, "failures_rx: error response sent");
                }
                recv = self.rx.recv() => {
                    match recv {
                        Some(recv) => {
                            self.handle_recv(recv).await;
                        }
                        None => {
                            tracing::trace!("driver rx closed, exiting loop");
                            break;
                        }
                    }
                }
                // The handler-future arm only fires when at least one
                // handler is in flight. The guard is essential:
                // `FuturesUnordered::next` on an empty stream returns
                // `Poll::Ready(None)` immediately, which would spin the
                // select loop.
                Some(item) = self.handler_futs.next(), if !self.handler_futs.is_empty() => {
                    match item {
                        Ok(HandlerCompletion::Finished(req_id)) => {
                            let removed = self.in_flight_handlers.remove(&req_id).is_some();
                            self.shared.finish_request(
                                req_id,
                                RequestDebugState::Finished,
                                RequestTerminationReason::ResponseDelivered,
                            );
                            tracing::trace!(
                                %req_id,
                                removed,
                                in_flight = self.in_flight_handlers.len(),
                                "handler completion processed",
                            );
                        }
                        Ok(HandlerCompletion::Panicked { request_id, method_id }) => {
                            tracing::error!(
                                req_id = ?request_id,
                                ?method_id,
                                "vox driver handler panicked; waiting for reply-sink failure path"
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
            in_flight.abort.abort();
        }
        // r[impl rpc.flow-control.max-concurrent-requests.connection-failure]
        self.shared.outbound_request_limit.close();
        self.shared.pending_responses.lock().clear();
        self.shared.request_scopes.lock().clear();
        let close_reason =
            (*self.closed_rx.borrow()).unwrap_or(ConnectionCloseReason::ConnectionShutdown);
        self.shared.set_connection_closed(close_reason);

        // Connection is gone: drop channel runtime state so any registered Rx
        // receivers observe closure instead of hanging on recv(), and wake any
        // outbound Tx handles waiting for grant-credit.
        self.close_all_channel_runtime_state(close_reason);
    }

    async fn handle_local_control(&mut self, control: DriverLocalControl) {
        match control {
            DriverLocalControl::CloseChannel { channel_id } => {
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
                self.shared.channel_receivers.lock().remove(&channel_id);
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

    async fn handle_recv(&mut self, recv: crate::connection::RecvMessage) {
        self.shared.mark_inbound_progress();
        let crate::connection::RecvMessage { schemas, msg, fds } = recv;
        let msg_ref = msg.get();
        match msg_ref {
            ConnectionMessage::Request(req) => {
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
                let msg = msg.map(|m| match m {
                    ConnectionMessage::Request(r) => r,
                    _ => unreachable!(),
                });
                self.handle_request(msg, schemas, fds);
            }
            ConnectionMessage::Channel(_) => {
                let msg = msg.map(|m| match m {
                    ConnectionMessage::Channel(c) => c,
                    _ => unreachable!(),
                });
                self.handle_channel(msg).await;
            }
            ConnectionMessage::Schema(_) => {
                let msg = msg.map(|m| match m {
                    ConnectionMessage::Schema(schema) => schema,
                    _ => unreachable!(),
                });
                self.handle_schema(msg).await;
            }
        }
    }

    async fn handle_schema(&mut self, msg: SelfRef<vox_types::SchemaMessage>) {
        let schema = msg.get();
        let roles = self
            .shared
            .channel_schema_roles_for(schema.method_id, schema.direction);
        for (role, channels) in roles {
            // r[impl schema.exchange.channels.tx-args]
            let bundle = match vox_types::writer_auxiliary_schema_bundle_from_bytes(
                &schema.schemas.0,
                &role,
            ) {
                Ok(Some(bundle)) => Arc::new(bundle),
                Ok(None) => continue,
                Err(error) => {
                    tracing::debug!(
                        method_id = ?schema.method_id,
                        direction = ?schema.direction,
                        role,
                        "failed to parse channel writer schema: {error}"
                    );
                    continue;
                }
            };
            for channel_id in channels {
                let sender = self.shared.inbound_channel_sender(channel_id);
                let _ = sender
                    .send(IncomingChannelMessage::WriterSchema(Arc::clone(&bundle)))
                    .await;
            }
        }
    }

    fn handle_request(
        &mut self,
        msg: SelfRef<RequestMessage<'static>>,
        schemas: Arc<vox_types::SchemaRecvTracker>,
        fds: vox_types::FrameFds,
    ) {
        let msg_ref = msg.get();
        let req_id = msg_ref.id;
        let is_call = matches!(&msg_ref.body, RequestBody::Call(_));
        let is_response = matches!(&msg_ref.body, RequestBody::Response(_));
        let is_cancel = matches!(&msg_ref.body, RequestBody::Cancel(_));

        if is_call {
            if !req_id.has_parity(self.shared.peer_request_parity) {
                // r[impl rpc.request.id-allocation]
                self.protocol_violation(format!(
                    "request id {:?} does not match peer parity {:?}",
                    req_id, self.shared.peer_request_parity
                ));
                return;
            }
            if self.in_flight_handlers.contains_key(&req_id) {
                // r[impl rpc.request.id-allocation]
                self.protocol_violation(format!("duplicate live request id {:?}", req_id));
                return;
            }
            if self.in_flight_handlers.len() >= self.shared.local_max_concurrent_requests as usize {
                // r[impl rpc.flow-control.max-concurrent-requests.inbound]
                self.protocol_violation(format!(
                    "max_concurrent_requests exceeded for request id {:?} (limit {}, in-flight {})",
                    req_id,
                    self.shared.local_max_concurrent_requests,
                    self.in_flight_handlers.len()
                ));
                return;
            }

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
            let method_id = call_ref.method_id;
            let associated_channels = call_ref.channels.clone();

            let reply = DriverReplySink {
                sender: Some(self.sender.clone()),
                request_id: req_id,
                method_id: call_ref.method_id,
                binder: self.internal_binder(),
            };
            self.shared.start_request(
                req_id,
                method_id,
                None,
                None,
                RequestDebugState::Dispatching,
                associated_channels,
            );
            let (abort, abort_reg) = AbortHandle::new_pair();
            let handler_fut: Pin<Box<dyn MaybeSendFuture<Output = HandlerCompletion> + 'static>> =
                Box::pin(async move {
                    tracing::debug!(
                        req_id = ?req_id,
                        method_id = ?method_id,
                        "vox driver handler starting"
                    );
                    vox_types::dlog!(
                        "[driver] handler start: req={:?} method={:?}",
                        req_id,
                        method_id
                    );
                    let result = AssertUnwindSafe(handler.handle(call, reply, schemas))
                        .catch_unwind()
                        .await;
                    if result.is_err() {
                        return HandlerCompletion::Panicked {
                            request_id: req_id,
                            method_id,
                        };
                    }
                    tracing::debug!(
                        req_id = ?req_id,
                        method_id = ?method_id,
                        "vox driver handler finished"
                    );
                    vox_types::dlog!(
                        "[driver] handler done: req={:?} method={:?}",
                        req_id,
                        method_id
                    );
                    HandlerCompletion::Finished(req_id)
                });
            self.handler_futs
                .push(Abortable::new(handler_fut, abort_reg));
            self.in_flight_handlers
                .insert(req_id, InFlightHandler { abort, method_id });
            tracing::trace!(%req_id, in_flight = self.in_flight_handlers.len(), "handler inserted");
        } else if is_response {
            vox_types::dlog!(
                "[driver] inbound response: conn={:?} req={:?}",
                self.sender.connection_id(),
                req_id
            );
            tracing::trace!(%req_id, "driver received response");
            if let Some(tx) = self.shared.pending_responses.lock().remove(&req_id) {
                vox_types::dlog!("[driver] routing response to waiter: req={:?}", req_id);
                tracing::trace!(%req_id, "routing response to pending oneshot");
                self.shared.mark_request_progress(req_id);
                let _: Result<(), _> = tx.send(Ok(PendingResponse { msg, schemas, fds }));
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
            // A cancel aborts the in-flight handler for this request, if any.
            if let Some(in_flight) = self.in_flight_handlers.remove(&req_id) {
                tracing::trace!(%req_id, "aborting handler");
                self.shared.mark_request_progress(req_id);
                in_flight.abort.abort();
                self.shared.finish_request(
                    req_id,
                    RequestDebugState::Failed,
                    RequestTerminationReason::Cancelled,
                );
                tracing::trace!(%req_id, in_flight = self.in_flight_handlers.len(), "handler removed on cancel");
            }
            // The response is sent automatically: aborting drops DriverReplySink →
            // mark_failure fires → failures_rx arm sends VoxError::Cancelled.
        }
    }

    async fn handle_channel(&mut self, msg: SelfRef<ChannelMessage<'static>>) {
        let msg_ref = msg.get();
        let chan_id = msg_ref.id;
        enum ChannelBodyKind {
            Item,
            Close,
            Reset,
            GrantCredit(u32),
        }
        let body_kind = match &msg_ref.body {
            ChannelBody::Item(_) => ChannelBodyKind::Item,
            ChannelBody::Close(_) => ChannelBodyKind::Close,
            ChannelBody::Reset(_) => ChannelBodyKind::Reset,
            ChannelBody::GrantCredit(grant) => ChannelBodyKind::GrantCredit(grant.additional),
        };

        match body_kind {
            // r[impl rpc.channel.item]
            // r[impl rpc.channel.delivery.reliable]
            ChannelBodyKind::Item => {
                if self.shared.terminal_channels.lock().contains(&chan_id) {
                    self.shared.record_inbound_item_not_enqueued(chan_id);
                    tracing::trace!(
                        conn_id = self.sender.connection_id().0,
                        channel_id = chan_id.0,
                        "driver dropped item for terminal channel"
                    );
                    return;
                }

                tracing::trace!(
                    conn_id = self.sender.connection_id().0,
                    channel_id = chan_id.0,
                    "driver received channel item"
                );
                let item = msg.map(|m| match m.body {
                    ChannelBody::Item(item) => item,
                    _ => unreachable!(),
                });
                let sender = self.shared.inbound_channel_sender(chan_id);
                if sender
                    .send(IncomingChannelMessage::Item(item))
                    .await
                    .is_err()
                {
                    self.shared.record_inbound_item_not_enqueued(chan_id);
                    self.shared.channel_senders.lock().remove(&chan_id);
                    self.shared.channel_receivers.lock().remove(&chan_id);
                    self.close_outbound_channel(chan_id);
                    let _ = self
                        .local_control_tx
                        .send(DriverLocalControl::ResetChannel {
                            channel_id: chan_id,
                        });
                    return;
                }
                self.shared
                    .observe_channel(chan_id, None, |channel| ChannelEvent::ItemReceived {
                        channel,
                    });
            }
            // r[impl rpc.channel.close]
            ChannelBodyKind::Close => {
                if self.shared.terminal_channels.lock().contains(&chan_id) {
                    return;
                }
                let sender = self.shared.inbound_channel_sender(chan_id);
                tracing::trace!(
                    conn_id = self.sender.connection_id().0,
                    channel_id = chan_id.0,
                    "driver received channel close"
                );
                let close = msg.map(|m| match m.body {
                    ChannelBody::Close(close) => close,
                    _ => unreachable!(),
                });
                let delivered = sender
                    .send(IncomingChannelMessage::Close(close))
                    .await
                    .is_ok();
                self.shared.channel_senders.lock().remove(&chan_id);
                self.shared.terminal_channels.lock().insert(chan_id);
                self.close_outbound_channel(chan_id);
                if !delivered {
                    self.shared.channel_receivers.lock().remove(&chan_id);
                    return;
                }
                self.shared
                    .observe_channel(chan_id, None, |channel| ChannelEvent::Closed {
                        channel,
                        reason: ChannelCloseReason::Remote,
                    });
            }
            // r[impl rpc.channel.reset]
            ChannelBodyKind::Reset => {
                if self.shared.terminal_channels.lock().contains(&chan_id) {
                    return;
                }
                let sender = self.shared.inbound_channel_sender(chan_id);
                tracing::trace!(
                    conn_id = self.sender.connection_id().0,
                    channel_id = chan_id.0,
                    "driver received channel reset"
                );
                let reset = msg.map(|m| match m.body {
                    ChannelBody::Reset(reset) => reset,
                    _ => unreachable!(),
                });
                let delivered = sender
                    .send(IncomingChannelMessage::Reset(reset))
                    .await
                    .is_ok();
                self.shared.channel_senders.lock().remove(&chan_id);
                self.shared.terminal_channels.lock().insert(chan_id);
                self.close_outbound_channel(chan_id);
                if !delivered {
                    self.shared.channel_receivers.lock().remove(&chan_id);
                    return;
                }
                self.shared
                    .observe_channel(chan_id, None, |channel| ChannelEvent::Reset {
                        channel,
                        reason: ChannelResetReason::Remote,
                    });
            }
            // r[impl rpc.flow-control.credit.grant]
            // r[impl rpc.flow-control.credit.grant.additive]
            ChannelBodyKind::GrantCredit(additional) => {
                self.shared.record_credit_received(chan_id, additional);
                self.shared.emit_channel_event(chan_id, None, |channel| {
                    ChannelEvent::CreditGranted {
                        channel,
                        amount: additional,
                    }
                });
                tracing::trace!(
                    conn_id = self.sender.connection_id().0,
                    channel_id = chan_id.0,
                    additional,
                    "driver received channel credit"
                );
                if let Some(semaphore) = self.shared.channel_credits.lock().get(&chan_id) {
                    semaphore.add_permits(additional as usize);
                }
            }
        }
    }
}
