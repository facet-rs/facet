use std::convert::Infallible;
use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;
use std::sync::Arc;
#[cfg(not(target_arch = "wasm32"))]
use std::sync::Mutex;
#[cfg(not(target_arch = "wasm32"))]
use std::sync::atomic::{AtomicBool, Ordering};

use facet::Facet;
use facet_core::PtrConst;
#[cfg(not(target_arch = "wasm32"))]
use tokio::sync::{Semaphore, mpsc};

#[cfg(not(target_arch = "wasm32"))]
use crate::{ChannelClose, ChannelItem, ChannelReset, Metadata, Payload, SelfRef};

// r[impl rpc.channel.pair]
/// The binding stored in a channel core — either a sink or a receiver, never both.
#[cfg(not(target_arch = "wasm32"))]
pub enum ChannelBinding {
    Sink(Arc<dyn ChannelSink>),
    Receiver(mpsc::Receiver<IncomingChannelMessage>),
}

// r[impl rpc.channel.pair]
/// Shared state between a `Tx`/`Rx` pair created by `channel()`.
///
/// Contains a `Mutex<Option<ChannelBinding>>` that is written once during
/// binding and read/taken by the paired handle. The mutex is only locked
/// during binding (once) and on first use by the paired handle (once).
#[cfg(not(target_arch = "wasm32"))]
pub struct ChannelCore {
    binding: Mutex<Option<ChannelBinding>>,
}

#[cfg(not(target_arch = "wasm32"))]
impl ChannelCore {
    fn new() -> Self {
        Self {
            binding: Mutex::new(None),
        }
    }

    /// Store a binding in the core. Panics if already set.
    pub fn set_binding(&self, binding: ChannelBinding) {
        let mut guard = self.binding.lock().expect("channel core mutex poisoned");
        assert!(guard.is_none(), "channel binding already set");
        *guard = Some(binding);
    }

    /// Clone the sink from the core (for Tx reading the sink).
    /// Returns None if no sink has been set or if the binding is a Receiver.
    pub fn get_sink(&self) -> Option<Arc<dyn ChannelSink>> {
        let guard = self.binding.lock().expect("channel core mutex poisoned");
        match guard.as_ref() {
            Some(ChannelBinding::Sink(sink)) => Some(sink.clone()),
            _ => None,
        }
    }

    /// Take the receiver out of the core (for Rx on first recv).
    /// Returns None if no receiver has been set or if it was already taken.
    pub fn take_receiver(&self) -> Option<mpsc::Receiver<IncomingChannelMessage>> {
        let mut guard = self.binding.lock().expect("channel core mutex poisoned");
        match guard.take() {
            Some(ChannelBinding::Receiver(rx)) => Some(rx),
            other => {
                // Put it back if it wasn't a receiver
                *guard = other;
                None
            }
        }
    }
}

/// Slot for the shared channel core, accessible via facet reflection.
#[derive(Facet)]
#[facet(opaque)]
pub(crate) struct CoreSlot {
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) inner: Option<Arc<ChannelCore>>,
}

impl CoreSlot {
    pub(crate) fn empty() -> Self {
        Self {
            #[cfg(not(target_arch = "wasm32"))]
            inner: None,
        }
    }
}

// r[impl rpc.channel.pair]
/// Create a channel pair with shared state.
///
/// Both ends hold an `Arc` reference to the same `ChannelCore`. The framework
/// binds the handle that appears in args or return values, and the paired
/// handle reads or takes the binding from the shared core.
pub fn channel<T>() -> (Tx<T>, Rx<T>) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let core = Arc::new(ChannelCore::new());
        (Tx::paired(core.clone()), Rx::paired(core))
    }
    #[cfg(target_arch = "wasm32")]
    {
        (Tx::unbound(), Rx::unbound())
    }
}

/// Runtime sink implemented by the session driver.
///
/// The contract is strict: successful completion means the item has gone
/// through the conduit to the link commit boundary.
#[cfg(not(target_arch = "wasm32"))]
pub trait ChannelSink: Send + Sync + 'static {
    fn send_payload<'payload>(
        &self,
        payload: Payload<'payload>,
    ) -> Pin<Box<dyn Future<Output = Result<(), TxError>> + Send + 'payload>>;

    fn close_channel(
        &self,
        metadata: Metadata,
    ) -> Pin<Box<dyn Future<Output = Result<(), TxError>> + Send + 'static>>;

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
#[cfg(not(target_arch = "wasm32"))]
pub struct CreditSink<S: ChannelSink> {
    inner: S,
    credit: Arc<Semaphore>,
}

#[cfg(not(target_arch = "wasm32"))]
impl<S: ChannelSink> CreditSink<S> {
    // r[impl rpc.flow-control.credit.initial]
    // r[impl rpc.flow-control.credit.initial.zero]
    /// Wrap `inner` with `initial_credit` permits (the const generic `N`).
    pub fn new(inner: S, initial_credit: u32) -> Self {
        Self {
            inner,
            credit: Arc::new(Semaphore::new(initial_credit as usize)),
        }
    }

    /// Returns the credit semaphore. The driver holds a clone so
    /// `GrantCredit` messages can call `add_permits`.
    pub fn credit(&self) -> &Arc<Semaphore> {
        &self.credit
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl<S: ChannelSink> ChannelSink for CreditSink<S> {
    fn send_payload<'payload>(
        &self,
        payload: Payload<'payload>,
    ) -> Pin<Box<dyn Future<Output = Result<(), TxError>> + Send + 'payload>> {
        let credit = self.credit.clone();
        let fut = self.inner.send_payload(payload);
        Box::pin(async move {
            let permit = credit
                .acquire()
                .await
                .map_err(|_| TxError::Transport("channel credit semaphore closed".into()))?;
            permit.forget();
            fut.await
        })
    }

    fn close_channel(
        &self,
        metadata: Metadata,
    ) -> Pin<Box<dyn Future<Output = Result<(), TxError>> + Send + 'static>> {
        // Close does not consume credit — it's a control message.
        self.inner.close_channel(metadata)
    }

    fn close_channel_on_drop(&self) {
        self.inner.close_channel_on_drop();
    }
}

/// Message delivered to an `Rx` by the driver.
#[cfg(not(target_arch = "wasm32"))]
pub enum IncomingChannelMessage {
    Item(SelfRef<ChannelItem<'static>>),
    Close(SelfRef<ChannelClose<'static>>),
    Reset(SelfRef<ChannelReset<'static>>),
}

/// Sender-side runtime slot.
#[derive(Facet)]
#[facet(opaque)]
pub(crate) struct SinkSlot {
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) inner: Option<Arc<dyn ChannelSink>>,
}

impl SinkSlot {
    pub(crate) fn empty() -> Self {
        Self {
            #[cfg(not(target_arch = "wasm32"))]
            inner: None,
        }
    }
}

/// Receiver-side runtime slot.
#[derive(Facet)]
#[facet(opaque)]
pub(crate) struct ReceiverSlot {
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) inner: Option<mpsc::Receiver<IncomingChannelMessage>>,
}

impl ReceiverSlot {
    pub(crate) fn empty() -> Self {
        Self {
            #[cfg(not(target_arch = "wasm32"))]
            inner: None,
        }
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
#[facet(proxy = ())]
pub struct Tx<T, const N: usize = 16> {
    pub(crate) sink: SinkSlot,
    pub(crate) core: CoreSlot,
    #[cfg(not(target_arch = "wasm32"))]
    #[facet(opaque)]
    closed: AtomicBool,
    #[facet(opaque)]
    _marker: PhantomData<T>,
}

impl<T, const N: usize> Tx<T, N> {
    /// Create a standalone unbound Tx (used by deserialization).
    pub fn unbound() -> Self {
        Self {
            sink: SinkSlot::empty(),
            core: CoreSlot::empty(),
            #[cfg(not(target_arch = "wasm32"))]
            closed: AtomicBool::new(false),
            _marker: PhantomData,
        }
    }

    /// Create a Tx that is part of a `channel()` pair.
    #[cfg(not(target_arch = "wasm32"))]
    fn paired(core: Arc<ChannelCore>) -> Self {
        Self {
            sink: SinkSlot::empty(),
            core: CoreSlot { inner: Some(core) },
            closed: AtomicBool::new(false),
            _marker: PhantomData,
        }
    }

    pub fn is_bound(&self) -> bool {
        #[cfg(not(target_arch = "wasm32"))]
        {
            if self.sink.inner.is_some() {
                return true;
            }
            if let Some(core) = &self.core.inner {
                return core.get_sink().is_some();
            }
            false
        }
        #[cfg(target_arch = "wasm32")]
        false
    }

    /// Check if this Tx is part of a channel() pair (has a shared core).
    pub fn has_core(&self) -> bool {
        #[cfg(not(target_arch = "wasm32"))]
        return self.core.inner.is_some();
        #[cfg(target_arch = "wasm32")]
        return false;
    }

    // r[impl rpc.channel.pair.tx-read]
    #[cfg(not(target_arch = "wasm32"))]
    fn resolve_sink(&self) -> Result<Arc<dyn ChannelSink>, TxError> {
        // Fast path: local slot (standalone/callee-side handle)
        if let Some(sink) = &self.sink.inner {
            return Ok(sink.clone());
        }
        // Slow path: read from shared core (paired handle)
        if let Some(core) = &self.core.inner
            && let Some(sink) = core.get_sink()
        {
            return Ok(sink);
        }
        Err(TxError::Unbound)
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub async fn send<'value>(&self, value: T) -> Result<(), TxError>
    where
        T: Facet<'value>,
    {
        let sink = self.resolve_sink()?;
        let ptr = PtrConst::new((&value as *const T).cast::<u8>());
        // SAFETY: `value` is explicitly dropped only after `await`, so the pointer
        // remains valid for the whole send operation.
        let payload = unsafe { Payload::outgoing_unchecked(ptr, T::SHAPE) };
        let result = sink.send_payload(payload).await;
        drop(value);
        result
    }

    // r[impl rpc.channel.lifecycle]
    #[cfg(not(target_arch = "wasm32"))]
    pub async fn close<'value>(&self, metadata: Metadata<'value>) -> Result<(), TxError> {
        self.closed.store(true, Ordering::Release);
        let sink = self.resolve_sink()?;
        sink.close_channel(metadata).await
    }

    #[doc(hidden)]
    #[cfg(not(target_arch = "wasm32"))]
    pub fn bind(&mut self, sink: Arc<dyn ChannelSink>) {
        self.sink.inner = Some(sink);
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl<T, const N: usize> Drop for Tx<T, N> {
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

#[allow(clippy::infallible_try_from)]
impl<T, const N: usize> TryFrom<&Tx<T, N>> for () {
    type Error = Infallible;

    fn try_from(_value: &Tx<T, N>) -> Result<Self, Self::Error> {
        Ok(())
    }
}

#[allow(clippy::infallible_try_from)]
impl<T, const N: usize> TryFrom<()> for Tx<T, N> {
    type Error = Infallible;

    fn try_from(_value: ()) -> Result<Self, Self::Error> {
        Ok(Self::unbound())
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
/// Wire encoding is always unit (`()`), with channel IDs carried exclusively
/// in `Message::Request.channels`.
#[derive(Facet)]
#[facet(proxy = ())]
pub struct Rx<T, const N: usize = 16> {
    pub(crate) receiver: ReceiverSlot,
    pub(crate) core: CoreSlot,
    #[facet(opaque)]
    _marker: PhantomData<T>,
}

impl<T, const N: usize> Rx<T, N> {
    /// Create a standalone unbound Rx (used by deserialization).
    pub fn unbound() -> Self {
        Self {
            receiver: ReceiverSlot::empty(),
            core: CoreSlot::empty(),
            _marker: PhantomData,
        }
    }

    /// Create an Rx that is part of a `channel()` pair.
    #[cfg(not(target_arch = "wasm32"))]
    fn paired(core: Arc<ChannelCore>) -> Self {
        Self {
            receiver: ReceiverSlot::empty(),
            core: CoreSlot { inner: Some(core) },
            _marker: PhantomData,
        }
    }

    pub fn is_bound(&self) -> bool {
        #[cfg(not(target_arch = "wasm32"))]
        {
            if self.receiver.inner.is_some() {
                return true;
            }
            false
        }
        #[cfg(target_arch = "wasm32")]
        false
    }

    /// Check if this Rx is part of a channel() pair (has a shared core).
    pub fn has_core(&self) -> bool {
        #[cfg(not(target_arch = "wasm32"))]
        return self.core.inner.is_some();
        #[cfg(target_arch = "wasm32")]
        return false;
    }

    // r[impl rpc.channel.pair.rx-take]
    #[cfg(not(target_arch = "wasm32"))]
    pub async fn recv(&mut self) -> Result<Option<SelfRef<T>>, RxError>
    where
        T: Facet<'static>,
    {
        // On first call, take receiver from shared core into local slot
        if self.receiver.inner.is_none()
            && let Some(core) = &self.core.inner
            && let Some(rx) = core.take_receiver()
        {
            self.receiver.inner = Some(rx);
        }

        let receiver = self.receiver.inner.as_mut().ok_or(RxError::Unbound)?;
        match receiver.recv().await {
            Some(IncomingChannelMessage::Close(_)) | None => Ok(None),
            Some(IncomingChannelMessage::Reset(_)) => Err(RxError::Reset),
            Some(IncomingChannelMessage::Item(msg)) => msg
                .try_repack(|item, _backing_bytes| {
                    let Payload::Incoming(bytes) = item.item else {
                        return Err(RxError::Protocol(
                            "incoming channel item payload was not Incoming".into(),
                        ));
                    };
                    facet_postcard::from_slice_borrowed(bytes).map_err(RxError::Deserialize)
                })
                .map(Some),
        }
    }

    #[doc(hidden)]
    #[cfg(not(target_arch = "wasm32"))]
    pub fn bind(&mut self, receiver: mpsc::Receiver<IncomingChannelMessage>) {
        self.receiver.inner = Some(receiver);
    }
}

#[allow(clippy::infallible_try_from)]
impl<T, const N: usize> TryFrom<&Rx<T, N>> for () {
    type Error = Infallible;

    fn try_from(_value: &Rx<T, N>) -> Result<Self, Self::Error> {
        Ok(())
    }
}

#[allow(clippy::infallible_try_from)]
impl<T, const N: usize> TryFrom<()> for Rx<T, N> {
    type Error = Infallible;

    fn try_from(_value: ()) -> Result<Self, Self::Error> {
        Ok(Self::unbound())
    }
}

/// Error when receiving from an `Rx`.
#[derive(Debug)]
pub enum RxError {
    Unbound,
    Reset,
    Deserialize(facet_postcard::DeserializeError),
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
        ) -> Pin<Box<dyn Future<Output = Result<(), TxError>> + Send + 'payload>> {
            self.send_calls.fetch_add(1, Ordering::AcqRel);
            Box::pin(async { Ok(()) })
        }

        fn close_channel(
            &self,
            _metadata: Metadata,
        ) -> Pin<Box<dyn Future<Output = Result<(), TxError>> + Send + 'static>> {
            self.close_calls.fetch_add(1, Ordering::AcqRel);
            Box::pin(async { Ok(()) })
        }

        fn close_channel_on_drop(&self) {
            self.close_on_drop_calls.fetch_add(1, Ordering::AcqRel);
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
        core.set_binding(ChannelBinding::Sink(sink));
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
        let (tx, rx_inner) = mpsc::channel(1);
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
        let (tx, rx_inner) = mpsc::channel(1);
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

        let (tx, rx_inner) = mpsc::channel(1);
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
}
