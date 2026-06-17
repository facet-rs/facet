use std::collections::VecDeque;
use std::marker::PhantomData;
use std::panic::Location;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use facet::{Facet, FacetOpaqueAdapter, OpaqueDeserialize, OpaqueSerialize};
use facet_core::PtrConst;
#[cfg(not(target_arch = "wasm32"))]
use tokio::sync::TryAcquireError;
#[cfg(target_arch = "wasm32")]
use vox_rt::sync::TryAcquireError;
use vox_rt::sync::{Notify, Semaphore};

use crate::{
    Backing, BindingDirection, ChannelClose, ChannelItem, ChannelReset, Metadata, MethodDescriptor,
    Payload, SchemaRecvTracker, SelfRef,
};
use crate::{
    ChannelCloseReason, ChannelDebugContext, ChannelEvent, ChannelEventContext, ChannelResetReason,
    ChannelSendOutcome, ChannelTrySendOutcome, ConnectionCloseReason, SourceLocation,
    VoxObserverHandle,
};
use crate::{ChannelId, LaneId};

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
    let _guard = set_channel_binder(binder);
    f()
}

/// Set the thread-local channel binder, returning a guard that restores
/// the previous value on drop.
///
/// Prefer this over [`with_channel_binder`] when the code that runs under
/// the binder needs to return borrowed data (closures can't return borrows
/// from captures).
pub fn set_channel_binder(binder: &dyn ChannelBinder) -> ChannelBinderGuard<'_> {
    // SAFETY: we restore the previous value on drop (via ChannelBinderGuard),
    // so the binder reference doesn't escape the guard's lifetime.
    #[allow(unsafe_code)]
    let static_ref: &'static dyn ChannelBinder = unsafe { std::mem::transmute(binder) };
    let prev = CHANNEL_BINDER.with(|cell| cell.borrow_mut().replace(static_ref));
    ChannelBinderGuard {
        prev,
        _lifetime: std::marker::PhantomData,
    }
}

/// RAII guard that restores the previous thread-local channel binder on drop.
pub struct ChannelBinderGuard<'a> {
    prev: Option<&'static dyn ChannelBinder>,
    _lifetime: std::marker::PhantomData<&'a dyn ChannelBinder>,
}

impl Drop for ChannelBinderGuard<'_> {
    fn drop(&mut self) {
        CHANNEL_BINDER.with(|cell| {
            *cell.borrow_mut() = self.prev.take();
        });
    }
}

// ---------------------------------------------------------------------------
// Out-of-band channel-id table.
//
// `Tx<T>`/`Rx<T>` appear in request arguments, but on the wire they are NOT
// serialized inline — each encodes only a small `u32` index, and the allocated
// `ChannelId`s travel out-of-band in `RequestCall.channels`. This mirrors the
// `Fd` → fd-table indirection (`crate::fd`): the binder above still does the
// allocation/binding; these thread-locals carry the id list alongside the args
// payload so the index can be re-associated at the peer.
//
// r[impl rpc.request] r[impl rpc.channel.allocation]
// ---------------------------------------------------------------------------

/// `wire_index` sentinel: this handle was never pushed into a collector (no
/// collector installed at encode). Decoding such an index is a clean error,
/// never a panic across the `extern "C"` encoder trampolines.
const CHANNEL_NOT_COLLECTED: u32 = u32::MAX;

/// The channel ids gathered while encoding one request's arguments.
struct ChannelCollector {
    /// Allocated channel ids, in encode walk-order — becomes `RequestCall.channels`.
    ids: Vec<ChannelId>,
    /// Handle value address → assigned index, scoped to this collector, so the
    /// same handle encoded more than once claims one slot.
    seen: std::collections::HashMap<usize, u32>,
    roles: Vec<ChannelArgSchemaRole>,
}

#[derive(Clone)]
struct ChannelArgSchemaRole {
    method_id: crate::MethodId,
    direction: BindingDirection,
    role: String,
}

struct CollectedChannel {
    index: u32,
    role: Option<ChannelArgSchemaRole>,
}

struct ChannelSource {
    ids: Vec<ChannelId>,
    bindings: Vec<Result<ProvidedChannelSchemas, String>>,
}

#[derive(Clone, Default)]
struct ProvidedChannelSchemas {
    writer_schema: Option<Arc<vox_phon::SchemaBundle>>,
    writer_schema_send: Option<crate::ChannelWriterSchemaPlan>,
}

struct ProvidedChannel {
    id: ChannelId,
    writer_schema: Option<Arc<vox_phon::SchemaBundle>>,
    writer_schema_send: Option<crate::ChannelWriterSchemaPlan>,
}

std::thread_local! {
    static CHANNEL_COLLECTOR: std::cell::RefCell<Option<ChannelCollector>> =
        const { std::cell::RefCell::new(None) };
    static CHANNEL_SOURCE: std::cell::RefCell<Option<ChannelSource>> =
        const { std::cell::RefCell::new(None) };
}

/// Install an empty channel collector for the duration of `f`, returning what
/// `f` produced together with the channel ids it gathered (the out-of-band list
/// for `RequestCall.channels`). Wrap the args encode with this — together with a
/// [`ChannelBinder`] (the collector records ids the binder allocates).
// r[impl rpc.channel.discovery]
pub fn collect_channels<R>(f: impl FnOnce() -> R) -> (R, Vec<ChannelId>) {
    struct Restore(Option<ChannelCollector>);
    impl Drop for Restore {
        fn drop(&mut self) {
            CHANNEL_COLLECTOR.with(|c| *c.borrow_mut() = self.0.take());
        }
    }
    let fresh = ChannelCollector {
        ids: Vec::new(),
        seen: std::collections::HashMap::new(),
        roles: Vec::new(),
    };
    let _restore = Restore(CHANNEL_COLLECTOR.with(|c| c.borrow_mut().replace(fresh)));
    let out = f();
    let ids = CHANNEL_COLLECTOR
        .with(|c| {
            c.borrow_mut()
                .as_mut()
                .map(|col| std::mem::take(&mut col.ids))
        })
        .unwrap_or_default();
    (out, ids)
}

pub fn collect_channels_for_method<R>(
    method: &MethodDescriptor,
    f: impl FnOnce() -> R,
) -> (R, Vec<ChannelId>) {
    // r[impl schema.exchange.channels]
    // r[impl rpc.channel.discovery]
    struct Restore(Option<ChannelCollector>);
    impl Drop for Restore {
        fn drop(&mut self) {
            CHANNEL_COLLECTOR.with(|c| *c.borrow_mut() = self.0.take());
        }
    }

    let roles = method
        .args
        .iter()
        .enumerate()
        .filter_map(|(index, arg)| {
            let direction = if is_tx(arg.shape) {
                "tx"
            } else if is_rx(arg.shape) {
                "rx"
            } else {
                return None;
            };
            Some(ChannelArgSchemaRole {
                method_id: method.id,
                direction: BindingDirection::Args,
                role: format!("channel.arg.{index}.{direction}.element"),
            })
        })
        .collect();

    let fresh = ChannelCollector {
        ids: Vec::new(),
        seen: std::collections::HashMap::new(),
        roles,
    };
    let _restore = Restore(CHANNEL_COLLECTOR.with(|c| c.borrow_mut().replace(fresh)));
    let out = f();
    let ids = CHANNEL_COLLECTOR
        .with(|c| {
            c.borrow_mut()
                .as_mut()
                .map(|col| std::mem::take(&mut col.ids))
        })
        .unwrap_or_default();
    (out, ids)
}

/// Provide the channel ids received with a request (`RequestCall.channels`) for
/// the duration of `f` (typed args decoding). Each `Tx`/`Rx` decoded inside
/// claims one by index. Wrap the args decode with this — together with a
/// [`ChannelBinder`] (the index is looked up here, the binder binds it).
pub fn provide_channels<R>(channels: Vec<ChannelId>, f: impl FnOnce() -> R) -> R {
    struct Restore(Option<ChannelSource>);
    impl Drop for Restore {
        fn drop(&mut self) {
            CHANNEL_SOURCE.with(|c| *c.borrow_mut() = self.0.take());
        }
    }
    let source = ChannelSource {
        ids: channels,
        bindings: Vec::new(),
    };
    let _restore = Restore(CHANNEL_SOURCE.with(|c| c.borrow_mut().replace(source)));
    f()
}

pub fn provide_channels_for_method<R>(
    channels: Vec<ChannelId>,
    method: &MethodDescriptor,
    schemas: &SchemaRecvTracker,
    f: impl FnOnce() -> R,
) -> R {
    // r[impl schema.exchange.channels]
    struct Restore(Option<ChannelSource>);
    impl Drop for Restore {
        fn drop(&mut self) {
            CHANNEL_SOURCE.with(|c| *c.borrow_mut() = self.0.take());
        }
    }

    let tx_plan = if method.args.iter().any(|arg| is_tx(arg.shape)) {
        Some(crate::SchemaSendTracker::plan_for_method_args(method))
    } else {
        None
    };

    let bindings = method
        .args
        .iter()
        .enumerate()
        .filter_map(|(index, arg)| {
            if is_tx(arg.shape) {
                // r[impl schema.exchange.channels.tx-args]
                let role = format!("channel.arg.{index}.tx.element");
                return Some(match tx_plan.as_ref().expect("tx plan should be present") {
                    Ok(prepared) => Ok(ProvidedChannelSchemas {
                        writer_schema: None,
                        writer_schema_send: Some(crate::ChannelWriterSchemaPlan {
                            method_id: method.id,
                            direction: BindingDirection::Args,
                            role,
                            prepared: prepared.clone(),
                        }),
                    }),
                    Err(error) => Err(error.to_string()),
                });
            }
            if !is_rx(arg.shape) {
                return None;
            }
            // r[impl schema.exchange.channels.rx-args]
            let role = format!("channel.arg.{index}.rx.element");
            Some(
                schemas
                    .writer_auxiliary_schema_bundle(method.id, BindingDirection::Args, &role)
                    .map(|bundle| ProvidedChannelSchemas {
                        writer_schema: bundle.map(Arc::new),
                        writer_schema_send: None,
                    }),
            )
        })
        .collect();

    let source = ChannelSource {
        ids: channels,
        bindings,
    };
    let _restore = Restore(CHANNEL_SOURCE.with(|c| c.borrow_mut().replace(source)));
    f()
}

/// Record `channel_id` in the active collector, returning its stable index.
/// Idempotent per handle value address. Returns [`CHANNEL_NOT_COLLECTED`] when
/// no collector is installed (the error surfaces cleanly at decode).
// r[impl rpc.channel.discovery]
fn collect_channel(key: usize, channel_id: ChannelId) -> CollectedChannel {
    CHANNEL_COLLECTOR.with(|c| {
        let mut slot = c.borrow_mut();
        let Some(col) = slot.as_mut() else {
            return CollectedChannel {
                index: CHANNEL_NOT_COLLECTED,
                role: None,
            };
        };
        if let Some(&idx) = col.seen.get(&key) {
            return CollectedChannel {
                index: idx,
                role: col.roles.get(idx as usize).cloned(),
            };
        }
        let idx = col.ids.len() as u32;
        col.ids.push(channel_id);
        col.seen.insert(key, idx);
        CollectedChannel {
            index: idx,
            role: col.roles.get(idx as usize).cloned(),
        }
    })
}

/// Look up channel `index` from the active source (the request's channel list).
fn take_channel(index: u32) -> Result<ProvidedChannel, String> {
    if index == CHANNEL_NOT_COLLECTED {
        return Err(
            "channel handle was encoded without a channel id (no collector installed)".to_string(),
        );
    }
    CHANNEL_SOURCE.with(|c| {
        let slot = c.borrow();
        let vec = slot
            .as_ref()
            .ok_or_else(|| "channel decoded with no channel source installed".to_string())?;
        let id = vec.ids.get(index as usize).copied().ok_or_else(|| {
            format!(
                "channel wire index {index} out of range ({})",
                vec.ids.len()
            )
        })?;
        let schemas = match vec.bindings.get(index as usize) {
            Some(Ok(binding)) => binding.clone(),
            Some(Err(error)) => return Err(error.clone()),
            None => ProvidedChannelSchemas::default(),
        };
        Ok(ProvidedChannel {
            id,
            writer_schema: schemas.writer_schema,
            writer_schema_send: schemas.writer_schema_send,
        })
    })
}

// r[impl rpc.channel.pair]
// r[impl rpc.channel.pair.binding-propagation]
/// The binding stored in a channel core — either a sink or a receiver, never both.
pub enum ChannelBinding {
    Sink(BoundChannelSink),
    Receiver(BoundChannelReceiver),
}

pub trait ChannelCreditReplenisher: crate::MaybeSend + crate::MaybeSync + 'static {
    fn on_item_consumed(&self);

    fn on_receiver_dropped(&self) {}

    fn channel_id(&self) -> Option<ChannelId> {
        None
    }

    fn connection_id(&self) -> Option<LaneId> {
        None
    }

    fn debug_context(&self) -> Option<ChannelDebugContext> {
        None
    }

    fn observer(&self) -> Option<VoxObserverHandle> {
        None
    }
}

pub type ChannelCreditReplenisherHandle = Arc<dyn ChannelCreditReplenisher>;

#[derive(Clone)]
pub struct BoundChannelSink {
    pub sink: Arc<dyn ChannelSink>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChannelMailboxStats {
    pub len: usize,
    pub capacity: usize,
    pub receiver_closed: bool,
    pub sender_count: usize,
}

pub struct ChannelMailboxSendError<T> {
    item: T,
}

impl<T> std::fmt::Debug for ChannelMailboxSendError<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChannelMailboxSendError")
            .finish_non_exhaustive()
    }
}

impl<T> ChannelMailboxSendError<T> {
    pub fn into_inner(self) -> T {
        self.item
    }
}

struct ChannelMailboxState<T> {
    inner: Mutex<ChannelMailboxInner<T>>,
    not_empty: Notify,
    not_full: Notify,
}

struct ChannelMailboxInner<T> {
    queue: VecDeque<T>,
    capacity: usize,
    receiver_closed: bool,
    sender_count: usize,
}

// r[impl rpc.channel.delivery.reliable]
pub struct ChannelMailboxSender<T> {
    state: Arc<ChannelMailboxState<T>>,
}

pub struct ChannelMailboxReceiver<T> {
    state: Arc<ChannelMailboxState<T>>,
}

pub fn channel_mailbox<T>(
    name: &'static str,
    capacity: usize,
) -> (ChannelMailboxSender<T>, ChannelMailboxReceiver<T>) {
    assert!(capacity > 0, "channel mailbox capacity must be non-zero");
    let state = Arc::new(ChannelMailboxState {
        inner: Mutex::new(ChannelMailboxInner {
            queue: VecDeque::with_capacity(capacity),
            capacity,
            receiver_closed: false,
            sender_count: 1,
        }),
        not_empty: Notify::new(name),
        not_full: Notify::new(name),
    });
    (
        ChannelMailboxSender {
            state: Arc::clone(&state),
        },
        ChannelMailboxReceiver { state },
    )
}

impl<T> Clone for ChannelMailboxSender<T> {
    fn clone(&self) -> Self {
        let mut guard = self
            .state
            .inner
            .lock()
            .expect("channel mailbox mutex poisoned");
        guard.sender_count = guard.sender_count.saturating_add(1);
        drop(guard);
        Self {
            state: Arc::clone(&self.state),
        }
    }
}

impl<T> Drop for ChannelMailboxSender<T> {
    fn drop(&mut self) {
        let mut guard = self
            .state
            .inner
            .lock()
            .expect("channel mailbox mutex poisoned");
        guard.sender_count = guard.sender_count.saturating_sub(1);
        let closed = guard.sender_count == 0;
        drop(guard);
        if closed {
            self.state.not_empty.notify_waiters();
            self.state.not_full.notify_waiters();
        }
    }
}

impl<T> ChannelMailboxSender<T> {
    pub async fn send(&self, item: T) -> Result<(), ChannelMailboxSendError<T>> {
        let mut item = Some(item);
        loop {
            let notified = {
                let mut guard = self
                    .state
                    .inner
                    .lock()
                    .expect("channel mailbox mutex poisoned");
                if guard.receiver_closed {
                    return Err(ChannelMailboxSendError {
                        item: item.take().expect("mailbox item already sent"),
                    });
                }
                if guard.queue.len() < guard.capacity {
                    guard
                        .queue
                        .push_back(item.take().expect("mailbox item already sent"));
                    drop(guard);
                    self.state.not_empty.notify_waiters();
                    return Ok(());
                }
                self.state.not_full.notified()
            };
            notified.await;
        }
    }

    pub fn force_send(&self, item: T) -> Result<(), ChannelMailboxSendError<T>> {
        let mut guard = self
            .state
            .inner
            .lock()
            .expect("channel mailbox mutex poisoned");
        if guard.receiver_closed {
            return Err(ChannelMailboxSendError { item });
        }
        guard.queue.push_back(item);
        drop(guard);
        self.state.not_empty.notify_waiters();
        Ok(())
    }

    pub fn stats(&self) -> ChannelMailboxStats {
        self.state.stats()
    }
}

impl<T> Drop for ChannelMailboxReceiver<T> {
    fn drop(&mut self) {
        let mut guard = self
            .state
            .inner
            .lock()
            .expect("channel mailbox mutex poisoned");
        guard.receiver_closed = true;
        guard.queue.clear();
        drop(guard);
        self.state.not_full.notify_waiters();
        self.state.not_empty.notify_waiters();
    }
}

impl<T> ChannelMailboxReceiver<T> {
    pub async fn recv(&mut self) -> Option<T> {
        loop {
            let notified = {
                let mut guard = self
                    .state
                    .inner
                    .lock()
                    .expect("channel mailbox mutex poisoned");
                if let Some(item) = guard.queue.pop_front() {
                    drop(guard);
                    self.state.not_full.notify_waiters();
                    return Some(item);
                }
                if guard.sender_count == 0 {
                    return None;
                }
                self.state.not_empty.notified()
            };
            notified.await;
        }
    }

    pub fn stats(&self) -> ChannelMailboxStats {
        self.state.stats()
    }
}

impl<T> ChannelMailboxState<T> {
    fn stats(&self) -> ChannelMailboxStats {
        let guard = self.inner.lock().expect("channel mailbox mutex poisoned");
        ChannelMailboxStats {
            len: guard.queue.len(),
            capacity: guard.capacity,
            receiver_closed: guard.receiver_closed,
            sender_count: guard.sender_count,
        }
    }
}

pub struct BoundChannelReceiver {
    pub receiver: ChannelMailboxReceiver<IncomingChannelMessage>,
    pub replenisher: Option<ChannelCreditReplenisherHandle>,
    pub writer_schema: Option<Arc<vox_phon::SchemaBundle>>,
}

struct LogicalReceiverState {
    generation: u64,
    replenisher: Option<ChannelCreditReplenisherHandle>,
    writer_schema: Option<Arc<vox_phon::SchemaBundle>>,
    sender: Option<ChannelMailboxSender<LogicalIncomingChannelMessage>>,
    receiver: Option<ChannelMailboxReceiver<LogicalIncomingChannelMessage>>,
}

type TakenLogicalReceiver = (
    ChannelMailboxReceiver<LogicalIncomingChannelMessage>,
    Option<ChannelCreditReplenisherHandle>,
    Option<Arc<vox_phon::SchemaBundle>>,
);

// r[impl rpc.channel.pair]
// r[impl rpc.channel.pair.binding-propagation]
/// Shared state between a `Tx`/`Rx` pair created by `channel()`.
///
/// Contains a `Mutex<Option<ChannelBinding>>` that is written once during
/// binding and read/taken by the paired handle. The mutex is only locked
/// during binding (once) and on first use by the paired handle (once).
pub struct ChannelCore {
    binding: Mutex<Option<ChannelBinding>>,
    logical_receiver: Mutex<Option<LogicalReceiverState>>,
    binding_changed: Notify,
    debug_context: ChannelDebugContext,
}

impl ChannelCore {
    fn new(debug_context: ChannelDebugContext) -> Self {
        Self {
            binding: Mutex::new(None),
            logical_receiver: Mutex::new(None),
            binding_changed: Notify::new("vox_types.channel.binding_changed"),
            debug_context,
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

    pub fn bind_logical_receiver(self: &Arc<Self>, bound: BoundChannelReceiver) {
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
            let (tx, rx) = channel_mailbox("vox_types.channel.logical_receiver", 64);
            LogicalReceiverState {
                generation: 0,
                replenisher: None,
                writer_schema: None,
                sender: Some(tx),
                receiver: Some(rx),
            }
        });
        state.generation = state.generation.wrapping_add(1);
        state.replenisher = bound.replenisher.clone();
        state.writer_schema = bound.writer_schema.clone();
        let generation = state.generation;

        let Some(sender) = state.sender.clone() else {
            return;
        };

        self.binding_changed.notify_waiters();

        drop(guard);
        let core = Arc::clone(self);

        vox_rt::task::spawn(async move {
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

    pub fn take_logical_receiver(&self) -> Option<TakenLogicalReceiver> {
        self.logical_receiver
            .lock()
            .expect("channel core logical receiver mutex poisoned")
            .as_mut()
            .and_then(|state| {
                state.receiver.take().map(|receiver| {
                    (
                        receiver,
                        state.replenisher.clone(),
                        state.writer_schema.clone(),
                    )
                })
            })
    }

    pub fn finish_logical_receiver_binding(&self) {
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
                let _ = sender.force_send(LogicalIncomingChannelMessage {
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

    pub fn debug_context(&self) -> ChannelDebugContext {
        self.debug_context
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
// r[impl rpc.channel.pair.binding-propagation]
// r[impl rpc.observability.channel.context]
/// Create a channel pair with shared state — a `Tx<T>` (sender) and `Rx<T>`
/// (receiver) over one `ChannelCore`.
///
/// Channels **stream values within a call**: pass one end in a method's
/// **arguments**. That is the *only* place a channel may appear — the `#[service]`
/// macro rejects a `Tx`/`Rx` in return position ("channels are only allowed in
/// method arguments"). The framework binds the handle that appears in the args, and
/// the paired handle reads or takes the binding from the shared `ChannelCore`.
///
/// A channel is one-directional streaming, not a reply path: to hand a value *back*
/// from the handler, take a `Tx<T>` (the handler holds it and sends → caller). For
/// request/response over the same link, open a **virtual connection** on the session
/// — do not simulate it by pairing two channels.
#[track_caller]
pub fn channel<T>() -> (Tx<T>, Rx<T>) {
    let caller = Location::caller();
    let debug_context = ChannelDebugContext {
        type_name: Some(std::any::type_name::<T>()),
        source_location: Some(SourceLocation {
            file: caller.file(),
            line: caller.line(),
            column: caller.column(),
        }),
        ..ChannelDebugContext::default()
    };
    let core = Arc::new(ChannelCore::new(debug_context));
    (Tx::paired(core.clone()), Rx::paired(core))
}

fn merge_debug_context(
    primary: Option<ChannelDebugContext>,
    fallback: ChannelDebugContext,
) -> Option<ChannelDebugContext> {
    match (
        primary.and_then(ChannelDebugContext::into_option),
        fallback.into_option(),
    ) {
        (Some(primary), Some(fallback)) => ChannelDebugContext {
            label: primary.label.or(fallback.label),
            type_name: primary.type_name.or(fallback.type_name),
            source_location: primary.source_location.or(fallback.source_location),
            service: primary.service.or(fallback.service),
            method: primary.method.or(fallback.method),
        }
        .into_option(),
        (Some(primary), None) => Some(primary),
        (None, fallback) => fallback,
    }
}

fn sink_event_context(
    sink: &dyn ChannelSink,
    channel_id: ChannelId,
    fallback: ChannelDebugContext,
) -> ChannelEventContext {
    ChannelEventContext {
        connection_id: sink.connection_id(),
        channel_id,
        debug: merge_debug_context(sink.debug_context(), fallback),
    }
}

fn replenisher_event_context(
    replenisher: &dyn ChannelCreditReplenisher,
    channel_id: ChannelId,
    fallback: ChannelDebugContext,
) -> ChannelEventContext {
    ChannelEventContext {
        connection_id: replenisher.connection_id(),
        channel_id,
        debug: merge_debug_context(replenisher.debug_context(), fallback),
    }
}

fn observe_sink_channel(
    sink: &dyn ChannelSink,
    channel_id: Option<ChannelId>,
    fallback: ChannelDebugContext,
    event: impl FnOnce(ChannelEventContext) -> ChannelEvent,
) {
    if let (Some(observer), Some(channel_id)) = (sink.observer(), channel_id) {
        observer.channel_event(event(sink_event_context(sink, channel_id, fallback)));
    }
}

fn observe_replenisher_channel(
    replenisher: &dyn ChannelCreditReplenisher,
    fallback: ChannelDebugContext,
    event: impl FnOnce(ChannelEventContext) -> ChannelEvent,
) {
    if let (Some(observer), Some(channel_id)) = (replenisher.observer(), replenisher.channel_id()) {
        observer.channel_event(event(replenisher_event_context(
            replenisher,
            channel_id,
            fallback,
        )));
    }
}

fn observe_optional_replenisher_channel(
    replenisher: Option<&ChannelCreditReplenisherHandle>,
    fallback: ChannelDebugContext,
    event: impl FnOnce(ChannelEventContext) -> ChannelEvent,
) {
    if let Some(replenisher) = replenisher {
        observe_replenisher_channel(replenisher.as_ref(), fallback, event);
    }
}

/// Decode one channel item through phon.
// r[impl schema.interaction.channels]
// r[impl schema.exchange.channels]
// r[impl schema.exchange.channels.rx-args]
fn decode_channel_payload<T: Facet<'static>>(
    bytes: &'static [u8],
    decoder: &mut ChannelElementDecoderSlot,
) -> Result<T, RxError> {
    let Some(writer) = decoder.writer.as_ref() else {
        return vox_phon::from_slice_borrowed::<T>(bytes)
            .map_err(|e| RxError::Deserialize(e.to_string()));
    };
    if decoder.program.is_none() {
        decoder.program = Some(
            vox_phon::build_decode_program::<T>(writer)
                .map_err(|e| RxError::Deserialize(e.to_string()))?,
        );
    }
    vox_phon::decode_with_program::<T>(
        decoder
            .program
            .as_ref()
            .expect("channel decode program just built"),
        bytes,
    )
    .map_err(|e| RxError::Deserialize(e.to_string()))
}

fn decode_channel_item<T>(
    msg: SelfRef<ChannelItem<'static>>,
    decoder: &mut ChannelElementDecoderSlot,
) -> Result<Option<SelfRef<T>>, RxError>
where
    T: Facet<'static>,
{
    msg.try_repack(|item, _backing_bytes| {
        let Payload::Encoded(bytes) = item.item else {
            return Err(RxError::Protocol(
                "incoming channel item payload was not Incoming".into(),
            ));
        };
        decode_channel_payload(bytes, decoder)
    })
    .map(Some)
}

fn handle_incoming_channel_message<T>(
    msg: Option<IncomingChannelMessage>,
    replenisher: Option<&ChannelCreditReplenisherHandle>,
    debug_context: ChannelDebugContext,
    closed: &AtomicBool,
    decoder: &mut ChannelElementDecoderSlot,
) -> Result<Option<SelfRef<T>>, RxError>
where
    T: Facet<'static>,
{
    match msg {
        Some(IncomingChannelMessage::WriterSchema(writer_schema)) => {
            decoder.writer = Some(writer_schema);
            decoder.program = None;
            Err(RxError::Protocol(
                "channel writer schema reached payload handler".into(),
            ))
        }
        Some(IncomingChannelMessage::Close(_)) => {
            observe_optional_replenisher_channel(replenisher, debug_context, |channel| {
                ChannelEvent::Closed {
                    channel,
                    reason: ChannelCloseReason::Remote,
                }
            });
            closed.store(true, Ordering::Release);
            Ok(None)
        }
        Some(IncomingChannelMessage::ConnectionClosed(reason)) => {
            observe_optional_replenisher_channel(replenisher, debug_context, |channel| {
                ChannelEvent::Closed {
                    channel,
                    reason: ChannelCloseReason::ConnectionClosed,
                }
            });
            closed.store(true, Ordering::Release);
            Err(RxError::ConnectionClosed(reason))
        }
        None => {
            observe_optional_replenisher_channel(replenisher, debug_context, |channel| {
                ChannelEvent::Closed {
                    channel,
                    reason: ChannelCloseReason::Unknown,
                }
            });
            closed.store(true, Ordering::Release);
            Ok(None)
        }
        Some(IncomingChannelMessage::Reset(_)) => {
            observe_optional_replenisher_channel(replenisher, debug_context, |channel| {
                ChannelEvent::Reset {
                    channel,
                    reason: ChannelResetReason::Remote,
                }
            });
            closed.store(true, Ordering::Release);
            Err(RxError::Reset)
        }
        Some(IncomingChannelMessage::Item(msg)) => {
            let value = decode_channel_item(msg, decoder);
            if value.is_ok() {
                observe_optional_replenisher_channel(replenisher, debug_context, |channel| {
                    ChannelEvent::ItemConsumed { channel }
                });
                if let Some(replenisher) = replenisher {
                    replenisher.on_item_consumed();
                }
            }
            value
        }
    }
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

    fn channel_id(&self) -> Option<ChannelId> {
        None
    }

    fn connection_id(&self) -> Option<LaneId> {
        None
    }

    fn debug_context(&self) -> Option<ChannelDebugContext> {
        None
    }

    fn observer(&self) -> Option<VoxObserverHandle> {
        None
    }

    #[doc(hidden)]
    fn note_send_started(&self) {}

    #[doc(hidden)]
    fn note_send_waiting_for_credit(&self) {}

    #[doc(hidden)]
    fn note_send_finished(&self, _outcome: ChannelSendOutcome) {}

    #[doc(hidden)]
    fn note_try_send_outcome(&self, _outcome: ChannelTrySendOutcome) {}

    #[doc(hidden)]
    fn try_send_payload_with_outcome<'payload>(
        &self,
        payload: Payload<'payload>,
    ) -> Result<(), ChannelTrySendOutcome> {
        self.try_send_payload(payload).map_err(|err| match err {
            TrySendError::Full(()) => ChannelTrySendOutcome::FullRuntimeQueue,
            TrySendError::Closed(()) => ChannelTrySendOutcome::Closed,
        })
    }

    // r[impl rpc.flow-control.credit.try-send]
    fn try_send_payload<'payload>(
        &self,
        _payload: Payload<'payload>,
    ) -> Result<(), TrySendError<()>> {
        Err(TrySendError::Full(()))
    }

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
    /// Wrap `inner` with runtime-configured initial credit permits.
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
        let channel_id = self.channel_id();
        if credit.available_permits() == 0 {
            self.inner.note_send_waiting_for_credit();
            observe_sink_channel(
                self,
                channel_id,
                ChannelDebugContext::default(),
                |channel| ChannelEvent::SendWaitingForCredit { channel },
            );
        }
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

    fn channel_id(&self) -> Option<ChannelId> {
        self.inner.channel_id()
    }

    fn connection_id(&self) -> Option<LaneId> {
        self.inner.connection_id()
    }

    fn debug_context(&self) -> Option<ChannelDebugContext> {
        self.inner.debug_context()
    }

    fn observer(&self) -> Option<VoxObserverHandle> {
        self.inner.observer()
    }

    fn note_send_started(&self) {
        self.inner.note_send_started();
    }

    fn note_send_waiting_for_credit(&self) {
        self.inner.note_send_waiting_for_credit();
    }

    fn note_send_finished(&self, outcome: ChannelSendOutcome) {
        self.inner.note_send_finished(outcome);
    }

    fn note_try_send_outcome(&self, outcome: ChannelTrySendOutcome) {
        self.inner.note_try_send_outcome(outcome);
    }

    // r[impl rpc.observability.channel.try-send-detail]
    fn try_send_payload_with_outcome<'payload>(
        &self,
        payload: Payload<'payload>,
    ) -> Result<(), ChannelTrySendOutcome> {
        let permit = self.credit.try_acquire_owned().map_err(|err| match err {
            TryAcquireError::NoPermits => ChannelTrySendOutcome::FullCredit,
            TryAcquireError::Closed => ChannelTrySendOutcome::Closed,
        })?;

        match self.inner.try_send_payload_with_outcome(payload) {
            Ok(()) => {
                std::mem::forget(permit);
                Ok(())
            }
            Err(err) => Err(err),
        }
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
    WriterSchema(Arc<vox_phon::SchemaBundle>),
    Item(SelfRef<ChannelItem<'static>>),
    Close(SelfRef<ChannelClose>),
    Reset(SelfRef<ChannelReset>),
    // r[impl rpc.channel.connection-closure]
    ConnectionClosed(ConnectionCloseReason),
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

/// Receiver-side runtime slot.
#[derive(Facet)]
#[facet(opaque)]
pub(crate) struct ReceiverSlot {
    pub(crate) inner: Option<ChannelMailboxReceiver<IncomingChannelMessage>>,
}

impl ReceiverSlot {
    pub(crate) fn empty() -> Self {
        Self { inner: None }
    }
}

#[derive(Facet)]
#[facet(opaque)]
pub(crate) struct LogicalReceiverSlot {
    pub(crate) inner: Option<ChannelMailboxReceiver<LogicalIncomingChannelMessage>>,
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

#[derive(Facet)]
#[facet(opaque)]
pub(crate) struct ChannelElementDecoderSlot {
    pub(crate) writer: Option<Arc<vox_phon::SchemaBundle>>,
    pub(crate) program: Option<vox_phon::DecodeProgram>,
}

impl ChannelElementDecoderSlot {
    pub(crate) fn empty() -> Self {
        Self {
            writer: None,
            program: None,
        }
    }
}

/// Sender handle: "I send". The holder of a `Tx<T>` sends items of type `T`.
///
/// In method args, the handler holds it (handler sends → caller).
///
/// On the wire a `Tx` is a `u32` index into `Message::Request.channels`; the
/// `ChannelId` itself travels out-of-band in that list (see [`TxChannelAdapter`]).
// r[impl rpc.channel]
// r[impl rpc.channel.direction]
// r[impl rpc.channel.payload-encoding]
#[derive(Facet)]
#[facet(opaque = TxChannelAdapter<T>)]
pub struct Tx<T> {
    pub(crate) channel_id: ChannelId,
    pub(crate) sink: SinkSlot,
    pub(crate) core: CoreSlot,
    debug_context: ChannelDebugContext,
    closed: AtomicBool,
    /// Scratch the adapter points `OpaqueSerialize` at: the index assigned by the
    /// channel collector at encode (`CHANNEL_NOT_COLLECTED` until then).
    wire_index: AtomicU32,
    _marker: PhantomData<T>,
}

impl<T> Tx<T> {
    /// Create a standalone unbound Tx (used by deserialization).
    #[track_caller]
    pub fn unbound() -> Self {
        let caller = Location::caller();
        Self::unbound_with_context(ChannelDebugContext {
            type_name: Some(std::any::type_name::<T>()),
            source_location: Some(SourceLocation {
                file: caller.file(),
                line: caller.line(),
                column: caller.column(),
            }),
            ..ChannelDebugContext::default()
        })
    }

    fn unbound_with_context(debug_context: ChannelDebugContext) -> Self {
        Self {
            channel_id: ChannelId::RESERVED,
            sink: SinkSlot::empty(),
            core: CoreSlot::empty(),
            debug_context,
            closed: AtomicBool::new(false),
            wire_index: AtomicU32::new(CHANNEL_NOT_COLLECTED),
            _marker: PhantomData,
        }
    }

    /// Create a Tx that is part of a `channel()` pair.
    fn paired(core: Arc<ChannelCore>) -> Self {
        let debug_context = core.debug_context();
        Self {
            channel_id: ChannelId::RESERVED,
            sink: SinkSlot::empty(),
            core: CoreSlot { inner: Some(core) },
            debug_context,
            closed: AtomicBool::new(false),
            wire_index: AtomicU32::new(CHANNEL_NOT_COLLECTED),
            _marker: PhantomData,
        }
    }

    pub fn debug_context(&self) -> ChannelDebugContext {
        self.debug_context
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
    // r[impl rpc.channel.pair.binding-propagation]
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

    fn channel_id_for_sink(&self, sink: &dyn ChannelSink) -> Option<ChannelId> {
        if self.channel_id == ChannelId::RESERVED {
            sink.channel_id()
        } else {
            Some(self.channel_id)
        }
    }

    fn observe_sink_event(
        &self,
        sink: &dyn ChannelSink,
        channel_id: Option<ChannelId>,
        event: impl FnOnce(ChannelEventContext) -> ChannelEvent,
    ) {
        observe_sink_channel(sink, channel_id, self.debug_context, event);
    }

    fn observe_try_send(
        &self,
        sink: &dyn ChannelSink,
        channel_id: Option<ChannelId>,
        outcome: ChannelTrySendOutcome,
    ) {
        self.observe_sink_event(sink, channel_id, |channel| ChannelEvent::TrySend {
            channel,
            outcome,
        });
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
        let channel_id = self.channel_id_for_sink(sink.as_ref());
        sink.note_send_started();
        self.observe_sink_event(sink.as_ref(), channel_id, |channel| {
            ChannelEvent::SendStarted { channel }
        });
        let started_at = crate::time::Instant::now();
        let ptr = PtrConst::new((&value as *const T).cast::<u8>());
        // SAFETY: `value` is explicitly dropped only after `await`, so the pointer
        // remains valid for the whole send operation.
        let payload = unsafe { Payload::outgoing_unchecked(ptr, T::SHAPE) };
        let result = sink.send_payload(payload).await;
        let outcome = match &result {
            Ok(()) => ChannelSendOutcome::Sent,
            Err(TxError::Transport(message)) if message == "channel closed" => {
                ChannelSendOutcome::Closed
            }
            Err(_) => ChannelSendOutcome::TransportError,
        };
        self.observe_sink_event(sink.as_ref(), channel_id, |channel| {
            ChannelEvent::SendFinished {
                channel,
                outcome,
                elapsed: started_at.elapsed(),
            }
        });
        sink.note_send_finished(outcome);
        drop(value);
        result
    }

    // r[impl rpc.flow-control.credit.try-send]
    pub fn try_send<'value>(&self, value: T) -> Result<(), TrySendError<T>>
    where
        T: Facet<'value>,
    {
        if self.closed.load(Ordering::Acquire) {
            return Err(TrySendError::Closed(value));
        }

        let Some(sink) = self.resolve_sink_now() else {
            return Err(TrySendError::Full(value));
        };
        let channel_id = self.channel_id_for_sink(sink.as_ref());

        let ptr = PtrConst::new((&value as *const T).cast::<u8>());
        // SAFETY: `try_send_payload` must complete synchronously before this
        // function returns, so `value` stays alive for the full borrow.
        let payload = unsafe { Payload::outgoing_unchecked(ptr, T::SHAPE) };
        match sink.try_send_payload_with_outcome(payload) {
            Ok(()) => {
                sink.note_try_send_outcome(ChannelTrySendOutcome::Sent);
                self.observe_try_send(sink.as_ref(), channel_id, ChannelTrySendOutcome::Sent);
                drop(value);
                Ok(())
            }
            Err(ChannelTrySendOutcome::Closed) => {
                sink.note_try_send_outcome(ChannelTrySendOutcome::Closed);
                self.observe_try_send(sink.as_ref(), channel_id, ChannelTrySendOutcome::Closed);
                self.closed.store(true, Ordering::Release);
                Err(TrySendError::Closed(value))
            }
            Err(outcome) => {
                sink.note_try_send_outcome(outcome);
                self.observe_try_send(sink.as_ref(), channel_id, outcome);
                Err(TrySendError::Full(value))
            }
        }
    }

    // r[impl rpc.channel.lifecycle]
    pub async fn close(&self, metadata: Metadata) -> Result<(), TxError> {
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
        self.sink.inner = Some(sink);
    }

    #[doc(hidden)]
    pub fn finish_call_binding(&self) {
        if let Some(core) = &self.core.inner {
            core.finish_logical_receiver_binding();
        }
    }
}

impl<T> Drop for Tx<T> {
    // r[impl rpc.channel.lifecycle]
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

    // r[impl rpc.channel.binding.caller-args]
    // r[impl rpc.channel.binding.caller-args.tx]
    // r[impl rpc.channel.pair.binding-propagation]
    fn try_from(value: &Tx<T>) -> Result<Self, Self::Error> {
        // Case 1: Caller passes Tx in args (callee sends, caller receives).
        // Allocate a channel ID and store the receiver binding in the shared
        // core so the caller's paired Rx can pick it up.
        CHANNEL_BINDER.with(|cell| {
            let borrow = cell.borrow();
            let Some(binder) = *borrow else {
                return Err("serializing Tx requires an active ChannelBinder".to_string());
            };
            let (channel_id, bound) = binder.create_rx_with_context(Some(value.debug_context));
            if let Some(core) = &value.core.inner {
                core.bind_logical_receiver(bound);
            }
            Ok(channel_id)
        })
    }
}

impl<T> TryFrom<ChannelId> for Tx<T> {
    type Error = String;

    // r[impl rpc.channel.binding.callee-args]
    // r[impl rpc.channel.binding.callee-args.tx]
    fn try_from(channel_id: ChannelId) -> Result<Self, Self::Error> {
        Self::from_channel_id_with_writer_schema(channel_id, None)
    }
}

impl<T> Tx<T> {
    fn from_channel_id_with_writer_schema(
        channel_id: ChannelId,
        writer_schema_send: Option<crate::ChannelWriterSchemaPlan>,
    ) -> Result<Self, String> {
        let debug_context = ChannelDebugContext {
            type_name: Some(std::any::type_name::<T>()),
            ..ChannelDebugContext::default()
        };
        let mut tx = Self::unbound_with_context(debug_context);
        tx.channel_id = channel_id;

        CHANNEL_BINDER.with(|cell| {
            let Some(binder) = *cell.borrow() else {
                return Err("deserializing Tx requires an active ChannelBinder".to_string());
            };
            let sink = binder.bind_tx_with_context_and_writer_schema(
                channel_id,
                Some(debug_context),
                writer_schema_send,
            );
            tx.bind(sink);
            Ok(())
        })?;

        Ok(tx)
    }
}

/// Opaque adapter bridging `Tx<T>` through the out-of-band channel table.
///
/// On the wire a `Tx` is a `u32` index into `RequestCall.channels`. Encode:
/// allocate + register the channel via the active [`ChannelBinder`] (the same
/// `TryFrom<&Tx>` logic the old proxy used), record the id in the active
/// collector, and point the wire at the assigned index. Decode: read the index,
/// resolve the id from the active source (`provide_channels`), and bind. Mirrors
/// [`FdAdapter`](crate::fd). `serialize_map` is infallible (a missing binder
/// yields the `CHANNEL_NOT_COLLECTED` sentinel; the error surfaces at decode).
// r[impl rpc.channel.payload-encoding] r[impl rpc.channel.binding]
pub struct TxChannelAdapter<T>(PhantomData<T>);

impl<T> FacetOpaqueAdapter for TxChannelAdapter<T> {
    type Error = String;
    type SendValue<'a> = Tx<T>;
    type RecvValue<'de> = Tx<T>;

    fn serialize_map(value: &Self::SendValue<'_>) -> OpaqueSerialize {
        let idx = match ChannelId::try_from(value) {
            Ok(channel_id) => {
                let collected = collect_channel(value as *const Tx<T> as usize, channel_id);
                if let Some(role) = collected.role {
                    CHANNEL_BINDER.with(|cell| {
                        if let Some(binder) = *cell.borrow() {
                            binder.note_channel_schema_role(
                                channel_id,
                                role.method_id,
                                role.direction,
                                &role.role,
                            );
                        }
                    });
                }
                collected.index
            }
            Err(_) => CHANNEL_NOT_COLLECTED,
        };
        value.wire_index.store(idx, Ordering::Relaxed);
        OpaqueSerialize {
            ptr: PtrConst::new(value.wire_index.as_ptr().cast::<u8>()),
            shape: <u32 as Facet>::SHAPE,
        }
    }

    fn deserialize_build<'de>(
        input: OpaqueDeserialize<'de>,
    ) -> Result<Self::RecvValue<'de>, Self::Error> {
        let bytes = match &input {
            OpaqueDeserialize::Borrowed(b) => *b,
            OpaqueDeserialize::Owned(b) => b.as_slice(),
        };
        let index =
            vox_phon::from_slice::<u32>(bytes).map_err(|e| format!("Tx channel index: {e}"))?;
        let channel = take_channel(index)?;
        Tx::from_channel_id_with_writer_schema(channel.id, channel.writer_schema_send)
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

/// Error returned by [`Tx::try_send`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrySendError<T> {
    /// Sending would block because channel credit or runtime queue capacity is exhausted.
    Full(T),
    /// The channel or underlying connection is closed.
    Closed(T),
}

impl<T> TrySendError<T> {
    pub fn into_inner(self) -> T {
        match self {
            Self::Full(value) | Self::Closed(value) => value,
        }
    }
}

impl<T> std::fmt::Display for TrySendError<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Full(_) => write!(f, "channel is full"),
            Self::Closed(_) => write!(f, "channel is closed"),
        }
    }
}

impl<T: std::fmt::Debug> std::error::Error for TrySendError<T> {}

/// Receiver handle: "I receive". The holder of an `Rx<T>` receives items of type `T`.
///
/// In method args, the handler holds it (handler receives ← caller).
///
/// On the wire an `Rx` is a `u32` index into `Message::Request.channels`; the
/// `ChannelId` itself travels out-of-band in that list (see [`RxChannelAdapter`]).
#[derive(Facet)]
#[facet(opaque = RxChannelAdapter<T>)]
pub struct Rx<T> {
    pub(crate) channel_id: ChannelId,
    pub(crate) receiver: ReceiverSlot,
    pub(crate) logical_receiver: LogicalReceiverSlot,
    pub(crate) core: CoreSlot,
    pub(crate) replenisher: ReplenisherSlot,
    pub(crate) decoder: ChannelElementDecoderSlot,
    debug_context: ChannelDebugContext,
    closed: AtomicBool,
    /// Scratch the adapter points `OpaqueSerialize` at: the index assigned by the
    /// channel collector at encode (`CHANNEL_NOT_COLLECTED` until then).
    wire_index: AtomicU32,
    _marker: PhantomData<T>,
}

impl<T> Rx<T> {
    /// Create a standalone unbound Rx (used by deserialization).
    #[track_caller]
    pub fn unbound() -> Self {
        let caller = Location::caller();
        Self::unbound_with_context(ChannelDebugContext {
            type_name: Some(std::any::type_name::<T>()),
            source_location: Some(SourceLocation {
                file: caller.file(),
                line: caller.line(),
                column: caller.column(),
            }),
            ..ChannelDebugContext::default()
        })
    }

    fn unbound_with_context(debug_context: ChannelDebugContext) -> Self {
        Self {
            channel_id: ChannelId::RESERVED,
            receiver: ReceiverSlot::empty(),
            logical_receiver: LogicalReceiverSlot::empty(),
            core: CoreSlot::empty(),
            replenisher: ReplenisherSlot::empty(),
            decoder: ChannelElementDecoderSlot::empty(),
            debug_context,
            closed: AtomicBool::new(false),
            wire_index: AtomicU32::new(CHANNEL_NOT_COLLECTED),
            _marker: PhantomData,
        }
    }

    /// Create an Rx that is part of a `channel()` pair.
    fn paired(core: Arc<ChannelCore>) -> Self {
        let debug_context = core.debug_context();
        Self {
            channel_id: ChannelId::RESERVED,
            receiver: ReceiverSlot::empty(),
            logical_receiver: LogicalReceiverSlot::empty(),
            core: CoreSlot { inner: Some(core) },
            replenisher: ReplenisherSlot::empty(),
            decoder: ChannelElementDecoderSlot::empty(),
            debug_context,
            closed: AtomicBool::new(false),
            wire_index: AtomicU32::new(CHANNEL_NOT_COLLECTED),
            _marker: PhantomData,
        }
    }

    pub fn debug_context(&self) -> ChannelDebugContext {
        self.debug_context
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
                && let Some((receiver, replenisher, writer_schema)) = core.take_logical_receiver()
            {
                self.logical_receiver.inner = Some(receiver);
                self.replenisher.inner = replenisher;
                self.decoder.writer = writer_schema;
                self.decoder.program = None;
            }

            if let Some(receiver) = self.logical_receiver.inner.as_mut() {
                let received = receiver.recv().await;
                if let Some(LogicalIncomingChannelMessage {
                    msg: IncomingChannelMessage::WriterSchema(writer_schema),
                    ..
                }) = received
                {
                    self.decoder.writer = Some(writer_schema);
                    self.decoder.program = None;
                    continue;
                }
                return match received {
                    Some(LogicalIncomingChannelMessage { msg, replenisher }) => {
                        handle_incoming_channel_message(
                            Some(msg),
                            replenisher.as_ref(),
                            self.debug_context,
                            &self.closed,
                            &mut self.decoder,
                        )
                    }
                    None => handle_incoming_channel_message(
                        None,
                        None,
                        self.debug_context,
                        &self.closed,
                        &mut self.decoder,
                    ),
                };
            }

            if self.receiver.inner.is_none()
                && let Some(core) = &self.core.inner
                && let Some(bound) = core.take_receiver()
            {
                self.receiver.inner = Some(bound.receiver);
                self.replenisher.inner = bound.replenisher;
                self.decoder.writer = bound.writer_schema;
                self.decoder.program = None;
            }

            if let Some(receiver) = self.receiver.inner.as_mut() {
                let received = receiver.recv().await;
                if let Some(IncomingChannelMessage::WriterSchema(writer_schema)) = received {
                    self.decoder.writer = Some(writer_schema);
                    self.decoder.program = None;
                    continue;
                }
                return handle_incoming_channel_message(
                    received,
                    self.replenisher.inner.as_ref(),
                    self.debug_context,
                    &self.closed,
                    &mut self.decoder,
                );
            }

            let Some(core) = &self.core.inner else {
                return Err(RxError::Unbound);
            };
            core.binding_changed.notified().await;
        }
    }
    #[doc(hidden)]
    pub fn bind(&mut self, receiver: ChannelMailboxReceiver<IncomingChannelMessage>) {
        self.receiver.inner = Some(receiver);
        self.logical_receiver.inner = None;
        self.replenisher.inner = None;
        self.closed.store(false, Ordering::Release);
    }
}

impl<T> Drop for Rx<T> {
    // r[impl rpc.channel.lifecycle]
    fn drop(&mut self) {
        if self.closed.swap(true, Ordering::AcqRel) {
            return;
        }

        if self.replenisher.inner.is_none()
            && let Some(core) = &self.core.inner
        {
            if let Some((_receiver, replenisher, _writer_schema)) = core.take_logical_receiver() {
                self.replenisher.inner = replenisher;
            } else if let Some(bound) = core.take_receiver() {
                self.replenisher.inner = bound.replenisher;
            }
        }

        if let Some(replenisher) = &self.replenisher.inner {
            observe_replenisher_channel(replenisher.as_ref(), self.debug_context, |channel| {
                ChannelEvent::Reset {
                    channel,
                    reason: ChannelResetReason::ReceiverDropped,
                }
            });
            replenisher.on_receiver_dropped();
        }
    }
}

impl<T> TryFrom<&Rx<T>> for ChannelId {
    type Error = String;

    // r[impl rpc.channel.binding.caller-args]
    // r[impl rpc.channel.binding.caller-args.rx]
    // r[impl rpc.channel.pair.binding-propagation]
    fn try_from(value: &Rx<T>) -> Result<Self, Self::Error> {
        // Case 2: Caller passes Rx in args (callee receives, caller sends).
        // Allocate a channel ID and store the sink binding in the shared
        // core so the caller's paired Tx can pick it up.
        CHANNEL_BINDER.with(|cell| {
            let borrow = cell.borrow();
            let Some(binder) = *borrow else {
                return Err("serializing Rx requires an active ChannelBinder".to_string());
            };
            let (channel_id, sink) = binder.create_tx_with_context(Some(value.debug_context));
            if let Some(core) = &value.core.inner {
                core.set_binding(ChannelBinding::Sink(BoundChannelSink { sink }));
            }
            Ok(channel_id)
        })
    }
}

impl<T> TryFrom<ChannelId> for Rx<T> {
    type Error = String;

    // r[impl rpc.channel.binding.callee-args]
    // r[impl rpc.channel.binding.callee-args.rx]
    fn try_from(channel_id: ChannelId) -> Result<Self, Self::Error> {
        Self::from_channel_id_with_writer(channel_id, None)
    }
}

impl<T> Rx<T> {
    fn from_channel_id_with_writer(
        channel_id: ChannelId,
        writer_schema: Option<Arc<vox_phon::SchemaBundle>>,
    ) -> Result<Self, String> {
        let debug_context = ChannelDebugContext {
            type_name: Some(std::any::type_name::<T>()),
            ..ChannelDebugContext::default()
        };
        let mut rx = Self::unbound_with_context(debug_context);
        rx.channel_id = channel_id;

        CHANNEL_BINDER.with(|cell| {
            let Some(binder) = *cell.borrow() else {
                return Err("deserializing Rx requires an active ChannelBinder".to_string());
            };
            let bound = binder.register_rx_with_context(channel_id, Some(debug_context));
            rx.receiver.inner = Some(bound.receiver);
            rx.replenisher.inner = bound.replenisher;
            rx.decoder.writer = writer_schema.or(bound.writer_schema);
            rx.decoder.program = None;
            Ok(())
        })?;

        Ok(rx)
    }
}

/// Opaque adapter bridging `Rx<T>` through the out-of-band channel table — the
/// receiver-side mirror of [`TxChannelAdapter`]. On the wire a `Rx` is a `u32`
/// index into `RequestCall.channels`.
// r[impl rpc.channel.payload-encoding] r[impl rpc.channel.binding]
pub struct RxChannelAdapter<T>(PhantomData<T>);

impl<T> FacetOpaqueAdapter for RxChannelAdapter<T> {
    type Error = String;
    type SendValue<'a> = Rx<T>;
    type RecvValue<'de> = Rx<T>;

    fn serialize_map(value: &Self::SendValue<'_>) -> OpaqueSerialize {
        let idx = match ChannelId::try_from(value) {
            Ok(channel_id) => {
                let collected = collect_channel(value as *const Rx<T> as usize, channel_id);
                if let Some(role) = collected.role {
                    CHANNEL_BINDER.with(|cell| {
                        if let Some(binder) = *cell.borrow() {
                            binder.note_channel_schema_role(
                                channel_id,
                                role.method_id,
                                role.direction,
                                &role.role,
                            );
                        }
                    });
                }
                collected.index
            }
            Err(_) => CHANNEL_NOT_COLLECTED,
        };
        value.wire_index.store(idx, Ordering::Relaxed);
        OpaqueSerialize {
            ptr: PtrConst::new(value.wire_index.as_ptr().cast::<u8>()),
            shape: <u32 as Facet>::SHAPE,
        }
    }

    fn deserialize_build<'de>(
        input: OpaqueDeserialize<'de>,
    ) -> Result<Self::RecvValue<'de>, Self::Error> {
        let bytes = match &input {
            OpaqueDeserialize::Borrowed(b) => *b,
            OpaqueDeserialize::Owned(b) => b.as_slice(),
        };
        let index =
            vox_phon::from_slice::<u32>(bytes).map_err(|e| format!("Rx channel index: {e}"))?;
        let channel = take_channel(index)?;
        Rx::from_channel_id_with_writer(channel.id, channel.writer_schema)
    }
}

/// Error when receiving from an `Rx`.
#[derive(Debug)]
pub enum RxError {
    Unbound,
    Reset,
    ConnectionClosed(ConnectionCloseReason),
    Deserialize(String),
    Protocol(String),
}

impl std::fmt::Display for RxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unbound => write!(f, "channel is not bound"),
            Self::Reset => write!(f, "channel reset by peer"),
            Self::ConnectionClosed(reason) => {
                write!(f, "connection closed while receiving channel: {reason:?}")
            }
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

    fn create_tx_with_context(
        &self,
        debug_context: Option<ChannelDebugContext>,
    ) -> (ChannelId, Arc<dyn ChannelSink>) {
        let _ = debug_context;
        self.create_tx()
    }

    /// Allocate a channel ID, register it for routing, and return a receiver.
    fn create_rx(&self) -> (ChannelId, BoundChannelReceiver);

    fn create_rx_with_context(
        &self,
        debug_context: Option<ChannelDebugContext>,
    ) -> (ChannelId, BoundChannelReceiver) {
        let _ = debug_context;
        self.create_rx()
    }

    /// Create a sink for a known channel ID (callee side).
    ///
    /// The channel ID comes from `Request.channels`.
    fn bind_tx(&self, channel_id: ChannelId) -> Arc<dyn ChannelSink>;

    fn bind_tx_with_context(
        &self,
        channel_id: ChannelId,
        debug_context: Option<ChannelDebugContext>,
    ) -> Arc<dyn ChannelSink> {
        let _ = debug_context;
        self.bind_tx(channel_id)
    }

    fn bind_tx_with_context_and_writer_schema(
        &self,
        channel_id: ChannelId,
        debug_context: Option<ChannelDebugContext>,
        writer_schema: Option<crate::ChannelWriterSchemaPlan>,
    ) -> Arc<dyn ChannelSink> {
        let _ = writer_schema;
        self.bind_tx_with_context(channel_id, debug_context)
    }

    /// Register an inbound channel by ID and return the receiver (callee side).
    ///
    /// The channel ID comes from `Request.channels`.
    fn register_rx(&self, channel_id: ChannelId) -> BoundChannelReceiver;

    fn register_rx_with_context(
        &self,
        channel_id: ChannelId,
        debug_context: Option<ChannelDebugContext>,
    ) -> BoundChannelReceiver {
        let _ = debug_context;
        self.register_rx(channel_id)
    }

    fn note_channel_schema_role(
        &self,
        channel_id: ChannelId,
        method_id: crate::MethodId,
        direction: BindingDirection,
        role: &str,
    ) {
        let _ = (channel_id, method_id, direction, role);
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
        dropped: AtomicUsize,
    }

    impl CountingReplenisher {
        fn new() -> Self {
            Self {
                calls: AtomicUsize::new(0),
                dropped: AtomicUsize::new(0),
            }
        }
    }

    impl ChannelCreditReplenisher for CountingReplenisher {
        fn on_item_consumed(&self) {
            self.calls.fetch_add(1, Ordering::AcqRel);
        }

        fn on_receiver_dropped(&self) {
            self.dropped.fetch_add(1, Ordering::AcqRel);
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
        core.set_binding(ChannelBinding::Sink(BoundChannelSink { sink }));
        drop(tx);

        assert_eq!(sink_impl.close_on_drop_calls.load(Ordering::Acquire), 1);
    }

    // r[verify rpc.channel.pair]
    // r[verify rpc.observability.channel.context]
    #[test]
    fn channel_pair_captures_source_location_and_type_context() {
        let expected_line = line!() + 1;
        let (tx, rx) = channel::<u32>();

        let tx_core = tx.core.inner.as_ref().expect("paired Tx should have core");
        let rx_core = rx.core.inner.as_ref().expect("paired Rx should have core");
        assert!(Arc::ptr_eq(tx_core, rx_core));
        assert!(!tx.is_bound());
        assert!(!rx.is_bound());

        for context in [tx.debug_context(), rx.debug_context()] {
            assert_eq!(context.type_name, Some(std::any::type_name::<u32>()));
            let location = context
                .source_location
                .expect("channel should capture source location");
            assert_eq!(location.file, file!());
            assert_eq!(location.line, expected_line);
        }
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
        let (tx, rx_inner) = channel_mailbox("vox_types.channel.test.rx1", 1);
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
        let (tx, rx_inner) = channel_mailbox("vox_types.channel.test.rx2", 1);
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

    // r[verify rpc.channel.connection-closure]
    #[tokio::test]
    async fn rx_recv_surfaces_connection_closed() {
        let (tx, rx_inner) = channel_mailbox("vox_types.channel.test.rx_connection_closed", 1);
        let mut rx = Rx::<u32>::unbound();
        rx.bind(rx_inner);

        tx.send(IncomingChannelMessage::ConnectionClosed(
            ConnectionCloseReason::Protocol,
        ))
        .await
        .expect("send connection close");

        let err = match rx.recv().await {
            Ok(_) => panic!("connection closure should be surfaced as error"),
            Err(err) => err,
        };
        assert!(matches!(
            err,
            RxError::ConnectionClosed(ConnectionCloseReason::Protocol)
        ));
    }

    #[tokio::test]
    async fn rx_recv_rejects_outgoing_payload_variant_as_protocol_error() {
        static VALUE: u32 = 42;

        let (tx, rx_inner) = channel_mailbox("vox_types.channel.test.rx3", 1);
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
        let (tx, rx_inner) = channel_mailbox("vox_types.channel.test.rx4", 1);
        let replenisher = Arc::new(CountingReplenisher::new());
        let mut rx = Rx::<u32>::unbound();
        rx.bind(rx_inner);
        rx.replenisher.inner = Some(replenisher.clone());

        let encoded = vox_phon::to_vec(&123_u32).expect("serialize test item");
        let item = SelfRef::owning(
            Backing::Boxed(Box::<[u8]>::default()),
            ChannelItem {
                item: Payload::Encoded(Box::leak(encoded.into_boxed_slice())),
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
        assert_eq!(*value.get(), 123_u32);
        assert_eq!(replenisher.calls.load(Ordering::Acquire), 1);
    }

    #[test]
    fn rx_drop_notifies_replenisher() {
        let (_tx, rx_inner) = channel_mailbox("vox_types.channel.test.rx_drop", 1);
        let replenisher = Arc::new(CountingReplenisher::new());
        let mut rx = Rx::<u32>::unbound();
        rx.bind(rx_inner);
        rx.replenisher.inner = Some(replenisher.clone());

        drop(rx);

        assert_eq!(replenisher.dropped.load(Ordering::Acquire), 1);
    }

    #[tokio::test]
    async fn rx_drop_after_close_does_not_notify_replenisher() {
        let (tx, rx_inner) = channel_mailbox("vox_types.channel.test.rx_drop_closed", 1);
        let replenisher = Arc::new(CountingReplenisher::new());
        let mut rx = Rx::<u32>::unbound();
        rx.bind(rx_inner);
        rx.replenisher.inner = Some(replenisher.clone());

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
        drop(rx);

        assert_eq!(replenisher.dropped.load(Ordering::Acquire), 0);
    }

    #[test]
    fn logical_rx_drop_notifies_replenisher() {
        let (_tx, rx_inner) = channel_mailbox("vox_types.channel.test.logical_rx_drop", 1);
        let replenisher = Arc::new(CountingReplenisher::new());
        let core = Arc::new(ChannelCore::new(ChannelDebugContext::default()));
        core.bind_logical_receiver(BoundChannelReceiver {
            receiver: rx_inner,
            replenisher: Some(replenisher.clone()),
            writer_schema: None,
        });

        let rx = Rx::<u32>::paired(core);
        drop(rx);

        assert_eq!(replenisher.dropped.load(Ordering::Acquire), 1);
    }

    #[tokio::test]
    async fn rx_recv_logical_receiver_decodes_items_and_notifies_replenisher() {
        let (tx, rx_inner) = channel_mailbox("vox_types.channel.test.rx5", 1);
        let replenisher = Arc::new(CountingReplenisher::new());
        let core = Arc::new(ChannelCore::new(ChannelDebugContext::default()));
        core.bind_logical_receiver(BoundChannelReceiver {
            receiver: rx_inner,
            replenisher: Some(replenisher.clone()),
            writer_schema: None,
        });

        let mut rx = Rx::<u32>::paired(core);

        let encoded = vox_phon::to_vec(&321_u32).expect("serialize test item");
        let item = SelfRef::owning(
            Backing::Boxed(Box::<[u8]>::default()),
            ChannelItem {
                item: Payload::Encoded(Box::leak(encoded.into_boxed_slice())),
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
        assert_eq!(*value.get(), 321_u32);
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
            let (tx, rx) = channel_mailbox("vox_types.channel.test.bind_logical1", 8);
            // Keep the sender alive by leaking it — test only.
            std::mem::forget(tx);
            (
                self.alloc_id(),
                BoundChannelReceiver {
                    receiver: rx,
                    replenisher: None,
                    writer_schema: None,
                },
            )
        }

        fn bind_tx(&self, _channel_id: ChannelId) -> Arc<dyn ChannelSink> {
            Arc::new(CountingSink::new())
        }

        fn register_rx(&self, _channel_id: ChannelId) -> BoundChannelReceiver {
            let (tx, rx) = channel_mailbox("vox_types.channel.test.bind_logical2", 8);
            std::mem::forget(tx);
            BoundChannelReceiver {
                receiver: rx,
                replenisher: None,
                writer_schema: None,
            }
        }
    }

    // Case 1: Caller passes Tx in args, keeps paired Rx.
    // Encoding the Tx allocates a channel ID via create_rx(), records it in the
    // out-of-band collector (RequestCall.channels), and stores the receiver in
    // the shared logical core so the kept Rx can receive without appearing in
    // the serialized args payload.
    // r[verify rpc.channel.binding.caller-args]
    // r[verify rpc.channel.binding.caller-args.tx]
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
        let (bytes, channels) = collect_channels(|| {
            with_channel_binder(&binder, || vox_phon::to_vec(&args).expect("serialize"))
        });

        // The args still encode (the Tx is a small index); the channel id rode
        // out-of-band into the collected list.
        assert!(!bytes.is_empty());
        assert_eq!(channels.len(), 1, "one channel id collected out-of-band");

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
    // Encoding the Rx allocates a channel ID via create_tx(), records it
    // out-of-band, and stores the sink in the shared core so the kept Tx can use it.
    // r[verify rpc.channel.binding.caller-args]
    // r[verify rpc.channel.binding.caller-args.rx]
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
        let (bytes, channels) = collect_channels(|| {
            with_channel_binder(&binder, || vox_phon::to_vec(&args).expect("serialize"))
        });

        assert!(!bytes.is_empty());
        assert_eq!(channels.len(), 1, "one channel id collected out-of-band");

        // The kept Tx should now have a sink binding in the shared core.
        assert!(tx.core.inner.is_some());
        let core = tx.core.inner.as_ref().unwrap();
        assert!(
            core.get_sink().is_some(),
            "core should have a Sink binding from create_tx()"
        );
    }

    // Case 3: Callee deserializes Tx from args. The handle's inline index selects
    // its channel id from the provided out-of-band list, then binds via bind_tx().
    // r[verify rpc.channel.binding.callee-args]
    // r[verify rpc.channel.binding.callee-args.tx]
    #[test]
    fn case3_deserialize_tx_binds_via_binder() {
        use facet::Facet;

        #[derive(Facet)]
        struct Args {
            data: u32,
            tx: Tx<u32>,
        }

        // Produce real wire bytes + out-of-band channel list by encoding on the
        // caller side, then decode on the callee side.
        let (tx, _rx) = channel::<u32>();
        let caller_binder = TestBinder::new();
        let (bytes, channels) = collect_channels(|| {
            with_channel_binder(&caller_binder, || {
                vox_phon::to_vec(&Args { data: 42, tx }).expect("serialize")
            })
        });
        assert_eq!(channels.len(), 1);
        let expected_id = channels[0];

        let callee_binder = TestBinder::new();
        let args: Args = provide_channels(channels, || {
            with_channel_binder(&callee_binder, || {
                vox_phon::from_slice(&bytes).expect("deserialize")
            })
        });

        assert_eq!(args.data, 42);
        assert_eq!(args.tx.channel_id, expected_id);
        assert!(
            args.tx.is_bound(),
            "deserialized Tx should be bound via bind_tx()"
        );
    }

    // Case 4: Callee deserializes Rx from args, binding via register_rx().
    // r[verify rpc.channel.binding.callee-args]
    // r[verify rpc.channel.binding.callee-args.rx]
    #[test]
    fn case4_deserialize_rx_binds_via_binder() {
        use facet::Facet;

        #[derive(Facet)]
        struct Args {
            data: u32,
            rx: Rx<u32>,
        }

        let (_tx, rx) = channel::<u32>();
        let caller_binder = TestBinder::new();
        let (bytes, channels) = collect_channels(|| {
            with_channel_binder(&caller_binder, || {
                vox_phon::to_vec(&Args { data: 42, rx }).expect("serialize")
            })
        });
        assert_eq!(channels.len(), 1);
        let expected_id = channels[0];

        let callee_binder = TestBinder::new();
        let args: Args = provide_channels(channels, || {
            with_channel_binder(&callee_binder, || {
                vox_phon::from_slice(&bytes).expect("deserialize")
            })
        });

        assert_eq!(args.data, 42);
        assert_eq!(args.rx.channel_id, expected_id);
        assert!(
            args.rx.is_bound(),
            "deserialized Rx should be bound via register_rx()"
        );
    }

    // r[verify schema.exchange.channels.rx-args]
    #[tokio::test]
    async fn rx_recv_uses_method_channel_auxiliary_writer_schema() {
        use facet::Facet;

        #[derive(Facet)]
        struct Writer {
            a: u32,
            gone: String,
            b: u32,
        }

        #[derive(Debug, Facet, PartialEq)]
        struct Reader {
            a: u32,
            b: u32,
            #[facet(default)]
            added: u32,
        }
        unsafe impl crate::Reborrow for Reader {
            type Ref<'a> = Reader;
        }

        #[derive(Facet)]
        struct WriterArgs {
            rx: Rx<Writer>,
        }

        #[derive(Facet)]
        struct ReaderArgs {
            rx: Rx<Reader>,
        }

        let writer_method = crate::method_descriptor::<WriterArgs, ()>(
            "StreamService",
            "push",
            &["rx"],
            &[Some(Writer::SHAPE)],
            crate::MethodDescriptorOptions {
                response_wire_shape: <() as Facet>::SHAPE,
                doc: None,
            },
        );
        let reader_method = crate::method_descriptor::<ReaderArgs, ()>(
            "StreamService",
            "push",
            &["rx"],
            &[Some(Reader::SHAPE)],
            crate::MethodDescriptorOptions {
                response_wire_shape: <() as Facet>::SHAPE,
                doc: None,
            },
        );
        assert_eq!(writer_method.id, reader_method.id);

        let prepared =
            crate::SchemaSendTracker::plan_for_method_args(writer_method).expect("schema plan");
        let tracker = crate::SchemaRecvTracker::new();
        tracker.record_received(
            writer_method.id,
            BindingDirection::Args,
            prepared.bytes.clone(),
        );

        let (_kept_tx, rx) = channel::<Writer>();
        let caller_binder = TestBinder::new();
        let (bytes, channels) = collect_channels(|| {
            with_channel_binder(&caller_binder, || {
                vox_phon::to_vec(&WriterArgs { rx }).expect("serialize writer args")
            })
        });

        struct RegisterRxBinder {
            sender: std::sync::Mutex<Option<ChannelMailboxSender<IncomingChannelMessage>>>,
        }

        impl ChannelBinder for RegisterRxBinder {
            fn create_tx(&self) -> (ChannelId, Arc<dyn ChannelSink>) {
                (ChannelId(900), Arc::new(CountingSink::new()))
            }

            fn create_rx(&self) -> (ChannelId, BoundChannelReceiver) {
                let (sender, receiver) =
                    channel_mailbox("vox_types.channel.test.schema_rx_create", 8);
                *self.sender.lock().expect("sender mutex poisoned") = Some(sender);
                (
                    ChannelId(902),
                    BoundChannelReceiver {
                        receiver,
                        replenisher: None,
                        writer_schema: None,
                    },
                )
            }

            fn bind_tx(&self, _channel_id: ChannelId) -> Arc<dyn ChannelSink> {
                Arc::new(CountingSink::new())
            }

            fn register_rx(&self, _channel_id: ChannelId) -> BoundChannelReceiver {
                let (sender, receiver) =
                    channel_mailbox("vox_types.channel.test.schema_rx_register", 8);
                *self.sender.lock().expect("sender mutex poisoned") = Some(sender);
                BoundChannelReceiver {
                    receiver,
                    replenisher: None,
                    writer_schema: None,
                }
            }
        }

        let callee_binder = RegisterRxBinder {
            sender: std::sync::Mutex::new(None),
        };
        let mut args: ReaderArgs =
            provide_channels_for_method(channels, reader_method, &tracker, || {
                with_channel_binder(&callee_binder, || {
                    vox_phon::from_slice(&bytes).expect("deserialize reader args")
                })
            });
        assert!(args.rx.decoder.writer.is_some());

        let item_bytes = vox_phon::to_vec(&Writer {
            a: 11,
            gone: "discard".to_string(),
            b: 22,
        })
        .expect("serialize writer item");
        let item = SelfRef::owning(
            Backing::Boxed(Box::<[u8]>::default()),
            ChannelItem {
                item: Payload::Encoded(Box::leak(item_bytes.into_boxed_slice())),
            },
        );
        let sender = callee_binder
            .sender
            .lock()
            .expect("sender mutex poisoned")
            .clone()
            .expect("register_rx should store sender");
        sender
            .send(IncomingChannelMessage::Item(item))
            .await
            .expect("send channel item");

        let decoded = args
            .rx
            .recv()
            .await
            .expect("recv should compat decode")
            .expect("expected channel item");
        assert_eq!(
            *decoded.get(),
            Reader {
                a: 11,
                b: 22,
                added: 0
            }
        );
        assert!(args.rx.decoder.program.is_some());
    }

    // Round-trip: encode with caller binder + collector, decode with callee binder
    // + provided list. Verifies the channel ID allocated at encode is the one the
    // decoded handle re-associates by index.
    // r[verify rpc.channel.binding]
    // r[verify rpc.channel.payload-encoding]
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
        let (bytes, channels) = collect_channels(|| {
            with_channel_binder(&caller_binder, || {
                vox_phon::to_vec(&args).expect("serialize")
            })
        });

        // The caller binder starts at ID 100, so the only channel id is 100.
        assert_eq!(channels, vec![ChannelId(100)]);

        let callee_binder = TestBinder::new();
        let deserialized: Args = provide_channels(channels, || {
            with_channel_binder(&callee_binder, || {
                vox_phon::from_slice(&bytes).expect("deserialize")
            })
        });

        assert_eq!(deserialized.tx.channel_id, ChannelId(100));
        assert!(deserialized.tx.is_bound());
    }

    // r[verify rpc.channel.discovery]
    // r[verify rpc.channel.payload-encoding]
    #[test]
    fn direct_tuple_arg_channels_are_collected_in_argument_order() {
        let (first, _r) = channel::<u32>();
        let (_t, second) = channel::<u32>();
        let args = (first, second);

        let caller_binder = TestBinder::new();
        let (bytes, channels) = collect_channels(|| {
            with_channel_binder(&caller_binder, || {
                vox_phon::to_vec(&args).expect("serialize")
            })
        });
        assert_eq!(channels.len(), 2, "two distinct channels collected");
        assert_ne!(channels[0], channels[1]);

        let callee_binder = TestBinder::new();
        let decoded: (Tx<u32>, Rx<u32>) = provide_channels(channels.clone(), || {
            with_channel_binder(&callee_binder, || {
                vox_phon::from_slice(&bytes).expect("deserialize")
            })
        });
        assert_eq!(decoded.0.channel_id, channels[0]);
        assert_eq!(decoded.1.channel_id, channels[1]);
    }
}
