// ============================================================================
// Channel creation
// ============================================================================

use std::convert::Infallible;
use std::marker::PhantomData;

use facet::Facet;
use peeps_tasks::PeepableFutureExt;

use crate::runtime::{Receiver, Sender};
use crate::{CHANNEL_SIZE, ChannelId, DriverMessage, IncomingChannelMessage, get_dispatch_context};

/// Create an unbound channel pair for channeled RPC.
///
/// Returns `(Tx<T>, Rx<T>)` with `channel_id: 0`. The `ConnectionHandle::call`
/// method will walk the args, find `Rx<T>` or `Tx<T>` fields, assign channel IDs,
/// and take the internal channel handles to register with the channel registry.
///
/// # Channel semantics (like regular mpsc)
///
/// - If caller wants to **send** data: pass `rx`, keep `tx`
/// - If caller wants to **receive** data: pass `tx`, keep `rx`
///
/// # Example
///
/// ```ignore
/// // sum(numbers: Rx<i32>) -> i64
/// let (tx, rx) = roam::channel::<i32>();
/// let fut = client.sum(rx);  // pass rx, keep tx
/// tx.send(1).await;
/// tx.send(2).await;
/// drop(tx);
/// let sum = fut.await?;
/// ```
pub fn channel<T: 'static>() -> (Tx<T>, Rx<T>) {
    let (sender, receiver) = crate::runtime::channel("roam_channel", CHANNEL_SIZE);

    // Check if we're in a dispatch context - if so, create bound channels
    if let Some(ctx) = get_dispatch_context() {
        let channel_id = ctx.channel_ids.next();
        debug!(channel_id, "roam::channel() creating bound channel pair");
        (
            Tx::bound(ctx.conn_id, channel_id, sender, ctx.driver_tx.clone()),
            Rx::bound(channel_id, receiver),
        )
    } else {
        trace!("roam::channel() creating unbound channel pair (no dispatch context)");
        (Tx::unbound(sender), Rx::unbound(receiver))
    }
}

// ============================================================================
// SenderSlot - Wrapper for Option<Sender> that implements Facet
// ============================================================================

/// A wrapper around `Option<Sender<IncomingChannelMessage>>` that implements Facet.
///
/// This allows `Poke::get_mut::<SenderSlot>()` to work, enabling `.take()`
/// via reflection. Used by `ConnectionHandle::call` to extract senders from
/// `Tx<T>` arguments and register them with the channel registry.
#[derive(Facet)]
#[facet(opaque)]
pub struct SenderSlot {
    /// The optional sender. Public within crate for `Tx::send()` access.
    pub(crate) inner: Option<Sender<IncomingChannelMessage>>,
}

impl SenderSlot {
    /// Create a slot containing a sender.
    pub fn new(tx: Sender<IncomingChannelMessage>) -> Self {
        Self { inner: Some(tx) }
    }

    /// Create an empty slot.
    pub fn empty() -> Self {
        Self { inner: None }
    }

    /// Take the sender out of the slot, leaving it empty.
    pub fn take(&mut self) -> Option<Sender<IncomingChannelMessage>> {
        self.inner.take()
    }

    /// Check if the slot contains a sender.
    pub fn is_some(&self) -> bool {
        self.inner.is_some()
    }

    /// Check if the slot is empty.
    pub fn is_none(&self) -> bool {
        self.inner.is_none()
    }

    /// Set the sender in this slot.
    ///
    /// Used by `ChannelRegistry::bind_channels` to hydrate a deserialized `Tx<T>`
    /// with an actual channel sender.
    pub fn set(&mut self, tx: Sender<IncomingChannelMessage>) {
        self.inner = Some(tx);
    }
}

// ============================================================================
// DriverTxSlot - Wrapper for Option<Sender<DriverMessage>> that implements Facet
// ============================================================================

/// A wrapper around `Option<Sender<DriverMessage>>` that implements Facet.
///
/// This allows `Poke::get_mut::<DriverTxSlot>()` to work, enabling reflection-based
/// hydration of `Tx<T>` handles on the server side. Sends Data/Close messages
/// directly to the connection driver.
#[derive(Facet)]
#[facet(opaque)]
pub struct DriverTxSlot {
    /// The optional sender. Public within crate for `Tx::send()` access.
    pub(crate) inner: Option<Sender<DriverMessage>>,
}

impl DriverTxSlot {
    /// Create a slot containing a task sender.
    pub fn new(tx: Sender<DriverMessage>) -> Self {
        Self { inner: Some(tx) }
    }

    /// Create an empty slot.
    pub fn empty() -> Self {
        Self { inner: None }
    }

    /// Take the sender out of the slot, leaving it empty.
    pub fn take(&mut self) -> Option<Sender<DriverMessage>> {
        self.inner.take()
    }

    /// Check if the slot contains a sender.
    pub fn is_some(&self) -> bool {
        self.inner.is_some()
    }

    /// Check if the slot is empty.
    pub fn is_none(&self) -> bool {
        self.inner.is_none()
    }

    /// Set the task sender in this slot.
    ///
    /// Used by `ChannelRegistry::bind_channels` to hydrate a deserialized `Tx<T>`
    /// with the connection's driver message channel.
    pub fn set(&mut self, tx: Sender<DriverMessage>) {
        self.inner = Some(tx);
    }

    /// Clone the sender if present.
    pub fn clone_inner(&self) -> Option<Sender<DriverMessage>> {
        self.inner.clone()
    }
}

/// Tx channel handle - caller sends data to callee.
///
/// r[impl channeling.caller-pov] - From caller's perspective, Tx means "I send".
/// r[impl channeling.type] - Serializes as u64 channel ID on wire.
/// r[impl channeling.holder-semantics] - The holder sends on this channel.
/// r[impl channeling.channels-outlive-response] - Tx channels may outlive Response.
/// r[impl channeling.lifecycle.immediate-data] - Can send Data before Response.
/// r[impl channeling.lifecycle.speculative] - Early Data may be wasted on error.
///
/// # Facet Implementation
///
/// Uses `#[facet(proxy = u64)]` so that:
/// - `channel_id` is pokeable (Connection can walk args and set channel IDs)
/// - Serializes as just a `u64` on the wire
/// - `T` is exposed as a type parameter for codegen introspection
///
/// # Two modes of operation
///
/// - **Client side**: `sender` holds a channel to an intermediate drain task.
///   `ConnectionHandle::call` takes the receiver and drains it to wire.
/// - **Server side**: `task_tx` holds a direct channel to the connection driver.
///   `ChannelRegistry::bind_channels` sets this, and `send()` writes `DriverMessage::Data`.
#[derive(Facet)]
#[facet(proxy = u64)]
pub struct Tx<T: 'static> {
    /// The connection ID this channel belongs to.
    pub conn_id: roam_wire::ConnectionId,
    /// The unique channel ID for this channel.
    /// Public so Connection can poke it when binding channels.
    pub channel_id: ChannelId,
    /// Channel sender for outgoing data (client-side mode).
    /// Used when Tx is created via `roam::channel()`.
    pub sender: SenderSlot,
    /// Direct driver message sender (server-side mode).
    /// Used when Tx is hydrated by `ChannelRegistry::bind_channels`.
    pub driver_tx: DriverTxSlot,
    /// Phantom data for the element type.
    #[facet(opaque)]
    _marker: PhantomData<T>,
}

/// Serialization: `&Tx<T>` -> u64 (extracts channel_id)
///
/// Uses TryFrom rather than From because facet's proxy mechanism requires TryFrom.
#[allow(clippy::infallible_try_from)]
impl<T: 'static> TryFrom<&Tx<T>> for u64 {
    type Error = Infallible;
    fn try_from(tx: &Tx<T>) -> Result<Self, Self::Error> {
        Ok(tx.channel_id)
    }
}

/// Deserialization: u64 -> `Tx<T>` (creates a "hollow" Tx)
///
/// Both sender slots are empty - the real sender gets set up by Connection
/// after deserialization when it binds the channel.
///
/// Uses TryFrom rather than From because facet's proxy mechanism requires TryFrom.
#[allow(clippy::infallible_try_from)]
impl<T: 'static> TryFrom<u64> for Tx<T> {
    type Error = Infallible;
    fn try_from(channel_id: u64) -> Result<Self, Self::Error> {
        // Create a hollow Tx - no actual sender, Connection will bind later
        // conn_id will be set when binding
        Ok(Tx {
            conn_id: roam_wire::ConnectionId::ROOT,
            channel_id,
            sender: SenderSlot::empty(),
            driver_tx: DriverTxSlot::empty(),
            _marker: PhantomData,
        })
    }
}

impl<T: 'static> Tx<T> {
    /// Create a new Tx handle with the given ID and sender (client-side mode).
    pub fn new(channel_id: ChannelId, tx: Sender<IncomingChannelMessage>) -> Self {
        Self {
            conn_id: roam_wire::ConnectionId::ROOT,
            channel_id,
            sender: SenderSlot::new(tx),
            driver_tx: DriverTxSlot::empty(),
            _marker: PhantomData,
        }
    }

    /// Create an unbound Tx with a sender but channel_id 0.
    ///
    /// Used by `roam::channel()` to create a pair before binding.
    /// Connection will poke the channel_id and conn_id when binding.
    pub fn unbound(tx: Sender<IncomingChannelMessage>) -> Self {
        Self {
            conn_id: roam_wire::ConnectionId::ROOT,
            channel_id: 0,
            sender: SenderSlot::new(tx),
            driver_tx: DriverTxSlot::empty(),
            _marker: PhantomData,
        }
    }

    /// Create a bound Tx with conn_id, channel_id and driver_tx already set.
    ///
    /// Used by `roam::channel()` when called during dispatch to create
    /// response channels that can send Data directly over the wire.
    pub fn bound(
        conn_id: roam_wire::ConnectionId,
        channel_id: ChannelId,
        tx: Sender<IncomingChannelMessage>,
        driver_tx: Sender<DriverMessage>,
    ) -> Self {
        Self {
            conn_id,
            channel_id,
            sender: SenderSlot::new(tx),
            driver_tx: DriverTxSlot::new(driver_tx),
            _marker: PhantomData,
        }
    }

    /// Get the channel ID.
    pub fn channel_id(&self) -> ChannelId {
        self.channel_id
    }

    /// Send a value on this channel.
    ///
    /// r[impl channeling.data] - Data messages carry serialized values.
    ///
    /// Works in two modes:
    /// - Client-side (or passthrough): sends raw bytes to intermediate channel (drained by connection)
    /// - Server-side: sends `DriverMessage::Data` directly to connection driver
    ///
    /// IMPORTANT: We prefer sender over driver_tx because when a channel created during
    /// dispatch is passed to a callback, the rx gets a NEW channel_id allocated by the
    /// caller's bind_streams. The drain task uses that new channel_id, while self.channel_id
    /// still has the old dispatch-context channel_id. By using sender, data flows through
    /// the drain task which uses the correct channel_id.
    pub async fn send(&self, value: &T) -> Result<(), TxError>
    where
        T: Facet<'static>,
    {
        let bytes = facet_postcard::to_vec(value).map_err(TxError::Serialize)?;

        // Prefer sender - data flows through drain task which has correct channel_id
        if let Some(tx) = self.sender.inner.as_ref() {
            tx.send(IncomingChannelMessage::Data(bytes))
                .peepable("tx.send.drain")
                .await
                .map_err(|_| TxError::Closed)
        }
        // Server-side path: sender was never set, so self.channel_id is the real id.
        // (If sender was taken after being set, self.channel_id would be stale — but
        // nothing takes the sender and then calls send().)
        else if let Some(task_tx) = self.driver_tx.inner.as_ref() {
            task_tx
                .send(DriverMessage::Data {
                    conn_id: self.conn_id,
                    channel_id: self.channel_id,
                    payload: bytes,
                })
                .peepable("tx.send.direct")
                .await
                .map_err(|_| TxError::Closed)
        } else {
            Err(TxError::Taken)
        }
    }
}

/// When a Tx is dropped, send a Close message.
///
/// r[impl channeling.close] - Close terminates the channel.
///
/// The Close path depends on how data was sent:
/// - If sender is present: data went through drain task, drain task sends Close when channel closes
/// - If only driver_tx is present: data went directly to driver, we send Close via driver_tx
impl<T: 'static> Drop for Tx<T> {
    fn drop(&mut self) {
        // If sender is still present, the drain task will handle Close when
        // the internal channel closes. Don't send Close via driver_tx because
        // it would use the wrong channel_id (dispatch-context id vs caller-allocated id).
        if self.sender.inner.is_some() {
            // Just drop the sender - drain task handles Close
            return;
        }

        // Sender was taken or never set - send Close via driver_tx if available
        if let Some(task_tx) = self.driver_tx.inner.take() {
            let conn_id = self.conn_id;
            let channel_id = self.channel_id;
            // Use try_send for synchronous Close delivery.
            // This ensures Close is queued before Response in dispatch_call.
            //
            // WARNING: If try_send fails (channel full), we spawn as fallback.
            // This creates a potential ordering issue where Close could arrive
            // after Response. To mitigate: task_tx channels should be sized
            // generously (256+) to make this unlikely. A proper fix would use
            // unbounded channels for task messages.
            if task_tx
                .try_send(DriverMessage::Close {
                    conn_id,
                    channel_id,
                })
                .is_err()
            {
                warn!(
                    conn_id = conn_id.raw(),
                    channel_id,
                    "failed to queue DriverMessage::Close with try_send, falling back to async send"
                );
                // Channel full or closed - spawn as fallback (see warning above)
                crate::runtime::spawn("roam_channel_close_drop_fallback", async move {
                    if task_tx
                        .send(DriverMessage::Close {
                            conn_id,
                            channel_id,
                        })
                        .await
                        .is_err()
                    {
                        warn!(
                            conn_id = conn_id.raw(),
                            channel_id, "failed to send DriverMessage::Close from drop fallback"
                        );
                    }
                });
            }
        }
    }
}

/// Error when sending on a Tx channel.
#[derive(Debug)]
pub enum TxError {
    /// Failed to serialize the value.
    Serialize(facet_postcard::SerializeError),
    /// The channel is closed.
    Closed,
    /// The sender was already taken (e.g., by ConnectionHandle::call).
    Taken,
}

impl std::fmt::Display for TxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TxError::Serialize(e) => write!(f, "serialize error: {e}"),
            TxError::Closed => write!(f, "channel closed"),
            TxError::Taken => write!(f, "sender was taken"),
        }
    }
}

impl std::error::Error for TxError {}

// ============================================================================
// ReceiverSlot - Wrapper for Option<Receiver> that implements Facet
// ============================================================================

/// A wrapper around `Option<Receiver<IncomingChannelMessage>>` that implements Facet.
///
/// This allows `Poke::get_mut::<ReceiverSlot>()` to work, enabling `.take()`
/// via reflection. Used by `ConnectionHandle::call` to extract receivers from
/// `Rx<T>` arguments and register them with the channel registry.
#[derive(Facet)]
#[facet(opaque)]
pub struct ReceiverSlot {
    /// The optional receiver. Public within crate for `Rx::recv()` access.
    pub(crate) inner: Option<Receiver<IncomingChannelMessage>>,
}

impl ReceiverSlot {
    /// Create a slot containing a receiver.
    pub fn new(rx: Receiver<IncomingChannelMessage>) -> Self {
        Self { inner: Some(rx) }
    }

    /// Create an empty slot.
    pub fn empty() -> Self {
        Self { inner: None }
    }

    /// Take the receiver out of the slot, leaving it empty.
    pub fn take(&mut self) -> Option<Receiver<IncomingChannelMessage>> {
        self.inner.take()
    }

    /// Check if the slot contains a receiver.
    pub fn is_some(&self) -> bool {
        self.inner.is_some()
    }

    /// Check if the slot is empty.
    pub fn is_none(&self) -> bool {
        self.inner.is_none()
    }

    /// Set the receiver in this slot.
    ///
    /// Used by `ChannelRegistry::bind_channels` to hydrate a deserialized `Rx<T>`
    /// with an actual channel receiver.
    pub fn set(&mut self, rx: Receiver<IncomingChannelMessage>) {
        self.inner = Some(rx);
    }
}

/// Rx channel handle - caller receives data from callee.
///
/// r[impl channeling.caller-pov] - From caller's perspective, Rx means "I receive".
/// r[impl channeling.type] - Serializes as u64 channel ID on wire.
/// r[impl channeling.holder-semantics] - The holder receives from this channel.
///
/// # Facet Implementation
///
/// Uses `#[facet(proxy = u64)]` so that:
/// - `channel_id` is pokeable (Connection can walk args and set channel IDs)
/// - Serializes as just a `u64` on the wire
/// - `T` is exposed as a type parameter for codegen introspection
///
/// The `receiver` field uses `ReceiverSlot` wrapper so that `ConnectionHandle::call`
/// can use `Poke::get_mut::<ReceiverSlot>()` to `.take()` the receiver and register
/// it with the channel registry.
#[derive(Facet)]
#[facet(proxy = u64)]
pub struct Rx<T: 'static> {
    /// The unique channel ID for this channel.
    /// Public so Connection can poke it when binding channels.
    pub channel_id: ChannelId,
    /// Channel receiver for incoming data.
    /// Uses ReceiverSlot so it's pokeable (can .take() via Poke).
    pub receiver: ReceiverSlot,
    /// Phantom data for the element type.
    #[facet(opaque)]
    _marker: PhantomData<T>,
}

/// Serialization: `&Rx<T>` -> u64 (extracts channel_id)
///
/// Uses TryFrom rather than From because facet's proxy mechanism requires TryFrom.
#[allow(clippy::infallible_try_from)]
impl<T: 'static> TryFrom<&Rx<T>> for u64 {
    type Error = Infallible;
    fn try_from(rx: &Rx<T>) -> Result<Self, Self::Error> {
        Ok(rx.channel_id)
    }
}

/// Deserialization: u64 -> `Rx<T>` (creates a "hollow" Rx)
///
/// The receiver is a placeholder - the real receiver gets set up by Connection
/// after deserialization when it binds the channel.
///
/// Uses TryFrom rather than From because facet's proxy mechanism requires TryFrom.
#[allow(clippy::infallible_try_from)]
impl<T: 'static> TryFrom<u64> for Rx<T> {
    type Error = Infallible;
    fn try_from(channel_id: u64) -> Result<Self, Self::Error> {
        // Create a hollow Rx - no actual receiver, Connection will bind later
        Ok(Rx {
            channel_id,
            receiver: ReceiverSlot::empty(),
            _marker: PhantomData,
        })
    }
}

impl<T: 'static> Rx<T> {
    /// Create a new Rx handle with the given ID and receiver.
    pub fn new(channel_id: ChannelId, rx: Receiver<IncomingChannelMessage>) -> Self {
        Self {
            channel_id,
            receiver: ReceiverSlot::new(rx),
            _marker: PhantomData,
        }
    }

    /// Create an unbound Rx with a receiver but channel_id 0.
    ///
    /// Used by `roam::channel()` to create a pair before binding.
    /// Connection will poke the channel_id when binding.
    pub fn unbound(rx: Receiver<IncomingChannelMessage>) -> Self {
        Self {
            channel_id: 0,
            receiver: ReceiverSlot::new(rx),
            _marker: PhantomData,
        }
    }

    /// Create a bound Rx with channel_id already set.
    ///
    /// Used by `roam::channel()` when called during dispatch to create
    /// response channels. The channel_id will be serialized and sent to
    /// the client, who will bind a receiver for incoming Data.
    pub fn bound(channel_id: ChannelId, rx: Receiver<IncomingChannelMessage>) -> Self {
        Self {
            channel_id,
            receiver: ReceiverSlot::new(rx),
            _marker: PhantomData,
        }
    }

    /// Get the channel ID.
    pub fn channel_id(&self) -> ChannelId {
        self.channel_id
    }

    /// Receive the next value from this channel.
    ///
    /// Returns `Ok(Some(value))` for each received value,
    /// `Ok(None)` when the channel is closed,
    /// or `Err` if deserialization fails.
    ///
    /// r[impl channeling.data] - Deserialize Data message payloads.
    /// r[impl channeling.data.invalid] - Caller must send Goodbye on deserialize error.
    pub async fn recv(&mut self) -> Result<Option<T>, RxError>
    where
        T: Facet<'static>,
    {
        let rx = self.receiver.inner.as_mut().ok_or(RxError::Taken)?;
        match rx.recv().peepable("rx.recv").await {
            Some(IncomingChannelMessage::Data(bytes)) => {
                let value = facet_postcard::from_slice(&bytes).map_err(RxError::Deserialize)?;
                Ok(Some(value))
            }
            Some(IncomingChannelMessage::Close) | None => Ok(None),
        }
    }
}

/// Error when receiving from an Rx channel.
#[derive(Debug)]
pub enum RxError {
    /// Failed to deserialize the value.
    Deserialize(facet_postcard::DeserializeError),
    /// The receiver was already taken (e.g., by ConnectionHandle::call).
    Taken,
}

impl std::fmt::Display for RxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RxError::Deserialize(e) => write!(f, "deserialize error: {e}"),
            RxError::Taken => write!(f, "receiver was taken"),
        }
    }
}

impl std::error::Error for RxError {}

#[cfg(test)]
mod tests {
    use super::*;
    use roam_wire::ConnectionId;

    #[tokio::test]
    async fn tx_drop_fallback_handles_closed_driver_channel() {
        let (driver_tx, mut driver_rx) = crate::runtime::channel::<DriverMessage>("test_driver", 1);

        driver_tx
            .try_send(DriverMessage::Data {
                conn_id: ConnectionId::ROOT,
                channel_id: 777,
                payload: vec![1],
            })
            .expect("seed message should fill single-slot channel");

        let (inner_tx, _inner_rx) =
            crate::runtime::channel::<IncomingChannelMessage>("test_inner", 1);
        let mut tx: Tx<Vec<u8>> = Tx::new(4242, inner_tx);
        tx.conn_id = ConnectionId::ROOT;
        tx.sender = SenderSlot::empty();
        tx.driver_tx = DriverTxSlot::new(driver_tx.clone());

        drop(driver_rx.recv().await);
        drop(driver_rx);

        drop(tx);
        for _ in 0..8 {
            tokio::task::yield_now().await;
        }
    }
}
