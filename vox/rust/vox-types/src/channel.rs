use std::marker::PhantomData;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use facet::Facet;
use facet_core::PtrConst;
use moire::sync::{Notify, Semaphore, mpsc};

use crate::ChannelId;
use crate::{Backing, ChannelClose, ChannelItem, ChannelReset, Metadata, Payload, SelfRef};

// ---------------------------------------------------------------------------
// Thread-local channel binder — set during deserialization so TryFrom impls
// can bind channels immediately.
// ---------------------------------------------------------------------------

std::thread_local! {
    static CHANNEL_BINDER: std::cell::RefCell<Option<&'static dyn ChannelBinder>> =
        const { std::cell::RefCell::new(None) };
}

/// Set the thread-local channel binder for the duration of `f`.
///
/// Any `Tx<T>` or `Rx<T>` deserialized (via `TryFrom<ChannelId>`) during `f`
/// will be bound through this binder.
pub fn with_channel_binder<R>(binder: &dyn ChannelBinder, f: impl FnOnce() -> R) -> R {
    // SAFETY: we restore the previous value (always None in practice) on exit,
    // so the binder reference doesn't escape the closure's lifetime.
    #[allow(unsafe_code)]
    let static_ref: &'static dyn ChannelBinder = unsafe { std::mem::transmute(binder) };
    CHANNEL_BINDER.with(|cell| {
        let prev = cell.borrow_mut().replace(static_ref);
        let result = f();
        *cell.borrow_mut() = prev;
        result
    })
}

// r[impl rpc.channel.pair]
/// The binding stored in a channel core — either a sink or a receiver, never both.
pub enum ChannelBinding {
    Sink(BoundChannelSink),
    Receiver(BoundChannelReceiver),
}

pub trait ChannelLiveness: crate::MaybeSend + crate::MaybeSync + 'static {}

impl<T: crate::MaybeSend + crate::MaybeSync + 'static> ChannelLiveness for T {}

pub type ChannelLivenessHandle = Arc<dyn ChannelLiveness>;

pub trait ChannelCreditReplenisher: crate::MaybeSend + crate::MaybeSync + 'static {
    fn on_item_consumed(&self);
}

pub type ChannelCreditReplenisherHandle = Arc<dyn ChannelCreditReplenisher>;

#[derive(Clone)]
pub struct BoundChannelSink {
    pub sink: Arc<dyn ChannelSink>,
    pub liveness: Option<ChannelLivenessHandle>,
}

pub struct BoundChannelReceiver {
    pub receiver: mpsc::Receiver<IncomingChannelMessage>,
    pub liveness: Option<ChannelLivenessHandle>,
    pub replenisher: Option<ChannelCreditReplenisherHandle>,
}

struct LogicalReceiverState {
    generation: u64,
    liveness: Option<ChannelLivenessHandle>,
    sender: Option<mpsc::Sender<LogicalIncomingChannelMessage>>,
    receiver: Option<mpsc::Receiver<LogicalIncomingChannelMessage>>,
}

// r[impl rpc.channel.pair]
/// Shared state between a `Tx`/`Rx` pair created by `channel()`.
///
/// Contains a `Mutex<Option<ChannelBinding>>` that is written once during
/// binding and read/taken by the paired handle. The mutex is only locked
/// during binding (once) and on first use by the paired handle (once).
pub struct ChannelCore {
    binding: Mutex<Option<ChannelBinding>>,
    logical_receiver: Mutex<Option<LogicalReceiverState>>,
    binding_changed: Notify,
}

impl ChannelCore {
    fn new() -> Self {
        Self {
            binding: Mutex::new(None),
            logical_receiver: Mutex::new(None),
            binding_changed: Notify::new("vox_types.channel.binding_changed"),
        }
    }

    /// Store or replace a binding in the core.
    pub fn set_binding(&self, binding: ChannelBinding) {
        let mut guard = self.binding.lock().expect("channel core mutex poisoned");
        *guard = Some(binding);
        self.binding_changed.notify_waiters();
    }

    /// Clone the sink from the core (for Tx reading the sink).
    /// Returns None if no sink has been set or if the binding is a Receiver.
    pub fn get_sink(&self) -> Option<Arc<dyn ChannelSink>> {
        let guard = self.binding.lock().expect("channel core mutex poisoned");
        match guard.as_ref() {
            Some(ChannelBinding::Sink(bound)) => Some(bound.sink.clone()),
            _ => None,
        }
    }

    /// Take the receiver out of the core (for Rx on first recv).
    /// Returns None if no receiver has been set or if it was already taken.
    pub fn take_receiver(&self) -> Option<BoundChannelReceiver> {
        let mut guard = self.binding.lock().expect("channel core mutex poisoned");
        match guard.take() {
            Some(ChannelBinding::Receiver(bound)) => Some(bound),
            other => {
                // Put it back if it wasn't a receiver
                *guard = other;
                None
            }
        }
    }

    pub fn bind_retryable_receiver(self: &Arc<Self>, bound: BoundChannelReceiver) {
        #[cfg(not(target_arch = "wasm32"))]
        if tokio::runtime::Handle::try_current().is_err() {
            self.set_binding(ChannelBinding::Receiver(bound));
            return;
        }

        let mut guard = self
            .logical_receiver
            .lock()
            .expect("channel core logical receiver mutex poisoned");
        let state = guard.get_or_insert_with(|| {
            let (tx, rx) = mpsc::channel("vox_types.channel.logical_receiver", 64);
            LogicalReceiverState {
                generation: 0,
                liveness: None,
                sender: Some(tx),
                receiver: Some(rx),
            }
        });
        state.generation = state.generation.wrapping_add(1);
        state.liveness = bound.liveness.clone();
        let generation = state.generation;

        let Some(sender) = state.sender.clone() else {
            return;
        };

        self.binding_changed.notify_waiters();

        drop(guard);
        let core = Arc::clone(self);

        moire::task::spawn(async move {
            let mut receiver = bound.receiver;
            let replenisher = bound.replenisher.clone();
            while let Some(msg) = receiver.recv().await {
                let is_current_generation = {
                    let guard = core
                        .logical_receiver
                        .lock()
                        .expect("channel core logical receiver mutex poisoned");
                    guard
                        .as_ref()
                        .map(|state| state.generation == generation)
                        .unwrap_or(false)
                };
                if !is_current_generation {
                    return;
                }
                let forwarded = LogicalIncomingChannelMessage {
                    msg,
                    replenisher: replenisher.clone(),
                };
                if sender.send(forwarded).await.is_err() {
                    return;
                }
            }
        });
    }

    pub fn take_logical_receiver(
        &self,
    ) -> Option<(
        mpsc::Receiver<LogicalIncomingChannelMessage>,
        Option<ChannelLivenessHandle>,
    )> {
        self.logical_receiver
            .lock()
            .expect("channel core logical receiver mutex poisoned")
            .as_mut()
            .and_then(|state| {
                state
                    .receiver
                    .take()
                    .map(|receiver| (receiver, state.liveness.clone()))
            })
    }

    pub fn finish_retry_binding(&self) {
        let mut guard = self
            .logical_receiver
            .lock()
            .expect("channel core logical receiver mutex poisoned");
        if let Some(state) = guard.as_mut() {
            if let Some(sender) = state.sender.as_ref() {
                let close = SelfRef::owning(
                    Backing::Boxed(Box::<[u8]>::default()),
                    ChannelClose {
                        metadata: Metadata::default(),
                    },
                );
                let _ = sender.try_send(LogicalIncomingChannelMessage {
                    msg: IncomingChannelMessage::Close(close),
                    replenisher: None,
                });
            }
            state.sender.take();
        }
        *guard = None;
        let mut guard = self.binding.lock().expect("channel core mutex poisoned");
        *guard = None;
        self.binding_changed.notify_waiters();
    }
}

/// Slot for the shared channel core, accessible via facet reflection.
#[derive(Facet)]
#[facet(opaque)]
pub(crate) struct CoreSlot {
    pub(crate) inner: Option<Arc<ChannelCore>>,
}

impl CoreSlot {
    pub(crate) fn empty() -> Self {
        Self { inner: None }
    }
}

// r[impl rpc.channel.pair]
/// Create a channel pair with shared state.
///
/// Both ends hold an `Arc` reference to the same `ChannelCore`. The framework
/// binds the handle that appears in args or return values, and the paired
/// handle reads or takes the binding from the shared core.
pub fn channel<T>() -> (Tx<T>, Rx<T>) {
    let core = Arc::new(ChannelCore::new());
    (Tx::paired(core.clone()), Rx::paired(core))
}

/// Runtime sink implemented by the session driver.
///
/// The contract is strict: successful completion means the item has gone
/// through the conduit to the link commit boundary.
pub trait ChannelSink: crate::MaybeSend + crate::MaybeSync + 'static {
    fn send_payload<'payload>(
        &self,
        payload: Payload<'payload>,
    ) -> Pin<Box<dyn crate::MaybeSendFuture<Output = Result<(), TxError>> + 'payload>>;

    fn close_channel(
        &self,
        metadata: Metadata,
    ) -> Pin<Box<dyn crate::MaybeSendFuture<Output = Result<(), TxError>> + 'static>>;

    /// Synchronous drop-time close signal.
    ///
    /// This is used by `Tx::drop` to notify the runtime immediately without
    /// spawning detached tasks. Implementations should enqueue a close intent
    /// to their runtime/driver if possible.
    fn close_channel_on_drop(&self) {}
}

// r[impl rpc.flow-control.credit]
// r[impl rpc.flow-control.credit.exhaustion]
/// A [`ChannelSink`] wrapper that enforces credit-based flow control.
///
/// Each `send_payload` acquires one permit from the semaphore, blocking if
/// credit is zero. The semaphore is shared with the driver so that incoming
/// `GrantCredit` messages can add permits via [`CreditSink::credit`].
pub struct CreditSink<S: ChannelSink> {
    inner: S,
    credit: Arc<Semaphore>,
}

impl<S: ChannelSink> CreditSink<S> {
    // r[impl rpc.flow-control.credit.initial]
    // r[impl rpc.flow-control.credit.initial.zero]
    /// Wrap `inner` with `initial_credit` permits (the const generic `N`).
    pub fn new(inner: S, initial_credit: u32) -> Self {
        Self {
            inner,
            credit: Arc::new(Semaphore::new(
                "vox_types.channel.credit",
                initial_credit as usize,
            )),
        }
    }

    /// Returns the credit semaphore. The driver holds a clone so
    /// `GrantCredit` messages can call `add_permits`.
    pub fn credit(&self) -> &Arc<Semaphore> {
        &self.credit
    }
}

impl<S: ChannelSink> ChannelSink for CreditSink<S> {
    fn send_payload<'payload>(
        &self,
        payload: Payload<'payload>,
    ) -> Pin<Box<dyn crate::MaybeSendFuture<Output = Result<(), TxError>> + 'payload>> {
        let credit = self.credit.clone();
        let fut = self.inner.send_payload(payload);
        Box::pin(async move {
            let permit = credit
                .acquire_owned()
                .await
                .map_err(|_| TxError::Transport("channel credit semaphore closed".into()))?;
            std::mem::forget(permit);
            fut.await
        })
    }

    fn close_channel(
        &self,
        metadata: Metadata,
    ) -> Pin<Box<dyn crate::MaybeSendFuture<Output = Result<(), TxError>> + 'static>> {
        // Close does not consume credit — it's a control message.
        self.inner.close_channel(metadata)
    }

    fn close_channel_on_drop(&self) {
        self.inner.close_channel_on_drop();
    }
}

/// Message delivered to an `Rx` by the driver.
pub enum IncomingChannelMessage {
    Item(SelfRef<ChannelItem<'static>>),
    Close(SelfRef<ChannelClose<'static>>),
    Reset(SelfRef<ChannelReset<'static>>),
}

pub struct LogicalIncomingChannelMessage {
    pub msg: IncomingChannelMessage,
    pub replenisher: Option<ChannelCreditReplenisherHandle>,
}

/// Sender-side runtime slot.
#[derive(Facet)]
#[facet(opaque)]
pub(crate) struct SinkSlot {
    pub(crate) inner: Option<Arc<dyn ChannelSink>>,
}

impl SinkSlot {
    pub(crate) fn empty() -> Self {
        Self { inner: None }
    }
}

/// Opaque liveness retention slot for bound channel handles.
#[derive(Facet)]
#[facet(opaque)]
pub(crate) struct LivenessSlot {
    pub(crate) inner: Option<ChannelLivenessHandle>,
}

impl LivenessSlot {
    pub(crate) fn empty() -> Self {
        Self { inner: None }
    }
}

/// Receiver-side runtime slot.
#[derive(Facet)]
#[facet(opaque)]
pub(crate) struct ReceiverSlot {
    pub(crate) inner: Option<mpsc::Receiver<IncomingChannelMessage>>,
}

impl ReceiverSlot {
    pub(crate) fn empty() -> Self {
        Self { inner: None }
    }
}

#[derive(Facet)]
#[facet(opaque)]
pub(crate) struct LogicalReceiverSlot {
    pub(crate) inner: Option<mpsc::Receiver<LogicalIncomingChannelMessage>>,
}

impl LogicalReceiverSlot {
    pub(crate) fn empty() -> Self {
        Self { inner: None }
    }
}

/// Receiver-side credit replenishment slot.
#[derive(Facet)]
#[facet(opaque)]
pub(crate) struct ReplenisherSlot {
    pub(crate) inner: Option<ChannelCreditReplenisherHandle>,
}

impl ReplenisherSlot {
    pub(crate) fn empty() -> Self {
        Self { inner: None }
    }
}

/// Sender handle: "I send". The holder of a `Tx<T>` sends items of type `T`.
///
/// In method args, the handler holds it (handler sends → caller).
///
/// Wire encoding is always unit (`()`), with channel IDs carried exclusively
/// in `Message::Request.channels`.
// r[impl rpc.channel]
// r[impl rpc.channel.direction]
// r[impl rpc.channel.payload-encoding]
#[derive(Facet)]
#[facet(proxy = crate::ChannelId)]
pub struct Tx<T> {
    pub(crate) channel_id: ChannelId,
    pub(crate) sink: SinkSlot,
    pub(crate) core: CoreSlot,
    pub(crate) liveness: LivenessSlot,
    #[facet(opaque)]
    closed: AtomicBool,
    #[facet(opaque)]
    _marker: PhantomData<T>,
}

impl<T> Tx<T> {
    /// Create a standalone unbound Tx (used by deserialization).
    pub fn unbound() -> Self {
        Self {
            channel_id: ChannelId::RESERVED,
            sink: SinkSlot::empty(),
            core: CoreSlot::empty(),
            liveness: LivenessSlot::empty(),
            closed: AtomicBool::new(false),
            _marker: PhantomData,
        }
    }

    /// Create a Tx that is part of a `channel()` pair.
    fn paired(core: Arc<ChannelCore>) -> Self {
        Self {
            channel_id: ChannelId::RESERVED,
            sink: SinkSlot::empty(),
            core: CoreSlot { inner: Some(core) },
            liveness: LivenessSlot::empty(),
            closed: AtomicBool::new(false),
            _marker: PhantomData,
        }
    }

    pub fn is_bound(&self) -> bool {
        if self.sink.inner.is_some() {
            return true;
        }
        if let Some(core) = &self.core.inner {
            return core.get_sink().is_some();
        }
        false
    }

    /// Check if this Tx is part of a channel() pair (has a shared core).
    pub fn has_core(&self) -> bool {
        self.core.inner.is_some()
    }

    // r[impl rpc.channel.pair.tx-read]
    fn resolve_sink_now(&self) -> Option<Arc<dyn ChannelSink>> {
        // Fast path: local slot (standalone/callee-side handle)
        if let Some(sink) = &self.sink.inner {
            return Some(sink.clone());
        }
        // Slow path: read from shared core (paired handle)
        if let Some(core) = &self.core.inner
            && let Some(sink) = core.get_sink()
        {
            return Some(sink);
        }
        None
    }

    pub async fn send<'value>(&self, value: T) -> Result<(), TxError>
    where
        T: Facet<'value>,
    {
        let sink = if let Some(sink) = self.resolve_sink_now() {
            sink
        } else if let Some(core) = &self.core.inner {
            loop {
                let notified = core.binding_changed.notified();
                if let Some(sink) = self.resolve_sink_now() {
                    break sink;
                }
                notified.await;
            }
        } else {
            return Err(TxError::Unbound);
        };
        let ptr = PtrConst::new((&value as *const T).cast::<u8>());
        // SAFETY: `value` is explicitly dropped only after `await`, so the pointer
        // remains valid for the whole send operation.
        let payload = unsafe { Payload::outgoing_unchecked(ptr, T::SHAPE) };
        let result = sink.send_payload(payload).await;
        drop(value);
        result
    }

    // r[impl rpc.channel.lifecycle]
    pub async fn close<'value>(&self, metadata: Metadata<'value>) -> Result<(), TxError> {
        self.closed.store(true, Ordering::Release);
        let sink = if let Some(sink) = self.resolve_sink_now() {
            sink
        } else if let Some(core) = &self.core.inner {
            loop {
                let notified = core.binding_changed.notified();
                if let Some(sink) = self.resolve_sink_now() {
                    break sink;
                }
                notified.await;
            }
        } else {
            return Err(TxError::Unbound);
        };
        sink.close_channel(metadata).await
    }

    #[doc(hidden)]
    pub fn bind(&mut self, sink: Arc<dyn ChannelSink>) {
        self.bind_with_liveness(sink, None);
    }

    #[doc(hidden)]
    pub fn bind_with_liveness(
        &mut self,
        sink: Arc<dyn ChannelSink>,
        liveness: Option<ChannelLivenessHandle>,
    ) {
        self.sink.inner = Some(sink);
        self.liveness.inner = liveness;
    }

    #[doc(hidden)]
    pub fn finish_retry_binding(&self) {
        if let Some(core) = &self.core.inner {
            core.finish_retry_binding();
        }
    }
}

impl<T> Drop for Tx<T> {
    fn drop(&mut self) {
        if self.closed.swap(true, Ordering::AcqRel) {
            return;
        }

        let sink = if let Some(sink) = &self.sink.inner {
            Some(sink.clone())
        } else if let Some(core) = &self.core.inner {
            core.get_sink()
        } else {
            None
        };

        let Some(sink) = sink else {
            return;
        };

        // Synchronous signal into the runtime/driver; no detached async work here.
        sink.close_channel_on_drop();
    }
}

impl<T> TryFrom<&Tx<T>> for ChannelId {
    type Error = String;

    fn try_from(value: &Tx<T>) -> Result<Self, Self::Error> {
        // Case 1: Caller passes Tx in args (callee sends, caller receives).
        // Allocate a channel ID and store the receiver binding in the shared
        // core so the caller's paired Rx can pick it up.
        CHANNEL_BINDER.with(|cell| {
            let borrow = cell.borrow();
            let Some(binder) = *borrow else {
                return Err("serializing Tx requires an active ChannelBinder".to_string());
            };
            let (channel_id, bound) = binder.create_rx();
            if let Some(core) = &value.core.inner {
                core.bind_retryable_receiver(bound);
            }
            Ok(channel_id)
        })
    }
}

impl<T> TryFrom<ChannelId> for Tx<T> {
    type Error = String;

    fn try_from(channel_id: ChannelId) -> Result<Self, Self::Error> {
        let mut tx = Self::unbound();
        tx.channel_id = channel_id;

        CHANNEL_BINDER.with(|cell| {
            let Some(binder) = *cell.borrow() else {
                return Err("deserializing Tx requires an active ChannelBinder".to_string());
            };
            let sink = binder.bind_tx(channel_id);
            let liveness = binder.channel_liveness();
            tx.bind_with_liveness(sink, liveness);
            Ok(())
        })?;

        Ok(tx)
    }
}

/// Error when sending on a `Tx`.
#[derive(Debug)]
pub enum TxError {
    Unbound,
    Transport(String),
}

impl std::fmt::Display for TxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unbound => write!(f, "channel is not bound"),
            Self::Transport(msg) => write!(f, "transport error: {msg}"),
        }
    }
}

impl std::error::Error for TxError {}

/// Receiver handle: "I receive". The holder of an `Rx<T>` receives items of type `T`.
///
/// In method args, the handler holds it (handler receives ← caller).
///
/// Channel IDs are serialized inline in the postcard payload.
#[derive(Facet)]
#[facet(proxy = crate::ChannelId)]
pub struct Rx<T> {
    pub(crate) channel_id: ChannelId,
    pub(crate) receiver: ReceiverSlot,
    pub(crate) logical_receiver: LogicalReceiverSlot,
    pub(crate) core: CoreSlot,
    pub(crate) liveness: LivenessSlot,
    pub(crate) replenisher: ReplenisherSlot,
    #[facet(opaque)]
    _marker: PhantomData<T>,
}

impl<T> Rx<T> {
    /// Create a standalone unbound Rx (used by deserialization).
    pub fn unbound() -> Self {
        Self {
            channel_id: ChannelId::RESERVED,
            receiver: ReceiverSlot::empty(),
            logical_receiver: LogicalReceiverSlot::empty(),
            core: CoreSlot::empty(),
            liveness: LivenessSlot::empty(),
            replenisher: ReplenisherSlot::empty(),
            _marker: PhantomData,
        }
    }

    /// Create an Rx that is part of a `channel()` pair.
    fn paired(core: Arc<ChannelCore>) -> Self {
        Self {
            channel_id: ChannelId::RESERVED,
            receiver: ReceiverSlot::empty(),
            logical_receiver: LogicalReceiverSlot::empty(),
            core: CoreSlot { inner: Some(core) },
            liveness: LivenessSlot::empty(),
            replenisher: ReplenisherSlot::empty(),
            _marker: PhantomData,
        }
    }

    pub fn is_bound(&self) -> bool {
        self.receiver.inner.is_some()
    }

    /// Check if this Rx is part of a channel() pair (has a shared core).
    pub fn has_core(&self) -> bool {
        self.core.inner.is_some()
    }

    // r[impl rpc.channel.pair.rx-take]
    pub async fn recv(&mut self) -> Result<Option<SelfRef<T>>, RxError>
    where
        T: Facet<'static>,
    {
        loop {
            if self.logical_receiver.inner.is_none()
                && let Some(core) = &self.core.inner
                && let Some((receiver, liveness)) = core.take_logical_receiver()
            {
                self.logical_receiver.inner = Some(receiver);
                self.liveness.inner = liveness;
            }

            if let Some(receiver) = self.logical_receiver.inner.as_mut() {
                match receiver.recv().await {
                    Some(LogicalIncomingChannelMessage {
                        msg: IncomingChannelMessage::Close(_),
                        ..
                    })
                    | None => return Ok(None),
                    Some(LogicalIncomingChannelMessage {
                        msg: IncomingChannelMessage::Reset(_),
                        ..
                    }) => return Err(RxError::Reset),
                    Some(LogicalIncomingChannelMessage {
                        msg: IncomingChannelMessage::Item(msg),
                        replenisher,
                    }) => {
                        let value = msg
                            .try_repack(|item, _backing_bytes| {
                                let Payload::PostcardBytes(bytes) = item.item else {
                                    return Err(RxError::Protocol(
                                        "incoming channel item payload was not Incoming".into(),
                                    ));
                                };
                                vox_postcard::from_slice_borrowed(bytes)
                                    .map_err(RxError::Deserialize)
                            })
                            .map(Some);
                        if value.is_ok()
                            && let Some(replenisher) = replenisher.as_ref()
                        {
                            replenisher.on_item_consumed();
                        }
                        return value;
                    }
                }
            }

            if self.receiver.inner.is_none()
                && let Some(core) = &self.core.inner
                && let Some(bound) = core.take_receiver()
            {
                self.receiver.inner = Some(bound.receiver);
                self.liveness.inner = bound.liveness;
                self.replenisher.inner = bound.replenisher;
            }

            if let Some(receiver) = self.receiver.inner.as_mut() {
                return match receiver.recv().await {
                    Some(IncomingChannelMessage::Close(_)) | None => Ok(None),
                    Some(IncomingChannelMessage::Reset(_)) => Err(RxError::Reset),
                    Some(IncomingChannelMessage::Item(msg)) => {
                        let value = msg
                            .try_repack(|item, _backing_bytes| {
                                let Payload::PostcardBytes(bytes) = item.item else {
                                    return Err(RxError::Protocol(
                                        "incoming channel item payload was not Incoming".into(),
                                    ));
                                };
                                vox_postcard::from_slice_borrowed(bytes)
                                    .map_err(RxError::Deserialize)
                            })
                            .map(Some);
                        if value.is_ok()
                            && let Some(replenisher) = &self.replenisher.inner
                        {
                            replenisher.on_item_consumed();
                        }
                        value
                    }
                };
            }

            let Some(core) = &self.core.inner else {
                return Err(RxError::Unbound);
            };
            core.binding_changed.notified().await;
        }
    }

    #[doc(hidden)]
    pub fn bind(&mut self, receiver: mpsc::Receiver<IncomingChannelMessage>) {
        self.bind_with_liveness(receiver, None);
    }

    #[doc(hidden)]
    pub fn bind_with_liveness(
        &mut self,
        receiver: mpsc::Receiver<IncomingChannelMessage>,
        liveness: Option<ChannelLivenessHandle>,
    ) {
        self.receiver.inner = Some(receiver);
        self.logical_receiver.inner = None;
        self.liveness.inner = liveness;
        self.replenisher.inner = None;
    }
}

impl<T> TryFrom<&Rx<T>> for ChannelId {
    type Error = String;

    fn try_from(value: &Rx<T>) -> Result<Self, Self::Error> {
        // Case 2: Caller passes Rx in args (callee receives, caller sends).
        // Allocate a channel ID and store the sink binding in the shared
        // core so the caller's paired Tx can pick it up.
        CHANNEL_BINDER.with(|cell| {
            let borrow = cell.borrow();
            let Some(binder) = *borrow else {
                return Err("serializing Rx requires an active ChannelBinder".to_string());
            };
            let (channel_id, sink) = binder.create_tx();
            let liveness = binder.channel_liveness();
            if let Some(core) = &value.core.inner {
                core.set_binding(ChannelBinding::Sink(BoundChannelSink { sink, liveness }));
            }
            Ok(channel_id)
        })
    }
}

impl<T> TryFrom<ChannelId> for Rx<T> {
    type Error = String;

    fn try_from(channel_id: ChannelId) -> Result<Self, Self::Error> {
        let mut rx = Self::unbound();
        rx.channel_id = channel_id;

        CHANNEL_BINDER.with(|cell| {
            let Some(binder) = *cell.borrow() else {
                return Err("deserializing Rx requires an active ChannelBinder".to_string());
            };
            let bound = binder.register_rx(channel_id);
            rx.receiver.inner = Some(bound.receiver);
            rx.liveness.inner = bound.liveness;
            rx.replenisher.inner = bound.replenisher;
            Ok(())
        })?;

        Ok(rx)
    }
}

/// Error when receiving from an `Rx`.
#[derive(Debug)]
pub enum RxError {
    Unbound,
    Reset,
    Deserialize(vox_postcard::error::DeserializeError),
    Protocol(String),
}

impl std::fmt::Display for RxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unbound => write!(f, "channel is not bound"),
            Self::Reset => write!(f, "channel reset by peer"),
            Self::Deserialize(e) => write!(f, "deserialize error: {e}"),
            Self::Protocol(msg) => write!(f, "protocol error: {msg}"),
        }
    }
}

impl std::error::Error for RxError {}

/// Check if a shape represents a `Tx` channel.
pub fn is_tx(shape: &facet_core::Shape) -> bool {
    shape.decl_id == Tx::<()>::SHAPE.decl_id
}

/// Check if a shape represents an `Rx` channel.
pub fn is_rx(shape: &facet_core::Shape) -> bool {
    shape.decl_id == Rx::<()>::SHAPE.decl_id
}

/// Check if a shape represents any channel type (`Tx` or `Rx`).
pub fn is_channel(shape: &facet_core::Shape) -> bool {
    is_tx(shape) || is_rx(shape)
}

pub trait ChannelBinder: crate::MaybeSend + crate::MaybeSync {
    /// Allocate a channel ID and create a sink for sending items.
    ///
    fn create_tx(&self) -> (ChannelId, Arc<dyn ChannelSink>);

    /// Allocate a channel ID, register it for routing, and return a receiver.
    fn create_rx(&self) -> (ChannelId, BoundChannelReceiver);

    /// Create a sink for a known channel ID (callee side).
    ///
    /// The channel ID comes from `Request.channels`.
    fn bind_tx(&self, channel_id: ChannelId) -> Arc<dyn ChannelSink>;

    /// Register an inbound channel by ID and return the receiver (callee side).
    ///
    /// The channel ID comes from `Request.channels`.
    fn register_rx(&self, channel_id: ChannelId) -> BoundChannelReceiver;

    /// Optional opaque handle that keeps the underlying session/connection alive
    /// for the lifetime of any bound channel handle.
    fn channel_liveness(&self) -> Option<ChannelLivenessHandle> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Backing, ChannelClose, ChannelItem, ChannelReset, Metadata, SelfRef};
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct CountingSink {
        send_calls: AtomicUsize,
        close_calls: AtomicUsize,
        close_on_drop_calls: AtomicUsize,
    }

    impl CountingSink {
        fn new() -> Self {
            Self {
                send_calls: AtomicUsize::new(0),
                close_calls: AtomicUsize::new(0),
                close_on_drop_calls: AtomicUsize::new(0),
            }
        }
    }

    impl ChannelSink for CountingSink {
        fn send_payload<'payload>(
            &self,
            _payload: Payload<'payload>,
        ) -> Pin<Box<dyn crate::MaybeSendFuture<Output = Result<(), TxError>> + 'payload>> {
            self.send_calls.fetch_add(1, Ordering::AcqRel);
            Box::pin(async { Ok(()) })
        }

        fn close_channel(
            &self,
            _metadata: Metadata,
        ) -> Pin<Box<dyn crate::MaybeSendFuture<Output = Result<(), TxError>> + 'static>> {
            self.close_calls.fetch_add(1, Ordering::AcqRel);
            Box::pin(async { Ok(()) })
        }

        fn close_channel_on_drop(&self) {
            self.close_on_drop_calls.fetch_add(1, Ordering::AcqRel);
        }
    }

    struct CountingReplenisher {
        calls: AtomicUsize,
    }

    impl CountingReplenisher {
        fn new() -> Self {
            Self {
                calls: AtomicUsize::new(0),
            }
        }
    }

    impl ChannelCreditReplenisher for CountingReplenisher {
        fn on_item_consumed(&self) {
            self.calls.fetch_add(1, Ordering::AcqRel);
        }
    }

    #[tokio::test]
    async fn tx_close_does_not_emit_drop_close_after_explicit_close() {
        let sink_impl = Arc::new(CountingSink::new());
        let sink: Arc<dyn ChannelSink> = sink_impl.clone();

        let mut tx = Tx::<u32>::unbound();
        tx.bind(sink);
        tx.close(Metadata::default())
            .await
            .expect("close should succeed");
        drop(tx);

        assert_eq!(sink_impl.close_calls.load(Ordering::Acquire), 1);
        assert_eq!(sink_impl.close_on_drop_calls.load(Ordering::Acquire), 0);
    }

    #[test]
    fn tx_drop_emits_close_on_drop_for_bound_sink() {
        let sink_impl = Arc::new(CountingSink::new());
        let sink: Arc<dyn ChannelSink> = sink_impl.clone();

        let mut tx = Tx::<u32>::unbound();
        tx.bind(sink);
        drop(tx);

        assert_eq!(sink_impl.close_on_drop_calls.load(Ordering::Acquire), 1);
    }

    #[test]
    fn tx_drop_emits_close_on_drop_for_paired_core_binding() {
        let sink_impl = Arc::new(CountingSink::new());
        let sink: Arc<dyn ChannelSink> = sink_impl.clone();

        let (tx, _rx) = channel::<u32>();
        let core = tx.core.inner.as_ref().expect("paired tx should have core");
        core.set_binding(ChannelBinding::Sink(BoundChannelSink {
            sink,
            liveness: None,
        }));
        drop(tx);

        assert_eq!(sink_impl.close_on_drop_calls.load(Ordering::Acquire), 1);
    }

    #[tokio::test]
    async fn rx_recv_returns_unbound_when_not_bound() {
        let mut rx = Rx::<u32>::unbound();
        let err = match rx.recv().await {
            Ok(_) => panic!("unbound rx should fail"),
            Err(err) => err,
        };
        assert!(matches!(err, RxError::Unbound));
    }

    #[tokio::test]
    async fn rx_recv_returns_none_on_close() {
        let (tx, rx_inner) = mpsc::channel("vox_types.channel.test.rx1", 1);
        let mut rx = Rx::<u32>::unbound();
        rx.bind(rx_inner);

        let close = SelfRef::owning(
            Backing::Boxed(Box::<[u8]>::default()),
            ChannelClose {
                metadata: Metadata::default(),
            },
        );
        tx.send(IncomingChannelMessage::Close(close))
            .await
            .expect("send close");

        assert!(rx.recv().await.expect("recv should succeed").is_none());
    }

    #[tokio::test]
    async fn rx_recv_returns_reset_error() {
        let (tx, rx_inner) = mpsc::channel("vox_types.channel.test.rx2", 1);
        let mut rx = Rx::<u32>::unbound();
        rx.bind(rx_inner);

        let reset = SelfRef::owning(
            Backing::Boxed(Box::<[u8]>::default()),
            ChannelReset {
                metadata: Metadata::default(),
            },
        );
        tx.send(IncomingChannelMessage::Reset(reset))
            .await
            .expect("send reset");

        let err = match rx.recv().await {
            Ok(_) => panic!("reset should be surfaced as error"),
            Err(err) => err,
        };
        assert!(matches!(err, RxError::Reset));
    }

    #[tokio::test]
    async fn rx_recv_rejects_outgoing_payload_variant_as_protocol_error() {
        static VALUE: u32 = 42;

        let (tx, rx_inner) = mpsc::channel("vox_types.channel.test.rx3", 1);
        let mut rx = Rx::<u32>::unbound();
        rx.bind(rx_inner);

        let item = SelfRef::owning(
            Backing::Boxed(Box::<[u8]>::default()),
            ChannelItem {
                item: Payload::outgoing(&VALUE),
            },
        );
        tx.send(IncomingChannelMessage::Item(item))
            .await
            .expect("send item");

        let err = match rx.recv().await {
            Ok(_) => panic!("outgoing payload should be protocol error"),
            Err(err) => err,
        };
        assert!(matches!(err, RxError::Protocol(_)));
    }

    #[tokio::test]
    async fn rx_recv_notifies_replenisher_after_consuming_an_item() {
        let (tx, rx_inner) = mpsc::channel("vox_types.channel.test.rx4", 1);
        let replenisher = Arc::new(CountingReplenisher::new());
        let mut rx = Rx::<u32>::unbound();
        rx.bind(rx_inner);
        rx.replenisher.inner = Some(replenisher.clone());

        let encoded = vox_postcard::to_vec(&123_u32).expect("serialize test item");
        let item = SelfRef::owning(
            Backing::Boxed(Box::<[u8]>::default()),
            ChannelItem {
                item: Payload::PostcardBytes(Box::leak(encoded.into_boxed_slice())),
            },
        );
        tx.send(IncomingChannelMessage::Item(item))
            .await
            .expect("send item");

        let value = rx
            .recv()
            .await
            .expect("recv should succeed")
            .expect("expected item");
        assert_eq!(*value, 123_u32);
        assert_eq!(replenisher.calls.load(Ordering::Acquire), 1);
    }

    #[tokio::test]
    async fn rx_recv_logical_receiver_decodes_items_and_notifies_replenisher() {
        let (tx, rx_inner) = mpsc::channel("vox_types.channel.test.rx5", 1);
        let replenisher = Arc::new(CountingReplenisher::new());
        let core = Arc::new(ChannelCore::new());
        core.bind_retryable_receiver(BoundChannelReceiver {
            receiver: rx_inner,
            liveness: None,
            replenisher: Some(replenisher.clone()),
        });

        let mut rx = Rx::<u32>::paired(core);

        let encoded = vox_postcard::to_vec(&321_u32).expect("serialize test item");
        let item = SelfRef::owning(
            Backing::Boxed(Box::<[u8]>::default()),
            ChannelItem {
                item: Payload::PostcardBytes(Box::leak(encoded.into_boxed_slice())),
            },
        );
        tx.send(IncomingChannelMessage::Item(item))
            .await
            .expect("send item");

        let value = rx
            .recv()
            .await
            .expect("recv should succeed")
            .expect("expected item");
        assert_eq!(*value, 321_u32);
        assert_eq!(replenisher.calls.load(Ordering::Acquire), 1);
    }

    // ========================================================================
    // Channel binding through ser/deser
    // ========================================================================

    /// A test binder that tracks allocations and bindings.
    struct TestBinder {
        next_id: std::sync::Mutex<u64>,
    }

    impl TestBinder {
        fn new() -> Self {
            Self {
                next_id: std::sync::Mutex::new(100),
            }
        }

        fn alloc_id(&self) -> ChannelId {
            let mut guard = self.next_id.lock().unwrap();
            let id = *guard;
            *guard += 2;
            ChannelId(id)
        }
    }

    impl ChannelBinder for TestBinder {
        fn create_tx(&self) -> (ChannelId, Arc<dyn ChannelSink>) {
            (self.alloc_id(), Arc::new(CountingSink::new()))
        }

        fn create_rx(&self) -> (ChannelId, BoundChannelReceiver) {
            let (tx, rx) = mpsc::channel("vox_types.channel.test.bind_retryable1", 8);
            // Keep the sender alive by leaking it — test only.
            std::mem::forget(tx);
            (
                self.alloc_id(),
                BoundChannelReceiver {
                    receiver: rx,
                    liveness: None,
                    replenisher: None,
                },
            )
        }

        fn bind_tx(&self, _channel_id: ChannelId) -> Arc<dyn ChannelSink> {
            Arc::new(CountingSink::new())
        }

        fn register_rx(&self, _channel_id: ChannelId) -> BoundChannelReceiver {
            let (_tx, rx) = mpsc::channel("vox_types.channel.test.bind_retryable2", 8);
            BoundChannelReceiver {
                receiver: rx,
                liveness: None,
                replenisher: None,
            }
        }
    }

    // Case 1: Caller passes Tx in args, keeps paired Rx.
    // Serializing the Tx allocates a channel ID via create_rx() and stores
    // the receiver in the shared logical core so the kept Rx can survive retries.
    #[tokio::test]
    async fn case1_serialize_tx_allocates_and_binds_paired_rx() {
        use facet::Facet;

        #[derive(Facet)]
        struct Args {
            data: u32,
            tx: Tx<u32>,
        }

        let (tx, rx) = channel::<u32>();
        let args = Args { data: 42, tx };

        let binder = TestBinder::new();
        let bytes =
            with_channel_binder(&binder, || vox_postcard::to_vec(&args).expect("serialize"));

        // The channel ID should be in the serialized bytes (after the u32 data field).
        assert!(!bytes.is_empty());

        // The kept Rx should now have a receiver binding in the shared core.
        assert!(
            rx.core.inner.is_some(),
            "paired Rx should have a shared core"
        );
        let core = rx.core.inner.as_ref().unwrap();
        assert!(
            core.take_logical_receiver().is_some(),
            "core should have a logical receiver binding from create_rx()"
        );
    }

    // Case 2: Caller passes Rx in args, keeps paired Tx.
    // Serializing the Rx allocates a channel ID via create_tx() and stores
    // the sink in the shared core so the kept Tx can use it.
    #[test]
    fn case2_serialize_rx_allocates_and_binds_paired_tx() {
        use facet::Facet;

        #[derive(Facet)]
        struct Args {
            data: u32,
            rx: Rx<u32>,
        }

        let (tx, rx) = channel::<u32>();
        let args = Args { data: 42, rx };

        let binder = TestBinder::new();
        let bytes =
            with_channel_binder(&binder, || vox_postcard::to_vec(&args).expect("serialize"));

        assert!(!bytes.is_empty());

        // The kept Tx should now have a sink binding in the shared core.
        assert!(tx.core.inner.is_some());
        let core = tx.core.inner.as_ref().unwrap();
        assert!(
            core.get_sink().is_some(),
            "core should have a Sink binding from create_tx()"
        );
    }

    // Case 3: Callee deserializes Tx from args.
    // The Tx is bound directly via bind_tx() during deserialization.
    #[test]
    fn case3_deserialize_tx_binds_via_binder() {
        use facet::Facet;

        #[derive(Facet)]
        struct Args {
            data: u32,
            tx: Tx<u32>,
        }

        // Simulate wire bytes: a u32 (42) followed by a channel ID (varint 7).
        let mut bytes = vox_postcard::to_vec(&42_u32).unwrap();
        bytes.extend_from_slice(&vox_postcard::to_vec(&ChannelId(7)).unwrap());

        let binder = TestBinder::new();
        let args: Args = with_channel_binder(&binder, || {
            vox_postcard::from_slice(&bytes).expect("deserialize")
        });

        assert_eq!(args.data, 42);
        assert_eq!(args.tx.channel_id, ChannelId(7));
        assert!(
            args.tx.is_bound(),
            "deserialized Tx should be bound via bind_tx()"
        );
    }

    // Case 4: Callee deserializes Rx from args.
    // The Rx is bound directly via register_rx() during deserialization.
    #[test]
    fn case4_deserialize_rx_binds_via_binder() {
        use facet::Facet;

        #[derive(Facet)]
        struct Args {
            data: u32,
            rx: Rx<u32>,
        }

        // Simulate wire bytes: a u32 (42) followed by a channel ID (varint 7).
        let mut bytes = vox_postcard::to_vec(&42_u32).unwrap();
        bytes.extend_from_slice(&vox_postcard::to_vec(&ChannelId(7)).unwrap());

        let binder = TestBinder::new();
        let args: Args = with_channel_binder(&binder, || {
            vox_postcard::from_slice(&bytes).expect("deserialize")
        });

        assert_eq!(args.data, 42);
        assert_eq!(args.rx.channel_id, ChannelId(7));
        assert!(
            args.rx.is_bound(),
            "deserialized Rx should be bound via register_rx()"
        );
    }

    // Round-trip: serialize with caller binder, deserialize with callee binder.
    // Verifies the channel ID allocated during serialization appears in the
    // deserialized handle.
    #[test]
    fn channel_id_round_trips_through_ser_deser() {
        use facet::Facet;

        #[derive(Facet)]
        struct Args {
            tx: Tx<u32>,
        }

        let (tx, _rx) = channel::<u32>();
        let args = Args { tx };

        let caller_binder = TestBinder::new();
        let bytes = with_channel_binder(&caller_binder, || {
            vox_postcard::to_vec(&args).expect("serialize")
        });

        let callee_binder = TestBinder::new();
        let deserialized: Args = with_channel_binder(&callee_binder, || {
            vox_postcard::from_slice(&bytes).expect("deserialize")
        });

        // The caller binder starts at ID 100, so the deserialized Tx should have that ID.
        assert_eq!(deserialized.tx.channel_id, ChannelId(100));
        assert!(deserialized.tx.is_bound());
    }
}
