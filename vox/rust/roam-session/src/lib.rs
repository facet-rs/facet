#![deny(unsafe_code)]

//! Session/state machine and RPC-level utilities.
//!
//! Canonical definitions live in `docs/content/spec/_index.md`,
//! `docs/content/rust-spec/_index.md`, and `docs/content/shm-spec/_index.md`.

#[macro_use]
mod macros;

pub mod diagnostic;
pub mod driver;
pub mod runtime;
pub mod transport;

pub use driver::{
    ConnectError, ConnectionError, Driver, FramedClient, HandshakeConfig, IncomingConnection,
    IncomingConnections, MessageConnector, Negotiated, NoDispatcher, RetryPolicy, accept_framed,
    connect_framed, connect_framed_with_policy, initiate_framed,
};
pub use transport::MessageTransport;

use std::marker::PhantomData;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::runtime::{OneshotSender, Receiver, Sender, oneshot};
use facet::Facet;
use std::convert::Infallible;

pub use roam_frame::{Frame, MsgDesc, OwnedMessage, Payload};

const CHANNEL_SIZE: usize = 1024;
const RX_STREAM_BUFFER_SIZE: usize = 1024;

// ============================================================================
// Streaming types
// ============================================================================

/// Stream ID type.
pub type ChannelId = u64;

/// Connection role - determines stream ID parity.
///
/// The initiator is whoever opened the connection (e.g. connected to a TCP socket,
/// or opened an SHM channel). The acceptor is whoever accepted/received the connection.
///
/// r[impl channeling.id.parity]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// Initiator uses odd stream IDs (1, 3, 5, ...).
    Initiator,
    /// Acceptor uses even stream IDs (2, 4, 6, ...).
    Acceptor,
}

/// Allocates unique stream IDs with correct parity.
///
/// r[impl channeling.id.uniqueness] - IDs are unique within a connection.
/// r[impl channeling.id.parity] - Initiator uses odd, Acceptor uses even.
pub struct ChannelIdAllocator {
    next: AtomicU64,
}

impl ChannelIdAllocator {
    /// Create a new allocator for the given role.
    pub fn new(role: Role) -> Self {
        let start = match role {
            Role::Initiator => 1, // odd: 1, 3, 5, ...
            Role::Acceptor => 2,  // even: 2, 4, 6, ...
        };
        Self {
            next: AtomicU64::new(start),
        }
    }

    /// Allocate the next stream ID.
    pub fn next(&self) -> ChannelId {
        self.next.fetch_add(2, Ordering::Relaxed)
    }
}

// ============================================================================
// SenderSlot - Wrapper for Option<Sender> that implements Facet
// ============================================================================

/// A wrapper around `Option<Sender<Vec<u8>>>` that implements Facet.
///
/// This allows `Poke::get_mut::<SenderSlot>()` to work, enabling `.take()`
/// via reflection. Used by `ConnectionHandle::call` to extract senders from
/// `Tx<T>` arguments and register them with the stream registry.
#[derive(Facet)]
#[facet(opaque)]
pub struct SenderSlot {
    /// The optional sender. Public within crate for `Tx::send()` access.
    pub(crate) inner: Option<Sender<Vec<u8>>>,
}

impl SenderSlot {
    /// Create a slot containing a sender.
    pub fn new(tx: Sender<Vec<u8>>) -> Self {
        Self { inner: Some(tx) }
    }

    /// Create an empty slot.
    pub fn empty() -> Self {
        Self { inner: None }
    }

    /// Take the sender out of the slot, leaving it empty.
    pub fn take(&mut self) -> Option<Sender<Vec<u8>>> {
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
    /// Used by `ChannelRegistry::bind_streams` to hydrate a deserialized `Tx<T>`
    /// with an actual channel sender.
    pub fn set(&mut self, tx: Sender<Vec<u8>>) {
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
    /// Used by `ChannelRegistry::bind_streams` to hydrate a deserialized `Tx<T>`
    /// with the connection's task message channel.
    pub fn set(&mut self, tx: Sender<DriverMessage>) {
        self.inner = Some(tx);
    }

    /// Clone the sender if present.
    pub fn clone_inner(&self) -> Option<Sender<DriverMessage>> {
        self.inner.clone()
    }
}

/// Tx stream handle - caller sends data to callee.
///
/// r[impl channeling.caller-pov] - From caller's perspective, Tx means "I send".
/// r[impl channeling.type] - Serializes as u64 stream ID on wire.
/// r[impl channeling.holder-semantics] - The holder sends on this stream.
/// r[impl channeling.channels-outlive-response] - Tx streams may outlive Response.
/// r[impl channeling.lifecycle.immediate-data] - Can send Data before Response.
/// r[impl channeling.lifecycle.speculative] - Early Data may be wasted on error.
///
/// # Facet Implementation
///
/// Uses `#[facet(proxy = u64)]` so that:
/// - `channel_id` is pokeable (Connection can walk args and set stream IDs)
/// - Serializes as just a `u64` on the wire
/// - `T` is exposed as a type parameter for codegen introspection
///
/// # Two modes of operation
///
/// - **Client side**: `sender` holds a channel to an intermediate drain task.
///   `ConnectionHandle::call` takes the receiver and drains it to wire.
/// - **Server side**: `task_tx` holds a direct channel to the connection driver.
///   `ChannelRegistry::bind_streams` sets this, and `send()` writes `DriverMessage::Data`.
#[derive(Facet)]
#[facet(proxy = u64)]
pub struct Tx<T: 'static> {
    /// The connection ID this stream belongs to.
    pub conn_id: roam_wire::ConnectionId,
    /// The unique stream ID for this stream.
    /// Public so Connection can poke it when binding streams.
    pub channel_id: ChannelId,
    /// Channel sender for outgoing data (client-side mode).
    /// Used when Tx is created via `roam::channel()`.
    pub sender: SenderSlot,
    /// Direct driver message sender (server-side mode).
    /// Used when Tx is hydrated by `ChannelRegistry::bind_streams`.
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
/// after deserialization when it binds the stream.
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
    /// Create a new Tx stream with the given ID and sender channel (client-side mode).
    pub fn new(channel_id: ChannelId, tx: Sender<Vec<u8>>) -> Self {
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
    pub fn unbound(tx: Sender<Vec<u8>>) -> Self {
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
        tx: Sender<Vec<u8>>,
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

    /// Get the stream ID.
    pub fn channel_id(&self) -> ChannelId {
        self.channel_id
    }

    /// Send a value on this stream.
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
            tx.send(bytes).await.map_err(|_| TxError::Closed)
        }
        // Fallback to direct driver_tx (sender was taken or never set)
        else if let Some(task_tx) = self.driver_tx.inner.as_ref() {
            task_tx
                .send(DriverMessage::Data {
                    conn_id: self.conn_id,
                    channel_id: self.channel_id,
                    payload: bytes,
                })
                .await
                .map_err(|_| TxError::Closed)
        } else {
            Err(TxError::Taken)
        }
    }
}

/// When a Tx is dropped, send a Close message.
///
/// r[impl channeling.close] - Close terminates the stream.
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
                // Channel full or closed - spawn as fallback (see warning above)
                crate::runtime::spawn(async move {
                    let _ = task_tx
                        .send(DriverMessage::Close {
                            conn_id,
                            channel_id,
                        })
                        .await;
                });
            }
        }
    }
}

/// Error when sending on a Tx stream.
#[derive(Debug)]
pub enum TxError {
    /// Failed to serialize the value.
    Serialize(facet_postcard::SerializeError),
    /// The stream channel is closed.
    Closed,
    /// The sender was already taken (e.g., by ConnectionHandle::call).
    Taken,
}

impl std::fmt::Display for TxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TxError::Serialize(e) => write!(f, "serialize error: {e}"),
            TxError::Closed => write!(f, "stream closed"),
            TxError::Taken => write!(f, "sender was taken"),
        }
    }
}

impl std::error::Error for TxError {}

// ============================================================================
// ReceiverSlot - Wrapper for Option<Receiver> that implements Facet
// ============================================================================

/// A wrapper around `Option<Receiver<Vec<u8>>>` that implements Facet.
///
/// This allows `Poke::get_mut::<ReceiverSlot>()` to work, enabling `.take()`
/// via reflection. Used by `ConnectionHandle::call` to extract receivers from
/// `Rx<T>` arguments and register them with the stream registry.
#[derive(Facet)]
#[facet(opaque)]
pub struct ReceiverSlot {
    /// The optional receiver. Public within crate for `Rx::recv()` access.
    pub(crate) inner: Option<Receiver<Vec<u8>>>,
}

impl ReceiverSlot {
    /// Create a slot containing a receiver.
    pub fn new(rx: Receiver<Vec<u8>>) -> Self {
        Self { inner: Some(rx) }
    }

    /// Create an empty slot.
    pub fn empty() -> Self {
        Self { inner: None }
    }

    /// Take the receiver out of the slot, leaving it empty.
    pub fn take(&mut self) -> Option<Receiver<Vec<u8>>> {
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
    /// Used by `ChannelRegistry::bind_streams` to hydrate a deserialized `Rx<T>`
    /// with an actual channel receiver.
    pub fn set(&mut self, rx: Receiver<Vec<u8>>) {
        self.inner = Some(rx);
    }
}

/// Rx stream handle - caller receives data from callee.
///
/// r[impl channeling.caller-pov] - From caller's perspective, Rx means "I receive".
/// r[impl channeling.type] - Serializes as u64 stream ID on wire.
/// r[impl channeling.holder-semantics] - The holder receives from this stream.
///
/// # Facet Implementation
///
/// Uses `#[facet(proxy = u64)]` so that:
/// - `channel_id` is pokeable (Connection can walk args and set stream IDs)
/// - Serializes as just a `u64` on the wire
/// - `T` is exposed as a type parameter for codegen introspection
///
/// The `receiver` field uses `ReceiverSlot` wrapper so that `ConnectionHandle::call`
/// can use `Poke::get_mut::<ReceiverSlot>()` to `.take()` the receiver and register
/// it with the stream registry.
#[derive(Facet)]
#[facet(proxy = u64)]
pub struct Rx<T: 'static> {
    /// The unique stream ID for this stream.
    /// Public so Connection can poke it when binding streams.
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
/// after deserialization when it binds the stream.
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
    /// Create a new Rx stream with the given ID and receiver channel.
    pub fn new(channel_id: ChannelId, rx: Receiver<Vec<u8>>) -> Self {
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
    pub fn unbound(rx: Receiver<Vec<u8>>) -> Self {
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
    pub fn bound(channel_id: ChannelId, rx: Receiver<Vec<u8>>) -> Self {
        Self {
            channel_id,
            receiver: ReceiverSlot::new(rx),
            _marker: PhantomData,
        }
    }

    /// Get the stream ID.
    pub fn channel_id(&self) -> ChannelId {
        self.channel_id
    }

    /// Receive the next value from this stream.
    ///
    /// Returns `Ok(Some(value))` for each received value,
    /// `Ok(None)` when the stream is closed,
    /// or `Err` if deserialization fails.
    ///
    /// r[impl channeling.data] - Deserialize Data message payloads.
    /// r[impl channeling.data.invalid] - Caller must send Goodbye on deserialize error.
    pub async fn recv(&mut self) -> Result<Option<T>, RxError>
    where
        T: Facet<'static>,
    {
        let rx = self.receiver.inner.as_mut().ok_or(RxError::Taken)?;
        match rx.recv().await {
            Some(bytes) => {
                let value = facet_postcard::from_slice(&bytes).map_err(RxError::Deserialize)?;
                Ok(Some(value))
            }
            None => Ok(None),
        }
    }
}

/// Error when receiving from a Rx stream.
#[derive(Debug)]
pub enum RxError {
    /// Failed to deserialize the value.
    Deserialize(facet_postcard::DeserializeError<facet_postcard::PostcardError>),
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

// ============================================================================
// Channel creation
// ============================================================================

/// Create an unbound channel pair for streaming RPC.
///
/// Returns `(Tx<T>, Rx<T>)` with `channel_id: 0`. The `ConnectionHandle::call`
/// method will walk the args, find `Rx<T>` or `Tx<T>` fields, assign stream IDs,
/// and take the internal channel handles to register with the stream registry.
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
    let (sender, receiver) = crate::runtime::channel(CHANNEL_SIZE);

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
// Dispatch Context (task-local for response channel binding)
// ============================================================================

/// Context for binding response channels during dispatch.
///
/// When a service handler creates a channel with `roam::channel()` and returns
/// the Rx, the Tx needs to be bound to send Data over the wire. This context
/// provides the channel ID allocator and driver_tx needed for binding.
#[derive(Clone)]
struct DispatchContext {
    conn_id: roam_wire::ConnectionId,
    channel_ids: Arc<ChannelIdAllocator>,
    driver_tx: Sender<DriverMessage>,
}

roam_task_local::task_local! {
    /// Task-local dispatch context. Using task_local instead of thread_local
    /// is critical: thread_local can leak across different async tasks that
    /// happen to run on the same worker thread, causing channel binding bugs.
    static DISPATCH_CONTEXT: DispatchContext;
}

/// Get the current dispatch context, if any.
fn get_dispatch_context() -> Option<DispatchContext> {
    DISPATCH_CONTEXT.try_with(|ctx| ctx.clone()).ok()
}

// ============================================================================
// Stream Registry
// ============================================================================

use std::collections::{HashMap, HashSet};

/// Response data returned from a call, including any response stream channels.
#[derive(Debug)]
pub struct ResponseData {
    /// The response payload bytes.
    pub payload: Vec<u8>,
    /// Channel IDs for streams in the response (`Rx<T>` returned by the method).
    /// Client must register receivers for these channels.
    pub channels: Vec<u64>,
}

/// All messages to the connection driver go through a single channel.
///
/// This unified channel ensures FIFO ordering: a Call followed by Data
/// will always be processed in that order, preventing race conditions
/// where Data could arrive before the Request is sent.
pub enum DriverMessage {
    /// Send a Request and expect a Response (client-side call).
    Call {
        conn_id: roam_wire::ConnectionId,
        request_id: u64,
        method_id: u64,
        metadata: Vec<(String, roam_wire::MetadataValue)>,
        /// Channel IDs used by this call (Tx/Rx), in declaration order.
        channels: Vec<u64>,
        payload: Vec<u8>,
        response_tx: OneshotSender<Result<ResponseData, TransportError>>,
    },
    /// Send a Data message on a stream.
    Data {
        conn_id: roam_wire::ConnectionId,
        channel_id: ChannelId,
        payload: Vec<u8>,
    },
    /// Send a Close message to end a stream.
    Close {
        conn_id: roam_wire::ConnectionId,
        channel_id: ChannelId,
    },
    /// Send a Response message (server-side call completed).
    Response {
        conn_id: roam_wire::ConnectionId,
        request_id: u64,
        /// Channel IDs for streams in the response (Tx/Rx returned by the method).
        channels: Vec<u64>,
        payload: Vec<u8>,
    },
    /// Request to open a new virtual connection.
    Connect {
        request_id: u64,
        metadata: roam_wire::Metadata,
        response_tx: OneshotSender<Result<ConnectionHandle, crate::ConnectError>>,
    },
}

/// Registry of active streams for a connection.
///
/// Handles incoming streams (Data from wire → `Rx<T>` / `Tx<T>` handles).
/// For outgoing streams (server `Tx<T>` args), spawned tasks drain receivers
/// and send Data/Close messages via `driver_tx`.
///
/// r[impl channeling.unknown] - Unknown stream IDs cause Goodbye.
pub struct ChannelRegistry {
    /// Connection ID this registry belongs to.
    conn_id: roam_wire::ConnectionId,

    /// Streams where we receive Data messages (backing `Rx<T>` or `Tx<T>` handles on our side).
    /// Key: channel_id, Value: sender to route Data payloads to the handle.
    incoming: HashMap<ChannelId, Sender<Vec<u8>>>,

    /// Stream IDs that have been closed.
    /// Used to detect data-after-close violations.
    ///
    /// r[impl channeling.data-after-close] - Track closed streams.
    closed: HashSet<ChannelId>,

    // ========================================================================
    // Flow Control
    // ========================================================================
    /// r[impl flow.channel.credit-based] - Credit tracking for incoming streams.
    /// r[impl flow.channel.all-transports] - Flow control applies to all transports.
    /// This is the credit we've granted to the peer - bytes they can still send us.
    /// Decremented when we receive Data, incremented when we send Credit.
    incoming_credit: HashMap<ChannelId, u32>,

    /// r[impl flow.channel.credit-based] - Credit tracking for outgoing streams.
    /// r[impl flow.channel.all-transports] - Flow control applies to all transports.
    /// This is the credit peer granted us - bytes we can still send them.
    /// Decremented when we send Data, incremented when we receive Credit.
    outgoing_credit: HashMap<ChannelId, u32>,

    /// Initial credit to grant new streams.
    /// r[impl flow.channel.initial-credit] - Each stream starts with this credit.
    initial_credit: u32,

    /// Unified channel for all messages to the driver.
    /// The driver owns the receiving end and sends these on the wire.
    /// Using a single channel ensures FIFO ordering.
    driver_tx: Sender<DriverMessage>,

    /// Channel ID allocator for response channels created during dispatch.
    /// These are channels returned by service methods (e.g., `subscribe() -> Rx<Event>`).
    response_channel_ids: Arc<ChannelIdAllocator>,
}

impl ChannelRegistry {
    /// Create a new registry with the given conn_id, initial credit, driver channel, and role.
    ///
    /// The `driver_tx` is used to send all messages (Call/Data/Close/Response)
    /// to the driver for transmission on the wire.
    ///
    /// The `role` determines channel ID parity for response channels:
    /// - Acceptor (server) uses even IDs
    /// - Initiator (client) uses odd IDs
    ///
    /// r[impl flow.channel.initial-credit] - Each stream starts with this credit.
    pub fn new_with_credit_and_role(
        conn_id: roam_wire::ConnectionId,
        initial_credit: u32,
        driver_tx: Sender<DriverMessage>,
        role: Role,
    ) -> Self {
        Self {
            conn_id,
            incoming: HashMap::new(),
            closed: HashSet::new(),
            incoming_credit: HashMap::new(),
            outgoing_credit: HashMap::new(),
            initial_credit,
            driver_tx,
            response_channel_ids: Arc::new(ChannelIdAllocator::new(role)),
        }
    }

    /// Create a new registry with the given initial credit and driver channel.
    /// Uses ROOT conn_id and Acceptor role for backward compatibility (server-side usage).
    ///
    /// r[impl flow.channel.initial-credit] - Each stream starts with this credit.
    pub fn new_with_credit(initial_credit: u32, driver_tx: Sender<DriverMessage>) -> Self {
        Self::new_with_credit_and_role(
            roam_wire::ConnectionId::ROOT,
            initial_credit,
            driver_tx,
            Role::Acceptor,
        )
    }

    /// Create a new registry with default infinite credit.
    ///
    /// r[impl flow.channel.infinite-credit] - Implementations MAY use very large credit.
    /// r[impl flow.channel.zero-credit] - With infinite credit, zero-credit never occurs.
    /// This disables backpressure but simplifies implementation.
    pub fn new(driver_tx: Sender<DriverMessage>) -> Self {
        Self::new_with_credit(u32::MAX, driver_tx)
    }

    /// Get the connection ID for this registry.
    pub fn conn_id(&self) -> roam_wire::ConnectionId {
        self.conn_id
    }

    /// Get the dispatch context for response channel binding.
    ///
    /// Used by `dispatch_call` and `dispatch_call_infallible` to set up
    /// thread-local context so `roam::channel()` can create bound channels.
    pub(crate) fn dispatch_context(&self) -> DispatchContext {
        DispatchContext {
            conn_id: self.conn_id,
            channel_ids: self.response_channel_ids.clone(),
            driver_tx: self.driver_tx.clone(),
        }
    }

    /// Get a clone of the driver message sender.
    ///
    /// Used by codegen to spawn tasks that send Data/Close/Response messages.
    pub fn driver_tx(&self) -> Sender<DriverMessage> {
        self.driver_tx.clone()
    }

    /// Get the response channel ID allocator.
    /// Used by ForwardingDispatcher to allocate downstream channel IDs for response channels.
    pub fn response_channel_ids(&self) -> Arc<ChannelIdAllocator> {
        self.response_channel_ids.clone()
    }

    /// Register an incoming stream.
    ///
    /// The connection layer will route Data messages for this channel_id to the sender.
    /// Used for both `Rx<T>` (caller receives from callee) and `Tx<T>` (callee sends to caller).
    ///
    /// r[impl flow.channel.initial-credit] - Stream starts with initial credit.
    pub fn register_incoming(&mut self, channel_id: ChannelId, tx: Sender<Vec<u8>>) {
        self.incoming.insert(channel_id, tx);
        // Grant initial credit - peer can send us this many bytes
        self.incoming_credit.insert(channel_id, self.initial_credit);
    }

    /// Register credit tracking for an outgoing stream.
    ///
    /// The actual receiver is NOT stored here - the driver owns it directly.
    /// This only sets up credit tracking for the stream.
    ///
    /// r[impl flow.channel.initial-credit] - Stream starts with initial credit.
    pub fn register_outgoing_credit(&mut self, channel_id: ChannelId) {
        // Assume peer grants us initial credit - we can send them this many bytes
        self.outgoing_credit.insert(channel_id, self.initial_credit);
    }

    /// Route a Data message payload to the appropriate incoming stream.
    ///
    /// Returns Ok(()) if routed successfully, Err(ChannelError) otherwise.
    ///
    /// r[impl channeling.data] - Data messages routed by channel_id.
    /// r[impl channeling.data-after-close] - Reject data on closed streams.
    /// r[impl flow.channel.credit-overrun] - Reject if data exceeds remaining credit.
    /// r[impl flow.channel.credit-consume] - Deduct bytes from remaining credit.
    /// r[impl flow.channel.byte-accounting] - Credit measured in payload bytes.
    ///
    /// Returns a sender and payload if routing is allowed, or an error.
    /// The actual send must be done by the caller to avoid holding locks across await.
    pub fn prepare_route_data(
        &mut self,
        channel_id: ChannelId,
        payload: Vec<u8>,
    ) -> Result<(Sender<Vec<u8>>, Vec<u8>), ChannelError> {
        // Check for data-after-close
        if self.closed.contains(&channel_id) {
            return Err(ChannelError::DataAfterClose);
        }

        // Check credit before routing
        // r[impl flow.channel.credit-overrun] - Reject if exceeds credit
        let payload_len = payload.len() as u32;
        if let Some(credit) = self.incoming_credit.get_mut(&channel_id) {
            if payload_len > *credit {
                return Err(ChannelError::CreditOverrun);
            }
            // r[impl flow.channel.credit-consume] - Deduct from credit
            *credit -= payload_len;
        }
        // Note: if no credit entry exists, the stream may not be registered yet
        // (e.g., Rx stream created by callee). In that case, skip credit check.

        if let Some(tx) = self.incoming.get(&channel_id) {
            Ok((tx.clone(), payload))
        } else {
            Err(ChannelError::Unknown)
        }
    }

    /// Route a Data message payload to the appropriate incoming stream.
    ///
    /// Returns Ok(()) if routed successfully, Err(ChannelError) otherwise.
    ///
    /// r[impl channeling.data] - Data messages routed by channel_id.
    /// r[impl channeling.data-after-close] - Reject data on closed streams.
    /// r[impl flow.channel.credit-overrun] - Reject if data exceeds remaining credit.
    /// r[impl flow.channel.credit-consume] - Deduct bytes from remaining credit.
    /// r[impl flow.channel.byte-accounting] - Credit measured in payload bytes.
    pub async fn route_data(
        &mut self,
        channel_id: ChannelId,
        payload: Vec<u8>,
    ) -> Result<(), ChannelError> {
        let (tx, payload) = self.prepare_route_data(channel_id, payload)?;
        // If send fails, the Rx<T> was dropped - that's okay, just drop the data
        let _ = tx.send(payload).await;
        Ok(())
    }

    /// Close an incoming stream (remove from registry).
    ///
    /// Dropping the sender will cause the `Rx<T>`'s recv() to return None.
    ///
    /// r[impl channeling.close] - Close terminates the stream.
    /// r[impl flow.channel.close-exempt] - Close doesn't consume credit.
    pub fn close(&mut self, channel_id: ChannelId) {
        self.incoming.remove(&channel_id);
        self.incoming_credit.remove(&channel_id);
        self.outgoing_credit.remove(&channel_id);
        self.closed.insert(channel_id);
    }

    /// Reset a stream (remove from registry, discard credit).
    ///
    /// r[impl channeling.reset] - Reset terminates the stream abruptly.
    /// r[impl channeling.reset.credit] - Outstanding credit is lost on reset.
    pub fn reset(&mut self, channel_id: ChannelId) {
        self.incoming.remove(&channel_id);
        self.incoming_credit.remove(&channel_id);
        self.outgoing_credit.remove(&channel_id);
        self.closed.insert(channel_id);
    }

    /// Receive a Credit message - add credit for an outgoing stream.
    ///
    /// r[impl flow.channel.credit-grant] - Credit message adds to available credit.
    /// r[impl flow.channel.credit-additive] - Credit accumulates additively.
    pub fn receive_credit(&mut self, channel_id: ChannelId, bytes: u32) {
        if let Some(credit) = self.outgoing_credit.get_mut(&channel_id) {
            // r[impl flow.channel.credit-additive] - Add to existing credit
            *credit = credit.saturating_add(bytes);
        }
        // If no entry, stream may be closed or unknown - ignore
    }

    /// Check if a stream ID is registered (either incoming or outgoing credit).
    pub fn contains(&self, channel_id: ChannelId) -> bool {
        self.incoming.contains_key(&channel_id) || self.outgoing_credit.contains_key(&channel_id)
    }

    /// Check if a stream ID is registered as incoming.
    pub fn contains_incoming(&self, channel_id: ChannelId) -> bool {
        self.incoming.contains_key(&channel_id)
    }

    /// Check if a stream ID has outgoing credit registered.
    pub fn contains_outgoing(&self, channel_id: ChannelId) -> bool {
        self.outgoing_credit.contains_key(&channel_id)
    }

    /// Check if a stream has been closed.
    pub fn is_closed(&self, channel_id: ChannelId) -> bool {
        self.closed.contains(&channel_id)
    }

    /// Get the number of active outgoing streams (by credit tracking).
    pub fn outgoing_count(&self) -> usize {
        self.outgoing_credit.len()
    }

    /// Get remaining credit for an outgoing stream.
    ///
    /// Returns None if stream is not registered.
    pub fn outgoing_credit(&self, channel_id: ChannelId) -> Option<u32> {
        self.outgoing_credit.get(&channel_id).copied()
    }

    /// Get remaining credit we've granted for an incoming stream.
    ///
    /// Returns None if stream is not registered.
    pub fn incoming_credit(&self, channel_id: ChannelId) -> Option<u32> {
        self.incoming_credit.get(&channel_id).copied()
    }

    /// Bind streams in deserialized args for server-side dispatch.
    ///
    /// Walks the args using Poke reflection to find any `Rx<T>` or `Tx<T>` fields.
    /// For each stream found:
    /// - For `Rx<T>`: creates a channel, sets the receiver slot, registers for incoming data
    /// - For `Tx<T>`: sets the task_tx so send() writes directly to the wire
    ///
    /// # Example
    ///
    /// ```ignore
    /// let mut args = facet_postcard::from_slice::<(Rx<i32>, Tx<String>)>(&payload)?;
    /// registry.bind_streams(&mut args);
    /// let (input, output) = args;
    /// // ... call handler with input, output ...
    /// // When handler returns and Tx is dropped, Close is sent automatically
    /// ```
    pub fn bind_streams<T: Facet<'static>>(&mut self, args: &mut T) {
        let poke = facet::Poke::new(args);
        self.bind_streams_recursive(poke);
    }

    /// Recursively walk a Poke value looking for Rx/Tx streams to bind.
    #[allow(unsafe_code)]
    fn bind_streams_recursive(&mut self, mut poke: facet::Poke<'_, '_>) {
        use facet::Def;

        let shape = poke.shape();

        trace!(
            module_path = ?shape.module_path,
            type_identifier = shape.type_identifier,
            "bind_streams_recursive: visiting type"
        );

        // Check if this is an Rx or Tx type
        if shape.module_path == Some("roam_session") {
            if shape.type_identifier == "Rx" {
                debug!("bind_streams_recursive: found Rx, binding");
                self.bind_rx_stream(poke);
                return;
            } else if shape.type_identifier == "Tx" {
                debug!("bind_streams_recursive: found Tx, binding");
                self.bind_tx_stream(poke);
                return;
            }
        }

        // Dispatch based on the shape's definition
        match shape.def {
            Def::Scalar => {}

            // Recurse into struct/tuple fields
            _ if poke.is_struct() => {
                let mut ps = poke.into_struct().expect("is_struct was true");
                let field_count = ps.field_count();
                trace!(field_count, "bind_streams_recursive: recursing into struct");
                for i in 0..field_count {
                    if let Ok(field_poke) = ps.field(i) {
                        self.bind_streams_recursive(field_poke);
                    }
                }
            }

            // Recurse into Option<T>
            Def::Option(_) => {
                // Option is represented as an enum, use into_enum to access its value
                if let Ok(mut pe) = poke.into_enum()
                    && let Ok(Some(inner_poke)) = pe.field(0)
                {
                    self.bind_streams_recursive(inner_poke);
                }
            }

            // Recurse into list elements (e.g., Vec<Tx<T>>)
            Def::List(list_def) => {
                let len = {
                    let peek = poke.as_peek();
                    peek.into_list().map(|pl| pl.len()).unwrap_or(0)
                };
                // Get mutable access to elements via VTable (no PokeList exists)
                if let Some(get_mut_fn) = list_def.vtable.get_mut {
                    let element_shape = list_def.t;
                    let data_ptr = poke.data_mut();
                    for i in 0..len {
                        // SAFETY: We have exclusive mutable access via poke, index < len, shape is correct
                        let element_ptr = unsafe { (get_mut_fn)(data_ptr, i, element_shape) };
                        if let Some(ptr) = element_ptr {
                            // SAFETY: ptr points to a valid element with the correct shape
                            let element_poke =
                                unsafe { facet::Poke::from_raw_parts(ptr, element_shape) };
                            self.bind_streams_recursive(element_poke);
                        }
                    }
                }
            }

            // Other enum variants
            _ if poke.is_enum() => {
                if let Ok(mut pe) = poke.into_enum()
                    && let Ok(Some(variant_poke)) = pe.field(0)
                {
                    self.bind_streams_recursive(variant_poke);
                }
            }

            _ => {}
        }
    }

    /// Bind an Rx<T> stream for server-side dispatch.
    ///
    /// Server receives data from client on this stream.
    /// Creates a channel, sets the receiver slot, registers the sender for routing.
    fn bind_rx_stream(&mut self, poke: facet::Poke<'_, '_>) {
        if let Ok(mut ps) = poke.into_struct() {
            // Get the channel_id that was deserialized from the wire
            let channel_id = if let Ok(channel_id_field) = ps.field_by_name("channel_id")
                && let Ok(id_ref) = channel_id_field.get::<ChannelId>()
            {
                *id_ref
            } else {
                warn!("bind_rx_stream: could not get channel_id field");
                return;
            };

            debug!(channel_id, "bind_rx_stream: registering incoming channel");

            // Create channel and set receiver slot
            let (tx, rx) = crate::runtime::channel(RX_STREAM_BUFFER_SIZE);

            if let Ok(mut receiver_field) = ps.field_by_name("receiver")
                && let Ok(slot) = receiver_field.get_mut::<ReceiverSlot>()
            {
                slot.set(rx);
            }

            // Register for incoming data routing
            self.register_incoming(channel_id, tx);
            debug!(channel_id, "bind_rx_stream: channel registered");
        } else {
            warn!("bind_rx_stream: could not convert poke to struct");
        }
    }

    /// Bind a Tx<T> stream for server-side dispatch.
    ///
    /// Server sends data to client on this stream.
    /// Sets the conn_id and driver_tx so Tx::send() writes DriverMessage::Data to the wire.
    /// When the Tx is dropped, it sends DriverMessage::Close automatically.
    fn bind_tx_stream(&mut self, poke: facet::Poke<'_, '_>) {
        if let Ok(mut ps) = poke.into_struct() {
            // Set conn_id so Data/Close messages go to the correct virtual connection
            // r[impl core.conn.independence]
            if let Ok(mut conn_id_field) = ps.field_by_name("conn_id")
                && let Ok(id_ref) = conn_id_field.get_mut::<roam_wire::ConnectionId>()
            {
                *id_ref = self.conn_id;
            }

            // Set driver_tx so Tx::send() can write directly to the wire
            if let Ok(mut driver_tx_field) = ps.field_by_name("driver_tx")
                && let Ok(slot) = driver_tx_field.get_mut::<DriverTxSlot>()
            {
                slot.set(self.driver_tx.clone());
            }
        }
    }
}

/// Error when routing stream data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelError {
    /// Stream ID not found in registry.
    Unknown,
    /// Data received after stream was closed.
    DataAfterClose,
    /// r[impl flow.channel.credit-overrun] - Data exceeded remaining credit.
    CreditOverrun,
}

// ============================================================================
// Flow Control
// ============================================================================

/// Abstraction for stream flow control mechanism.
///
/// Different transports implement credit-based flow control differently:
/// - **Stream transports** (TCP, WebSocket): explicit `Message::Credit` on the wire
/// - **SHM**: shared atomic counters in the channel table (`ChannelEntry::granted_total`)
///
/// This trait abstracts the mechanism while `ChannelRegistry` remains the source
/// of truth for stream lifecycle (routing, ordering, existence).
///
/// r[impl flow.channel.credit-based]
/// r[impl flow.channel.all-transports]
pub trait FlowControl: Send {
    /// Called when we receive data on a channel (receiver side).
    ///
    /// The implementation may grant credit back to the sender:
    /// - Stream: queue a `Message::Credit` to send
    /// - SHM: increment `ChannelEntry::granted_total` atomically
    ///
    /// r[impl flow.channel.credit-grant]
    fn on_data_received(&mut self, channel_id: ChannelId, bytes: u32);

    /// Wait until we have enough credit to send `bytes` on a channel (sender side).
    ///
    /// - Stream: check `ChannelRegistry::outgoing_credit`, wait on notify if insufficient
    /// - SHM: poll/futex wait on `granted_total - sent_total >= bytes`
    ///
    /// Returns `Ok(())` when credit is available, `Err` if the channel is closed/invalid.
    ///
    /// r[impl flow.channel.zero-credit]
    fn wait_for_send_credit(
        &mut self,
        channel_id: ChannelId,
        bytes: u32,
    ) -> impl std::future::Future<Output = std::io::Result<()>> + Send;

    /// Consume credit after sending data (sender side).
    ///
    /// Called after successfully sending `bytes` on a channel.
    /// - Stream: decrement `ChannelRegistry::outgoing_credit`
    /// - SHM: increment local `sent_total`
    ///
    /// r[impl flow.channel.credit-consume]
    fn consume_send_credit(&mut self, channel_id: ChannelId, bytes: u32);
}

/// No-op flow control for infinite credit mode.
///
/// r[impl flow.channel.infinite-credit]
///
/// Used when flow control is disabled or not yet implemented.
/// All operations succeed immediately without tracking.
#[derive(Debug, Clone, Copy, Default)]
pub struct InfiniteCredit;

impl FlowControl for InfiniteCredit {
    fn on_data_received(&mut self, _channel_id: ChannelId, _bytes: u32) {
        // No credit tracking needed
    }

    async fn wait_for_send_credit(
        &mut self,
        _channel_id: ChannelId,
        _bytes: u32,
    ) -> std::io::Result<()> {
        // Infinite credit - always available
        Ok(())
    }

    fn consume_send_credit(&mut self, _channel_id: ChannelId, _bytes: u32) {
        // No credit tracking needed
    }
}

// ============================================================================
// Request ID generation
// ============================================================================

/// Generates unique request IDs for a connection.
///
/// r[impl call.request-id.uniqueness] - monotonically increasing counter starting at 1
pub struct RequestIdGenerator {
    next: AtomicU64,
}

impl RequestIdGenerator {
    /// Create a new generator starting at 1.
    pub fn new() -> Self {
        Self {
            next: AtomicU64::new(1),
        }
    }

    /// Generate the next unique request ID.
    pub fn next(&self) -> u64 {
        self.next.fetch_add(1, Ordering::Relaxed)
    }
}

impl Default for RequestIdGenerator {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Dispatch Helper
// ============================================================================

/// Helper for dispatching RPC methods with minimal generated code.
///
/// This function handles the common dispatch pattern:
/// 1. Deserialize args from payload
/// 2. Bind any Tx/Rx streams via registry
/// 3. Call the handler closure
/// 4. Encode the result and send Response
///
/// The generated code just needs to provide a closure that calls the handler method.
///
/// # Type Parameters
///
/// - `A`: Args tuple type (must implement Facet for deserialization)
/// - `R`: Result ok type (must implement Facet for serialization)
/// - `E`: User error type (must implement Facet for serialization)
/// - `F`: Handler closure type
/// - `Fut`: Future returned by handler
///
/// # Example
///
/// ```ignore
/// fn dispatch_echo(&self, payload: Vec<u8>, request_id: u64, registry: &mut ChannelRegistry)
///     -> Pin<Box<dyn Future<Output = ()> + Send + 'static>>
/// {
///     let handler = self.handler.clone();
///     dispatch_call(payload, request_id, registry, move |args: (String,)| async move {
///         handler.echo(args.0).await
///     })
/// }
/// ```
///
/// The handler returns `Result<R, E>` - user errors are automatically wrapped
/// in `RoamError::User(e)` for wire serialization.
///
/// The `channels` parameter contains channel IDs from the Request message framing.
/// These are patched into the deserialized args before binding streams.
pub fn dispatch_call<A, R, E, F, Fut>(
    cx: &Context,
    payload: Vec<u8>,
    registry: &mut ChannelRegistry,
    handler: F,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'static>>
where
    A: Facet<'static> + Send,
    R: Facet<'static> + Send,
    E: Facet<'static> + Send,
    F: FnOnce(A) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = Result<R, E>> + Send + 'static,
{
    let conn_id = cx.conn_id;
    let request_id = cx.request_id.raw();
    let channels = &cx.channels;

    // Deserialize args
    let mut args: A = match facet_postcard::from_slice(&payload) {
        Ok(args) => args,
        Err(_) => {
            let task_tx = registry.driver_tx();
            return Box::pin(async move {
                // InvalidPayload error: Result::Err(1) + RoamError::InvalidPayload(2)
                let _ = task_tx
                    .send(DriverMessage::Response {
                        conn_id,
                        request_id,
                        channels: Vec::new(),
                        payload: vec![1, 2],
                    })
                    .await;
            });
        }
    };

    // Patch channel IDs from Request framing into deserialized args
    debug!(channels = ?channels, "dispatch_call: patching channel IDs");
    patch_channel_ids(&mut args, channels);

    // Bind streams via reflection - THIS MUST HAPPEN SYNCHRONOUSLY
    debug!("dispatch_call: binding streams SYNC");
    registry.bind_streams(&mut args);
    debug!("dispatch_call: streams bound SYNC - channels should now be registered");

    let task_tx = registry.driver_tx();
    let dispatch_ctx = registry.dispatch_context();

    // Use task_local scope so roam::channel() creates bound channels.
    // This is critical: unlike thread_local, task_local won't leak to other
    // tasks that happen to run on the same worker thread.
    Box::pin(DISPATCH_CONTEXT.scope(dispatch_ctx, async move {
        debug!("dispatch_call: handler ASYNC starting");
        let result = handler(args).await;
        debug!("dispatch_call: handler ASYNC finished");
        let (payload, response_channels) = match result {
            Ok(ref ok_result) => {
                // Collect channel IDs from the result (e.g., Rx<T> in return type)
                let channels = collect_channel_ids(ok_result);
                // Result::Ok(0) + serialized value
                let mut out = vec![0u8];
                match facet_postcard::to_vec(ok_result) {
                    Ok(bytes) => out.extend(bytes),
                    Err(_) => return,
                }
                (out, channels)
            }
            Err(user_error) => {
                // Result::Err(1) + RoamError::User(0) + serialized user error
                let mut out = vec![1u8, 0u8];
                match facet_postcard::to_vec(&user_error) {
                    Ok(bytes) => out.extend(bytes),
                    Err(_) => return,
                }
                (out, Vec::new())
            }
        };

        // Send Response with channel IDs for any Rx<T> in the result.
        // ForwardingDispatcher uses these to set up Data forwarding.
        let _ = task_tx
            .send(DriverMessage::Response {
                conn_id,
                request_id,
                channels: response_channels,
                payload,
            })
            .await;
    }))
}

/// Dispatch helper for infallible methods (those that return `T` instead of `Result<T, E>`).
///
/// Same as `dispatch_call` but for handlers that cannot fail at the application level.
pub fn dispatch_call_infallible<A, R, F, Fut>(
    cx: &Context,
    payload: Vec<u8>,
    registry: &mut ChannelRegistry,
    handler: F,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'static>>
where
    A: Facet<'static> + Send,
    R: Facet<'static> + Send,
    F: FnOnce(A) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = R> + Send + 'static,
{
    let conn_id = cx.conn_id;
    let request_id = cx.request_id.raw();
    let channels = &cx.channels;

    // Deserialize args
    let mut args: A = match facet_postcard::from_slice(&payload) {
        Ok(args) => args,
        Err(_) => {
            let task_tx = registry.driver_tx();
            return Box::pin(async move {
                // InvalidPayload error: Result::Err(1) + RoamError::InvalidPayload(2)
                let _ = task_tx
                    .send(DriverMessage::Response {
                        conn_id,
                        request_id,
                        channels: Vec::new(),
                        payload: vec![1, 2],
                    })
                    .await;
            });
        }
    };

    // Patch channel IDs from Request framing into deserialized args
    patch_channel_ids(&mut args, channels);

    // Bind streams via reflection
    registry.bind_streams(&mut args);

    let task_tx = registry.driver_tx();
    let dispatch_ctx = registry.dispatch_context();

    // Use task_local scope so roam::channel() creates bound channels.
    Box::pin(DISPATCH_CONTEXT.scope(dispatch_ctx, async move {
        let result = handler(args).await;

        // Collect channel IDs from the result (e.g., Rx<T> in return type)
        let response_channels = collect_channel_ids(&result);
        if !response_channels.is_empty() {
            debug!(
                channels = ?response_channels,
                "dispatch_call_infallible: collected response channels"
            );
        }

        // Result::Ok(0) + serialized value
        let mut payload = vec![0u8];
        match facet_postcard::to_vec(&result) {
            Ok(bytes) => payload.extend(bytes),
            Err(_) => return,
        }

        // Send Response with channel IDs for any Rx<T> in the result.
        // ForwardingDispatcher uses these to set up Data forwarding.
        let _ = task_tx
            .send(DriverMessage::Response {
                conn_id,
                request_id,
                channels: response_channels,
                payload,
            })
            .await;
    }))
}

/// Send an "unknown method" error response.
///
/// Used by dispatchers when the method_id doesn't match any known method.
pub fn dispatch_unknown_method(
    cx: &Context,
    registry: &mut ChannelRegistry,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'static>> {
    let conn_id = cx.conn_id;
    let request_id = cx.request_id.raw();
    let task_tx = registry.driver_tx();
    Box::pin(async move {
        // UnknownMethod error
        let _ = task_tx
            .send(DriverMessage::Response {
                conn_id,
                request_id,
                channels: Vec::new(),
                payload: vec![1, 1],
            })
            .await;
    })
}

/// Collect channel IDs from args by walking with Peek.
///
/// Returns channel IDs in declaration order (depth-first traversal).
/// Used by the client to populate the `channels` vec in Request messages.
///
/// r[impl call.request.channels] - Collects channel IDs in declaration order for the Request.
pub fn collect_channel_ids<T: Facet<'static>>(args: &T) -> Vec<u64> {
    let mut ids = Vec::new();
    let poke = facet::Peek::new(args);
    collect_channel_ids_recursive(poke, &mut ids);
    ids
}

fn collect_channel_ids_recursive(peek: facet::Peek<'_, '_>, ids: &mut Vec<u64>) {
    let shape = peek.shape();

    // Check if this is an Rx or Tx type
    if shape.module_path == Some("roam_session")
        && (shape.type_identifier == "Rx" || shape.type_identifier == "Tx")
    {
        // Read the channel_id field
        if let Ok(ps) = peek.into_struct()
            && let Ok(channel_id_field) = ps.field_by_name("channel_id")
            && let Ok(&channel_id) = channel_id_field.get::<ChannelId>()
        {
            ids.push(channel_id);
        }
        return;
    }

    // Recurse into struct/tuple fields
    if let Ok(ps) = peek.into_struct() {
        let field_count = ps.field_count();
        for i in 0..field_count {
            if let Ok(field_peek) = ps.field(i) {
                collect_channel_ids_recursive(field_peek, ids);
            }
        }
        return;
    }

    // Recurse into Option<T> (specialized handling)
    if let Ok(po) = peek.into_option() {
        if let Some(inner) = po.value() {
            collect_channel_ids_recursive(inner, ids);
        }
        return;
    }

    // Recurse into enum variants (for other enums with data)
    if let Ok(pe) = peek.into_enum() {
        // Try to get the first field of the active variant (e.g., Some(T) has one field)
        if let Ok(Some(variant_peek)) = pe.field(0) {
            collect_channel_ids_recursive(variant_peek, ids);
        }
        return;
    }

    // Recurse into sequences (e.g., Vec<Tx<T>>)
    if let Ok(pl) = peek.into_list() {
        for element in pl.iter() {
            collect_channel_ids_recursive(element, ids);
        }
    }
}

/// Patch channel IDs into deserialized args by walking with Poke.
///
/// Overwrites channel_id fields in Rx/Tx in declaration order.
/// Used by the server to apply the authoritative `channels` vec from Request.
pub fn patch_channel_ids<T: Facet<'static>>(args: &mut T, channels: &[u64]) {
    debug!(channels = ?channels, "patch_channel_ids: patching channels from wire");
    let mut idx = 0;
    let poke = facet::Poke::new(args);
    patch_channel_ids_recursive(poke, channels, &mut idx);
}

#[allow(unsafe_code)]
fn patch_channel_ids_recursive(mut poke: facet::Poke<'_, '_>, channels: &[u64], idx: &mut usize) {
    use facet::Def;

    let shape = poke.shape();

    // Check if this is an Rx or Tx type
    if shape.module_path == Some("roam_session")
        && (shape.type_identifier == "Rx" || shape.type_identifier == "Tx")
    {
        // Overwrite the channel_id field
        if let Ok(mut ps) = poke.into_struct()
            && let Ok(mut channel_id_field) = ps.field_by_name("channel_id")
            && let Ok(channel_id_ref) = channel_id_field.get_mut::<ChannelId>()
            && *idx < channels.len()
        {
            *channel_id_ref = channels[*idx];
            *idx += 1;
        }
        return;
    }

    // Dispatch based on the shape's definition
    match shape.def {
        Def::Scalar => {}

        // Recurse into struct/tuple fields
        _ if poke.is_struct() => {
            let mut ps = poke.into_struct().expect("is_struct was true");
            let field_count = ps.field_count();
            for i in 0..field_count {
                if let Ok(field_poke) = ps.field(i) {
                    patch_channel_ids_recursive(field_poke, channels, idx);
                }
            }
        }

        // Recurse into Option<T>
        Def::Option(_) => {
            // Option is represented as an enum, use into_enum to access its value
            if let Ok(mut pe) = poke.into_enum()
                && let Ok(Some(inner_poke)) = pe.field(0)
            {
                patch_channel_ids_recursive(inner_poke, channels, idx);
            }
        }

        // Recurse into list elements (e.g., Vec<Tx<T>>)
        Def::List(list_def) => {
            let len = {
                let peek = poke.as_peek();
                peek.into_list().map(|pl| pl.len()).unwrap_or(0)
            };
            // Get mutable access to elements via VTable (no PokeList exists)
            if let Some(get_mut_fn) = list_def.vtable.get_mut {
                let element_shape = list_def.t;
                let data_ptr = poke.data_mut();
                for i in 0..len {
                    // SAFETY: We have exclusive mutable access via poke, index < len, shape is correct
                    let element_ptr = unsafe { (get_mut_fn)(data_ptr, i, element_shape) };
                    if let Some(ptr) = element_ptr {
                        // SAFETY: ptr points to a valid element with the correct shape
                        let element_poke =
                            unsafe { facet::Poke::from_raw_parts(ptr, element_shape) };
                        patch_channel_ids_recursive(element_poke, channels, idx);
                    }
                }
            }
        }

        // Other enum variants
        _ if poke.is_enum() => {
            if let Ok(mut pe) = poke.into_enum()
                && let Ok(Some(variant_poke)) = pe.field(0)
            {
                patch_channel_ids_recursive(variant_poke, channels, idx);
            }
        }

        _ => {}
    }
}

// ============================================================================
// Service Dispatcher
// ============================================================================

/// Context passed to service method implementations.
///
/// Contains information about the request that may be useful to the handler:
/// - `conn_id`: Which virtual connection the request came from
/// - `metadata`: Key-value pairs sent with the request
///
/// This enables services to identify callers and access per-request metadata.
#[derive(Debug, Clone)]
pub struct Context {
    /// The connection ID this request arrived on.
    ///
    /// For virtual connections, this identifies which specific connection
    /// the request came from, enabling bidirectional communication.
    pub conn_id: roam_wire::ConnectionId,

    /// The request ID for this call.
    ///
    /// Unique within the connection; used for response routing and cancellation.
    pub request_id: roam_wire::RequestId,

    /// The method ID being called.
    pub method_id: roam_wire::MethodId,

    /// Metadata sent with the request.
    ///
    /// This is the `metadata` field from the wire `Request` message.
    pub metadata: roam_wire::Metadata,

    /// Channel IDs from the request, in argument declaration order.
    ///
    /// Used for stream binding. Proxies can use this to remap channel IDs.
    pub channels: Vec<u64>,
}

impl Context {
    /// Create a new context.
    pub fn new(
        conn_id: roam_wire::ConnectionId,
        request_id: roam_wire::RequestId,
        method_id: roam_wire::MethodId,
        metadata: roam_wire::Metadata,
        channels: Vec<u64>,
    ) -> Self {
        Self {
            conn_id,
            request_id,
            method_id,
            metadata,
            channels,
        }
    }

    /// Get the connection ID.
    pub fn conn_id(&self) -> roam_wire::ConnectionId {
        self.conn_id
    }

    /// Get the request ID.
    pub fn request_id(&self) -> roam_wire::RequestId {
        self.request_id
    }

    /// Get the method ID.
    pub fn method_id(&self) -> roam_wire::MethodId {
        self.method_id
    }

    /// Get the request metadata.
    pub fn metadata(&self) -> &roam_wire::Metadata {
        &self.metadata
    }

    /// Get the channel IDs.
    pub fn channels(&self) -> &[u64] {
        &self.channels
    }
}

/// Trait for dispatching requests to a service.
///
/// The dispatcher handles both simple and channeling methods uniformly.
/// Stream binding is done via reflection (Poke) on the deserialized args.
pub trait ServiceDispatcher: Send + Sync {
    /// Returns the method IDs this dispatcher handles.
    ///
    /// Used by [`RoutedDispatcher`] to determine which methods to route
    /// to which dispatcher.
    fn method_ids(&self) -> Vec<u64>;

    /// Dispatch a request and send the response via the task channel.
    ///
    /// The dispatcher is responsible for:
    /// - Looking up the method by `cx.method_id()`
    /// - Deserializing arguments from payload
    /// - Patching channel IDs from `cx.channels()` into deserialized args via `patch_channel_ids()`
    /// - Binding any Tx/Rx streams via the registry
    /// - Calling the service method
    /// - Sending Data/Close messages for any Tx streams
    /// - Sending the Response message via DriverMessage::Response
    ///
    /// By using a single channel for Data/Close/Response, correct ordering is guaranteed:
    /// all stream Data and Close messages are sent before the Response.
    ///
    /// The `cx.channels()` contains channel IDs from the Request message framing,
    /// in declaration order. For a ForwardingDispatcher, this enables transparent proxying
    /// without parsing the payload.
    ///
    /// Returns a boxed future with `'static` lifetime so it can be spawned.
    /// Implementations should clone their service into the future to achieve this.
    ///
    /// r[impl channeling.allocation.caller] - Stream IDs are from Request.channels (caller allocated).
    fn dispatch(
        &self,
        cx: &Context,
        payload: Vec<u8>,
        registry: &mut ChannelRegistry,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'static>>;
}

/// A dispatcher that routes to one of two dispatchers based on method ID.
///
/// Methods handled by `primary` (via [`ServiceDispatcher::method_ids`]) are
/// routed to it; all other methods are routed to `fallback`.
pub struct RoutedDispatcher<A, B> {
    primary: A,
    fallback: B,
    primary_methods: Vec<u64>,
}

impl<A, B> RoutedDispatcher<A, B>
where
    A: ServiceDispatcher,
{
    /// Create a new routed dispatcher.
    ///
    /// Methods declared by `primary.method_ids()` are routed to `primary`,
    /// all others to `fallback`.
    pub fn new(primary: A, fallback: B) -> Self {
        let primary_methods = primary.method_ids();
        Self {
            primary,
            fallback,
            primary_methods,
        }
    }
}

impl<A, B> ServiceDispatcher for RoutedDispatcher<A, B>
where
    A: ServiceDispatcher,
    B: ServiceDispatcher,
{
    fn method_ids(&self) -> Vec<u64> {
        let mut ids = self.primary_methods.clone();
        ids.extend(self.fallback.method_ids());
        ids
    }

    fn dispatch(
        &self,
        cx: &Context,
        payload: Vec<u8>,
        registry: &mut ChannelRegistry,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'static>> {
        if self.primary_methods.contains(&cx.method_id().raw()) {
            self.primary.dispatch(cx, payload, registry)
        } else {
            self.fallback.dispatch(cx, payload, registry)
        }
    }
}

// ============================================================================
// ForwardingDispatcher - Transparent RPC Proxy
// ============================================================================

/// A dispatcher that forwards all requests to an upstream connection.
///
/// This enables transparent proxying without knowing the service schema.
/// Channel IDs are remapped automatically: the proxy allocates new channel IDs
/// for the upstream connection and maintains bidirectional forwarding.
///
/// # Example
///
/// ```ignore
/// use roam_session::{ForwardingDispatcher, ConnectionHandle};
///
/// // Upstream connection to the actual service
/// let upstream: ConnectionHandle = /* ... */;
///
/// // Create a forwarding dispatcher
/// let proxy = ForwardingDispatcher::new(upstream);
///
/// // Use with accept() - all calls will be forwarded to upstream
/// let (handle, driver) = accept(stream, config, proxy).await?;
/// ```
pub struct ForwardingDispatcher {
    upstream: ConnectionHandle,
}

impl ForwardingDispatcher {
    /// Create a new forwarding dispatcher that proxies to the upstream connection.
    pub fn new(upstream: ConnectionHandle) -> Self {
        Self { upstream }
    }
}

impl Clone for ForwardingDispatcher {
    fn clone(&self) -> Self {
        Self {
            upstream: self.upstream.clone(),
        }
    }
}

impl ServiceDispatcher for ForwardingDispatcher {
    /// Returns empty - this dispatcher accepts all method IDs.
    fn method_ids(&self) -> Vec<u64> {
        vec![]
    }

    fn dispatch(
        &self,
        cx: &Context,
        payload: Vec<u8>,
        registry: &mut ChannelRegistry,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'static>> {
        let task_tx = registry.driver_tx();
        let upstream = self.upstream.clone();
        let conn_id = cx.conn_id;
        let method_id = cx.method_id.raw();
        let request_id = cx.request_id.raw();
        let channels = cx.channels.clone();

        if channels.is_empty() {
            // Unary call - but response may contain Rx<T> channels
            // We need to set up forwarding for any response channels.
            //
            // IMPORTANT: Upstream and downstream use different channel ID spaces.
            // The upstream channel IDs must be remapped to downstream channel IDs.
            let downstream_channel_ids = registry.response_channel_ids();

            Box::pin(async move {
                let response = upstream
                    .call_raw_with_channels(method_id, vec![], payload, None)
                    .await;

                let (response_payload, upstream_response_channels) = match response {
                    Ok(data) => (data.payload, data.channels),
                    Err(TransportError::Encode(_)) => {
                        // Should not happen for raw call
                        (vec![1, 2], Vec::new()) // Err(InvalidPayload)
                    }
                    Err(TransportError::ConnectionClosed) | Err(TransportError::DriverGone) => {
                        // Connection to upstream failed - return Cancelled
                        (vec![1, 3], Vec::new()) // Err(Cancelled)
                    }
                };

                // If response has channels (e.g., method returns Rx<T>),
                // set up forwarding for Data from upstream to downstream.
                // We allocate new downstream channel IDs and remap when forwarding.
                let mut downstream_channels = Vec::new();
                if !upstream_response_channels.is_empty() {
                    debug!(
                        upstream_channels = ?upstream_response_channels,
                        "ForwardingDispatcher: setting up response channel forwarding"
                    );
                    for &upstream_id in &upstream_response_channels {
                        // Allocate a downstream channel ID
                        let downstream_id = downstream_channel_ids.next();
                        downstream_channels.push(downstream_id);

                        debug!(
                            upstream_id,
                            downstream_id, "ForwardingDispatcher: mapping channel IDs"
                        );

                        // Set up forwarding: upstream → downstream
                        let (tx, mut rx) = crate::runtime::channel::<Vec<u8>>(64);
                        upstream.register_incoming(upstream_id, tx);

                        let task_tx_clone = task_tx.clone();
                        crate::runtime::spawn(async move {
                            debug!(
                                upstream_id,
                                downstream_id, "ForwardingDispatcher: forwarding task started"
                            );
                            while let Some(data) = rx.recv().await {
                                debug!(
                                    upstream_id,
                                    downstream_id,
                                    data_len = data.len(),
                                    "ForwardingDispatcher: forwarding data"
                                );
                                let _ = task_tx_clone
                                    .send(DriverMessage::Data {
                                        conn_id,
                                        channel_id: downstream_id,
                                        payload: data,
                                    })
                                    .await;
                            }
                            debug!(
                                upstream_id,
                                downstream_id,
                                "ForwardingDispatcher: forwarding task ended, sending Close"
                            );
                            // Channel closed
                            let _ = task_tx_clone
                                .send(DriverMessage::Close {
                                    conn_id,
                                    channel_id: downstream_id,
                                })
                                .await;
                        });
                    }
                }

                let _ = task_tx
                    .send(DriverMessage::Response {
                        conn_id,
                        request_id,
                        channels: downstream_channels,
                        payload: response_payload,
                    })
                    .await;
            })
        } else {
            // Streaming call - set up bidirectional channel forwarding
            //
            // IMPORTANT: We must send the upstream Request BEFORE any Data is
            // forwarded, otherwise the backend will reject Data for unknown channels.
            //
            // Strategy:
            // 1. Register incoming handlers synchronously (buffers Data in mpsc channels)
            // 2. In the async block: send Request first, then spawn forwarding tasks
            //    (spawning AFTER Request is sent is safe - ordering is established)

            // Allocate upstream channel IDs and set up buffering channels
            let mut upstream_channels = Vec::with_capacity(channels.len());
            let mut ds_to_us_rxs = Vec::with_capacity(channels.len());
            let mut us_to_ds_rxs = Vec::with_capacity(channels.len());
            let mut channel_map = Vec::with_capacity(channels.len());

            let upstream_task_tx = upstream.driver_tx();

            for &downstream_id in &channels {
                let upstream_id = upstream.alloc_channel_id();
                upstream_channels.push(upstream_id);
                channel_map.push((downstream_id, upstream_id));

                // Buffer for downstream → upstream (client sends Data)
                let (ds_to_us_tx, ds_to_us_rx) = crate::runtime::channel(64);
                registry.register_incoming(downstream_id, ds_to_us_tx);
                ds_to_us_rxs.push(ds_to_us_rx);

                // Buffer for upstream → downstream (server sends Data)
                let (us_to_ds_tx, us_to_ds_rx) = crate::runtime::channel(64);
                upstream.register_incoming(upstream_id, us_to_ds_tx);
                us_to_ds_rxs.push(us_to_ds_rx);
            }

            // Everything below runs in the async block
            Box::pin(async move {
                // Send the upstream Request - this queues the Request command
                // which will be sent before any Data we forward
                let response_future =
                    upstream.call_raw_with_channels(method_id, upstream_channels, payload, None);

                // Now spawn forwarding tasks - safe because Request is queued first
                // and command_tx/task_tx are processed in order by the driver
                let upstream_conn_id = upstream.conn_id();
                for (i, mut rx) in ds_to_us_rxs.into_iter().enumerate() {
                    let upstream_id = channel_map[i].1;
                    let upstream_task_tx = upstream_task_tx.clone();
                    crate::runtime::spawn(async move {
                        while let Some(data) = rx.recv().await {
                            let _ = upstream_task_tx
                                .send(DriverMessage::Data {
                                    conn_id: upstream_conn_id,
                                    channel_id: upstream_id,
                                    payload: data,
                                })
                                .await;
                        }
                        // Channel closed
                        let _ = upstream_task_tx
                            .send(DriverMessage::Close {
                                conn_id: upstream_conn_id,
                                channel_id: upstream_id,
                            })
                            .await;
                    });
                }

                for (i, mut rx) in us_to_ds_rxs.into_iter().enumerate() {
                    let downstream_id = channel_map[i].0;
                    let task_tx = task_tx.clone();
                    crate::runtime::spawn(async move {
                        while let Some(data) = rx.recv().await {
                            let _ = task_tx
                                .send(DriverMessage::Data {
                                    conn_id,
                                    channel_id: downstream_id,
                                    payload: data,
                                })
                                .await;
                        }
                        // Channel closed
                        let _ = task_tx
                            .send(DriverMessage::Close {
                                conn_id,
                                channel_id: downstream_id,
                            })
                            .await;
                    });
                }

                // Wait for upstream response
                let response = response_future.await;

                let (response_payload, upstream_response_channels) = match response {
                    Ok(data) => (data.payload, data.channels),
                    Err(TransportError::Encode(_)) => {
                        (vec![1, 2], Vec::new()) // Err(InvalidPayload)
                    }
                    Err(TransportError::ConnectionClosed) | Err(TransportError::DriverGone) => {
                        (vec![1, 3], Vec::new()) // Err(Cancelled)
                    }
                };

                // Map upstream response channels back to downstream channel IDs.
                // The downstream client allocated the original IDs and expects them
                // in the Response, not the upstream IDs we allocated for forwarding.
                let downstream_response_channels: Vec<u64> = upstream_response_channels
                    .iter()
                    .filter_map(|&upstream_id| {
                        channel_map
                            .iter()
                            .find(|(_, us)| *us == upstream_id)
                            .map(|(ds, _)| *ds)
                    })
                    .collect();

                let _ = task_tx
                    .send(DriverMessage::Response {
                        conn_id,
                        request_id,
                        channels: downstream_response_channels,
                        payload: response_payload,
                    })
                    .await;
            })
        }
    }
}

// TODO: Remove this shim once facet implements `Facet` for `core::convert::Infallible`
// and for the never type `!` (facet-rs/facet#1668), then use `Infallible`.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct Never;

/// Call error type encoded in RPC responses.
///
/// r[impl core.error.roam-error] - Wraps call results to distinguish app vs protocol errors
/// r[impl call.response.encoding] - Response is `Result<T, RoamError<E>>`
/// r[impl call.error.roam-error] - Protocol errors use RoamError variants
/// r[impl call.error.protocol] - Discriminants 1-3 are protocol-level errors
///
/// Spec: `docs/content/spec/_index.md` "RoamError".
#[repr(u8)]
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub enum RoamError<E> {
    /// r[impl core.error.call-vs-connection] - User errors affect only this call
    /// r[impl call.error.user] - User(E) carries the application's error type
    User(E) = 0,
    /// r[impl call.error.unknown-method] - Method ID not recognized
    UnknownMethod = 1,
    /// r[impl call.error.invalid-payload] - Request payload deserialization failed
    InvalidPayload = 2,
    Cancelled = 3,
}

impl<E> RoamError<E> {
    /// Map the user error type to a different type.
    pub fn map_user<F, E2>(self, f: F) -> RoamError<E2>
    where
        F: FnOnce(E) -> E2,
    {
        match self {
            RoamError::User(e) => RoamError::User(f(e)),
            RoamError::UnknownMethod => RoamError::UnknownMethod,
            RoamError::InvalidPayload => RoamError::InvalidPayload,
            RoamError::Cancelled => RoamError::Cancelled,
        }
    }
}

pub type CallResult<T, E> = ::core::result::Result<T, RoamError<E>>;
pub type BorrowedCallResult<T, E> = OwnedMessage<CallResult<T, E>>;

// ============================================================================
// Connection Handle (Client-side API)
// ============================================================================

/// Error from making an outgoing call.
///
/// This flattens the nested `Result<Result<T, RoamError<E>>, CallError>` pattern
/// into a single `Result<T, CallError<E>>` for better ergonomics.
///
/// The type parameter `E` represents the user's error type from fallible methods.
/// For infallible methods, use `CallError<Never>`.
#[derive(Debug)]
pub enum CallError<E = Never> {
    /// The remote returned a roam-level error (user error or protocol error).
    Roam(RoamError<E>),
    /// Failed to encode request payload.
    Encode(facet_postcard::SerializeError),
    /// Failed to decode response payload.
    Decode(facet_postcard::DeserializeError<facet_postcard::PostcardError>),
    /// Protocol-level decode error (malformed response structure).
    Protocol(DecodeError),
    /// Connection was closed before response.
    ConnectionClosed,
    /// Driver task is gone.
    DriverGone,
}

impl<E> CallError<E> {
    /// Map the user error type to a different type.
    pub fn map_user<F, E2>(self, f: F) -> CallError<E2>
    where
        F: FnOnce(E) -> E2,
    {
        match self {
            CallError::Roam(roam_err) => CallError::Roam(roam_err.map_user(f)),
            CallError::Encode(e) => CallError::Encode(e),
            CallError::Decode(e) => CallError::Decode(e),
            CallError::Protocol(e) => CallError::Protocol(e),
            CallError::ConnectionClosed => CallError::ConnectionClosed,
            CallError::DriverGone => CallError::DriverGone,
        }
    }
}

impl<E: std::fmt::Debug> std::fmt::Display for CallError<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CallError::Roam(e) => write!(f, "roam error: {e:?}"),
            CallError::Encode(e) => write!(f, "encode error: {e}"),
            CallError::Decode(e) => write!(f, "decode error: {e}"),
            CallError::Protocol(e) => write!(f, "protocol error: {e}"),
            CallError::ConnectionClosed => write!(f, "connection closed"),
            CallError::DriverGone => write!(f, "driver task stopped"),
        }
    }
}

impl<E: std::fmt::Debug> std::error::Error for CallError<E> {}

/// Transport-level call error (no user error type).
///
/// Used by the `Caller` trait which operates at the transport level
/// before response decoding.
#[derive(Debug)]
pub enum TransportError {
    /// Failed to encode request payload.
    Encode(facet_postcard::SerializeError),
    /// Connection was closed before response.
    ConnectionClosed,
    /// Driver task is gone.
    DriverGone,
}

impl<E> From<TransportError> for CallError<E> {
    fn from(e: TransportError) -> Self {
        match e {
            TransportError::Encode(e) => CallError::Encode(e),
            TransportError::ConnectionClosed => CallError::ConnectionClosed,
            TransportError::DriverGone => CallError::DriverGone,
        }
    }
}

impl std::fmt::Display for TransportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransportError::Encode(e) => write!(f, "encode error: {e}"),
            TransportError::ConnectionClosed => write!(f, "connection closed"),
            TransportError::DriverGone => write!(f, "driver task stopped"),
        }
    }
}

impl std::error::Error for TransportError {}

/// Error decoding a response payload.
#[derive(Debug)]
pub enum DecodeError {
    /// Empty response payload.
    EmptyPayload,
    /// Truncated error response.
    TruncatedError,
    /// Unknown RoamError discriminant.
    UnknownRoamErrorDiscriminant(u8),
    /// Invalid Result discriminant.
    InvalidResultDiscriminant(u8),
    /// Postcard deserialization error.
    Postcard(facet_postcard::DeserializeError<facet_postcard::PostcardError>),
}

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DecodeError::EmptyPayload => write!(f, "empty response payload"),
            DecodeError::TruncatedError => write!(f, "truncated error response"),
            DecodeError::UnknownRoamErrorDiscriminant(d) => {
                write!(f, "unknown RoamError discriminant: {d}")
            }
            DecodeError::InvalidResultDiscriminant(d) => {
                write!(f, "invalid Result discriminant: {d}")
            }
            DecodeError::Postcard(e) => write!(f, "postcard: {e}"),
        }
    }
}

impl std::error::Error for DecodeError {}

impl<E> From<DecodeError> for CallError<E> {
    fn from(e: DecodeError) -> Self {
        match e {
            DecodeError::Postcard(pe) => CallError::Decode(pe),
            other => CallError::Protocol(other),
        }
    }
}

/// Decode a response payload into the expected type.
///
/// This is the core response decoding logic used by generated clients.
/// It handles the wire format: `[0] + value_bytes` for Ok, `[1, discriminant] + error_bytes` for Err.
///
/// Returns `Result<T, CallError<E>>` with the decoded value or error.
pub fn decode_response<T: Facet<'static>, E: Facet<'static>>(
    payload: &[u8],
) -> Result<T, CallError<E>> {
    if payload.is_empty() {
        return Err(DecodeError::EmptyPayload.into());
    }

    match payload[0] {
        0 => {
            // Ok variant: deserialize the value
            facet_postcard::from_slice(&payload[1..]).map_err(CallError::Decode)
        }
        1 => {
            // Err variant: deserialize RoamError<E>
            if payload.len() < 2 {
                return Err(DecodeError::TruncatedError.into());
            }
            let roam_error = match payload[1] {
                0 => {
                    // User error
                    let user_error: E =
                        facet_postcard::from_slice(&payload[2..]).map_err(CallError::Decode)?;
                    RoamError::User(user_error)
                }
                1 => RoamError::UnknownMethod,
                2 => RoamError::InvalidPayload,
                3 => RoamError::Cancelled,
                d => return Err(DecodeError::UnknownRoamErrorDiscriminant(d).into()),
            };
            Err(CallError::Roam(roam_error))
        }
        d => Err(DecodeError::InvalidResultDiscriminant(d).into()),
    }
}

/// Trait for making RPC calls.
///
/// This abstracts over different connection types (e.g., `ConnectionHandle`,
/// `ReconnectingClient`) so generated clients can work with any of them.
///
/// All callers return `TransportError` for transport-level failures.
/// Generated clients convert this to `CallError<E>` which also includes
/// response-level errors like `RoamError::User(E)`.
#[allow(async_fn_in_trait)]
pub trait Caller: Clone + Send + Sync + 'static {
    /// Make an RPC call with the given method ID and arguments.
    ///
    /// The arguments are mutable because stream bindings (Tx/Rx) need to be
    /// assigned channel IDs before serialization.
    ///
    /// Returns ResponseData containing the payload and any response channel IDs.
    fn call<T: Facet<'static> + Send>(
        &self,
        method_id: u64,
        args: &mut T,
    ) -> impl std::future::Future<Output = Result<ResponseData, TransportError>> + Send {
        self.call_with_metadata(method_id, args, roam_wire::Metadata::default())
    }

    /// Make an RPC call with the given method ID, arguments, and metadata.
    ///
    /// The arguments are mutable because stream bindings (Tx/Rx) need to be
    /// assigned channel IDs before serialization.
    ///
    /// Returns ResponseData containing the payload and any response channel IDs.
    fn call_with_metadata<T: Facet<'static> + Send>(
        &self,
        method_id: u64,
        args: &mut T,
        metadata: roam_wire::Metadata,
    ) -> impl std::future::Future<Output = Result<ResponseData, TransportError>> + Send;

    /// Bind receivers for `Rx<T>` streams in the response.
    ///
    /// After deserializing a response, any `Rx<T>` values in it are "hollow" -
    /// they have channel IDs but no actual receiver. This method walks the
    /// response and binds receivers for each Rx using the channel IDs from
    /// the Response message.
    fn bind_response_streams<T: Facet<'static>>(&self, response: &mut T, channels: &[u64]);
}

impl Caller for ConnectionHandle {
    async fn call_with_metadata<T: Facet<'static> + Send>(
        &self,
        method_id: u64,
        args: &mut T,
        metadata: roam_wire::Metadata,
    ) -> Result<ResponseData, TransportError> {
        ConnectionHandle::call_with_metadata(self, method_id, args, metadata).await
    }

    fn bind_response_streams<T: Facet<'static>>(&self, response: &mut T, channels: &[u64]) {
        ConnectionHandle::bind_response_streams(self, response, channels)
    }
}

// ============================================================================
// CallFuture - Builder pattern for RPC calls with optional metadata
// ============================================================================

/// A future representing an RPC call that can be configured with metadata.
///
/// This provides a builder pattern for RPC calls:
/// - `client.method(args).await` - Simple call with default (empty) metadata
/// - `client.method(args).with_metadata(meta).await` - Call with custom metadata
///
/// The future is lazy - the RPC call is not made until `.await` is called.
///
/// # Example
///
/// ```ignore
/// // Simple call
/// let result = client.subscribe(route).await?;
///
/// // With metadata
/// let result = client.subscribe(route)
///     .with_metadata(vec![("trace-id".into(), MetadataValue::String("abc".into()))])
///     .await?;
/// ```
pub struct CallFuture<C, Args, Ok, Err>
where
    C: Caller,
    Args: Facet<'static>,
{
    caller: C,
    method_id: u64,
    args: Args,
    metadata: roam_wire::Metadata,
    _phantom: PhantomData<fn() -> (Ok, Err)>,
}

impl<C, Args, Ok, Err> CallFuture<C, Args, Ok, Err>
where
    C: Caller,
    Args: Facet<'static>,
{
    /// Create a new CallFuture.
    pub fn new(caller: C, method_id: u64, args: Args) -> Self {
        Self {
            caller,
            method_id,
            args,
            metadata: roam_wire::Metadata::default(),
            _phantom: PhantomData,
        }
    }

    /// Set metadata for this call.
    ///
    /// Metadata is a list of key-value pairs that will be sent with the request.
    /// The server can access this via `Context::metadata()`.
    pub fn with_metadata(mut self, metadata: roam_wire::Metadata) -> Self {
        self.metadata = metadata;
        self
    }
}

impl<C, Args, Ok, Err> std::future::IntoFuture for CallFuture<C, Args, Ok, Err>
where
    C: Caller,
    Args: Facet<'static> + Send + 'static,
    Ok: Facet<'static> + Send + 'static,
    Err: Facet<'static> + Send + 'static,
{
    type Output = Result<Ok, CallError<Err>>;
    type IntoFuture = std::pin::Pin<Box<dyn std::future::Future<Output = Self::Output> + Send>>;

    fn into_future(self) -> Self::IntoFuture {
        let CallFuture {
            caller,
            method_id,
            mut args,
            metadata,
            _phantom,
        } = self;

        Box::pin(async move {
            let response = caller
                .call_with_metadata(method_id, &mut args, metadata)
                .await
                .map_err(CallError::from)?;
            let mut result = decode_response::<Ok, Err>(&response.payload)?;
            caller.bind_response_streams(&mut result, &response.channels);
            Ok(result)
        })
    }
}

/// Shared state between ConnectionHandle and Driver.
struct HandleShared {
    /// Connection ID for this handle (0 = root connection).
    conn_id: roam_wire::ConnectionId,
    /// Unified channel to send all messages to the driver.
    driver_tx: Sender<DriverMessage>,
    /// Request ID generator.
    request_ids: RequestIdGenerator,
    /// Stream ID allocator.
    channel_ids: ChannelIdAllocator,
    /// Stream registry for routing incoming data.
    /// Protected by a mutex since handles may create streams concurrently.
    channel_registry: std::sync::Mutex<ChannelRegistry>,
    /// Optional diagnostic state for SIGUSR1 dumps.
    diagnostic_state: Option<Arc<crate::diagnostic::DiagnosticState>>,
}

/// Handle for making outgoing RPC calls.
///
/// This is the client-side API. It can be cloned and used from multiple tasks.
/// The actual I/O is driven by the `Driver` future which must be spawned.
///
/// # Example
///
/// ```ignore
/// let (handle, driver) = establish_connection(transport, dispatcher).await?;
/// tokio::spawn(driver);
///
/// // Use handle to make calls
/// let response = handle.call_raw(method_id, payload).await?;
/// ```
#[derive(Clone)]
pub struct ConnectionHandle {
    shared: Arc<HandleShared>,
}

impl ConnectionHandle {
    /// Create a new handle for the root connection (conn_id = 0).
    ///
    /// All messages (Call/Data/Close/Response) go through a single unified channel
    /// to ensure FIFO ordering.
    pub fn new(driver_tx: Sender<DriverMessage>, role: Role, initial_credit: u32) -> Self {
        Self::new_with_diagnostics(
            roam_wire::ConnectionId::ROOT,
            driver_tx,
            role,
            initial_credit,
            None,
        )
    }

    /// Create a new handle with a specific connection ID and optional diagnostic state.
    ///
    /// If `diagnostic_state` is provided, all RPC calls and channels will be tracked
    /// for debugging purposes.
    pub fn new_with_diagnostics(
        conn_id: roam_wire::ConnectionId,
        driver_tx: Sender<DriverMessage>,
        role: Role,
        initial_credit: u32,
        diagnostic_state: Option<Arc<crate::diagnostic::DiagnosticState>>,
    ) -> Self {
        let channel_registry = ChannelRegistry::new_with_credit(initial_credit, driver_tx.clone());
        Self {
            shared: Arc::new(HandleShared {
                conn_id,
                driver_tx,
                request_ids: RequestIdGenerator::new(),
                channel_ids: ChannelIdAllocator::new(role),
                channel_registry: std::sync::Mutex::new(channel_registry),
                diagnostic_state,
            }),
        }
    }

    /// Get the connection ID for this handle.
    pub fn conn_id(&self) -> roam_wire::ConnectionId {
        self.shared.conn_id
    }

    /// Get the diagnostic state, if any.
    pub fn diagnostic_state(&self) -> Option<&Arc<crate::diagnostic::DiagnosticState>> {
        self.shared.diagnostic_state.as_ref()
    }

    /// Make a typed RPC call with automatic serialization and stream binding.
    ///
    /// Walks the args using Poke reflection to find any `Rx<T>` or `Tx<T>` fields,
    /// binds stream IDs, and sets up the stream infrastructure before serialization.
    ///
    /// # Arguments
    ///
    /// * `method_id` - The method ID to call
    /// * `args` - Arguments to serialize (typically a tuple of all method args).
    ///   Must be mutable so stream IDs can be assigned.
    ///
    /// # Stream Binding
    ///
    /// For `Rx<T>` in args (caller passes receiver, keeps sender to push data):
    /// - Allocates a stream ID
    /// - Takes the receiver and spawns a task to drain it, sending Data messages
    /// - The caller keeps the `Tx<T>` from `roam::channel()` to send values
    ///
    /// For `Tx<T>` in args (caller passes sender, keeps receiver to pull data):
    /// - Allocates a stream ID
    /// - Takes the sender and registers for incoming Data routing
    /// - The caller keeps the `Rx<T>` from `roam::channel()` to receive values
    ///
    /// # Example
    ///
    /// ```ignore
    /// // For a streaming method sum(numbers: Rx<i32>) -> i64
    /// let (tx, rx) = roam::channel::<i32>();
    /// let response = handle.call(method_id::SUM, &mut (rx,)).await?;
    /// // tx.send(&42).await to push values
    /// ```
    /// Make an RPC call with default (empty) metadata.
    pub async fn call<T: Facet<'static>>(
        &self,
        method_id: u64,
        args: &mut T,
    ) -> Result<ResponseData, TransportError> {
        self.call_with_metadata(method_id, args, roam_wire::Metadata::default())
            .await
    }

    /// Make an RPC call with custom metadata.
    pub async fn call_with_metadata<T: Facet<'static>>(
        &self,
        method_id: u64,
        args: &mut T,
        metadata: roam_wire::Metadata,
    ) -> Result<ResponseData, TransportError> {
        // Walk args and bind any streams (allocates channel IDs)
        // This collects receivers that need to be drained but does NOT spawn
        let mut drains = Vec::new();
        debug!("ConnectionHandle::call: binding streams");
        self.bind_streams(args, &mut drains);

        // Collect channel IDs for the Request message
        let channels = collect_channel_ids(args);
        debug!(
            channels = ?channels,
            drain_count = drains.len(),
            "ConnectionHandle::call: collected channels after bind_streams"
        );

        let payload = facet_postcard::to_vec(args).map_err(TransportError::Encode)?;

        // Generate args debug info for diagnostics when enabled
        let args_debug = if diagnostic::debug_enabled() {
            Some(
                facet_pretty::PrettyPrinter::new()
                    .with_colors(facet_pretty::ColorMode::Never)
                    .with_max_content_len(64)
                    .format(args),
            )
        } else {
            None
        };

        if drains.is_empty() {
            // No Rx streams - simple call
            self.call_raw_with_channels_and_metadata(
                method_id, channels, payload, args_debug, metadata,
            )
            .await
        } else {
            // Has Rx streams - spawn tasks to drain them
            // IMPORTANT: We must send Request BEFORE spawning drain tasks to ensure ordering.
            // We need to actually send the DriverMessage::Call to the driver's queue
            // before spawning drains, not just create the future.
            let request_id = self.shared.request_ids.next();
            let (response_tx, response_rx) = oneshot();

            // Track outgoing request for diagnostics
            if let Some(diag) = &self.shared.diagnostic_state {
                let args = args_debug.map(|s| {
                    let mut map = std::collections::HashMap::new();
                    map.insert("args".to_string(), s);
                    map
                });
                diag.record_outgoing_request(request_id, method_id, args);
                // Associate channels with this request
                diag.associate_channels_with_request(&channels, request_id);
            }

            let msg = DriverMessage::Call {
                conn_id: self.shared.conn_id,
                request_id,
                method_id,
                metadata,
                channels,
                payload,
                response_tx,
            };

            // Send the Call message NOW, before spawning drain tasks
            if self.shared.driver_tx.send(msg).await.is_err() {
                return Err(TransportError::DriverGone);
            }

            let task_tx = self.shared.channel_registry.lock().unwrap().driver_tx();
            let conn_id = self.shared.conn_id;

            // Spawn a task for each drain to forward data to driver
            for (channel_id, mut rx) in drains {
                let task_tx = task_tx.clone();
                crate::runtime::spawn(async move {
                    loop {
                        match rx.recv().await {
                            Some(payload) => {
                                debug!(
                                    "drain task: received {} bytes on channel {}",
                                    payload.len(),
                                    channel_id
                                );
                                // Send data to driver
                                let _ = task_tx
                                    .send(DriverMessage::Data {
                                        conn_id,
                                        channel_id,
                                        payload,
                                    })
                                    .await;
                                debug!(
                                    "drain task: sent DriverMessage::Data for channel {}",
                                    channel_id
                                );
                            }
                            None => {
                                debug!("drain task: channel {} closed", channel_id);
                                // Channel closed, send Close and exit
                                let _ = task_tx
                                    .send(DriverMessage::Close {
                                        conn_id,
                                        channel_id,
                                    })
                                    .await;
                                debug!(
                                    "drain task: sent DriverMessage::Close for channel {}",
                                    channel_id
                                );
                                break;
                            }
                        }
                    }
                });
            }

            // Just await the response - drain tasks run independently
            let result = response_rx
                .await
                .map_err(|_| TransportError::DriverGone)?
                .map_err(|_| TransportError::ConnectionClosed);

            // Mark request as complete
            if let Some(diag) = &self.shared.diagnostic_state {
                diag.complete_request(request_id);
            }

            result
        }
    }

    /// Walk args and bind any Rx<T> or Tx<T> streams.
    /// Collects (channel_id, receiver) pairs for Rx streams that need draining.
    fn bind_streams<T: Facet<'static>>(
        &self,
        args: &mut T,
        drains: &mut Vec<(ChannelId, Receiver<Vec<u8>>)>,
    ) {
        let poke = facet::Poke::new(args);
        self.bind_streams_recursive(poke, drains);
    }

    /// Recursively walk a Poke value looking for Rx/Tx streams to bind.
    #[allow(unsafe_code)]
    fn bind_streams_recursive(
        &self,
        mut poke: facet::Poke<'_, '_>,
        drains: &mut Vec<(ChannelId, Receiver<Vec<u8>>)>,
    ) {
        use facet::Def;

        let shape = poke.shape();

        // Check if this is an Rx or Tx type
        if shape.module_path == Some("roam_session") {
            if shape.type_identifier == "Rx" {
                self.bind_rx_stream(poke, drains);
                return;
            } else if shape.type_identifier == "Tx" {
                self.bind_tx_stream(poke);
                return;
            }
        }

        // Dispatch based on the shape's definition
        match shape.def {
            Def::Scalar => {}

            // Recurse into struct/tuple fields
            _ if poke.is_struct() => {
                let mut ps = poke.into_struct().expect("is_struct was true");
                let field_count = ps.field_count();
                for i in 0..field_count {
                    if let Ok(field_poke) = ps.field(i) {
                        self.bind_streams_recursive(field_poke, drains);
                    }
                }
            }

            // Recurse into Option<T>
            Def::Option(_) => {
                // Option is represented as an enum, use into_enum to access its value
                if let Ok(mut pe) = poke.into_enum()
                    && let Ok(Some(inner_poke)) = pe.field(0)
                {
                    self.bind_streams_recursive(inner_poke, drains);
                }
            }

            // Recurse into list elements (e.g., Vec<Tx<T>>)
            Def::List(list_def) => {
                let len = {
                    let peek = poke.as_peek();
                    peek.into_list().map(|pl| pl.len()).unwrap_or(0)
                };
                // Get mutable access to elements via VTable (no PokeList exists)
                if let Some(get_mut_fn) = list_def.vtable.get_mut {
                    let element_shape = list_def.t;
                    let data_ptr = poke.data_mut();
                    for i in 0..len {
                        // SAFETY: We have exclusive mutable access via poke, index < len, shape is correct
                        let element_ptr = unsafe { (get_mut_fn)(data_ptr, i, element_shape) };
                        if let Some(ptr) = element_ptr {
                            // SAFETY: ptr points to a valid element with the correct shape
                            let element_poke =
                                unsafe { facet::Poke::from_raw_parts(ptr, element_shape) };
                            self.bind_streams_recursive(element_poke, drains);
                        }
                    }
                }
            }

            // Other enum variants
            _ if poke.is_enum() => {
                if let Ok(mut pe) = poke.into_enum()
                    && let Ok(Some(variant_poke)) = pe.field(0)
                {
                    self.bind_streams_recursive(variant_poke, drains);
                }
            }

            _ => {}
        }
    }

    /// Bind an Rx<T> stream - caller passes receiver, keeps sender.
    /// Collects the receiver for draining (no spawning).
    fn bind_rx_stream(
        &self,
        poke: facet::Poke<'_, '_>,
        drains: &mut Vec<(ChannelId, Receiver<Vec<u8>>)>,
    ) {
        let channel_id = self.alloc_channel_id();
        debug!(
            channel_id,
            "OutgoingBinder::bind_rx_stream: allocated channel_id for Rx"
        );

        if let Ok(mut ps) = poke.into_struct() {
            // Set channel_id field by getting mutable access to the u64
            if let Ok(mut channel_id_field) = ps.field_by_name("channel_id")
                && let Ok(id_ref) = channel_id_field.get_mut::<ChannelId>()
            {
                debug!(
                    old_id = *id_ref,
                    new_id = channel_id,
                    "OutgoingBinder::bind_rx_stream: overwriting channel_id"
                );
                *id_ref = channel_id;
            }

            // Take the receiver from ReceiverSlot - collect for draining later
            if let Ok(mut receiver_field) = ps.field_by_name("receiver")
                && let Ok(slot) = receiver_field.get_mut::<ReceiverSlot>()
                && let Some(rx) = slot.take()
            {
                debug!(
                    channel_id,
                    "OutgoingBinder::bind_rx_stream: took receiver, adding to drains"
                );
                drains.push((channel_id, rx));
            }
        }
    }

    /// Bind a Tx<T> stream - caller passes sender, keeps receiver.
    /// We take the sender and register for incoming Data routing.
    fn bind_tx_stream(&self, poke: facet::Poke<'_, '_>) {
        let channel_id = self.alloc_channel_id();
        debug!(
            channel_id,
            "OutgoingBinder::bind_tx_stream: allocated channel_id for Tx"
        );

        if let Ok(mut ps) = poke.into_struct() {
            // Set channel_id field by getting mutable access to the u64
            if let Ok(mut channel_id_field) = ps.field_by_name("channel_id")
                && let Ok(id_ref) = channel_id_field.get_mut::<ChannelId>()
            {
                debug!(
                    old_id = *id_ref,
                    new_id = channel_id,
                    "OutgoingBinder::bind_tx_stream: overwriting channel_id"
                );
                *id_ref = channel_id;
            }

            // Take the sender from SenderSlot
            if let Ok(mut sender_field) = ps.field_by_name("sender")
                && let Ok(slot) = sender_field.get_mut::<SenderSlot>()
                && let Some(tx) = slot.take()
            {
                debug!(
                    channel_id,
                    "OutgoingBinder::bind_tx_stream: took sender, registering for incoming"
                );
                // Register for incoming Data routing
                self.register_incoming(channel_id, tx);
            }
        }
    }

    /// Make a raw RPC call with pre-serialized payload.
    ///
    /// Returns the raw response payload bytes.
    /// Note: For streaming calls, use `call()` which handles channel binding.
    pub async fn call_raw(
        &self,
        method_id: u64,
        payload: Vec<u8>,
    ) -> Result<Vec<u8>, TransportError> {
        self.call_raw_full(method_id, Vec::new(), Vec::new(), payload, None)
            .await
            .map(|r| r.payload)
    }

    /// Make a raw RPC call with pre-serialized payload and channel IDs.
    ///
    /// Used internally by `call()` after binding streams.
    /// Returns ResponseData so caller can handle response channels.
    async fn call_raw_with_channels(
        &self,
        method_id: u64,
        channels: Vec<u64>,
        payload: Vec<u8>,
        args_debug: Option<String>,
    ) -> Result<ResponseData, TransportError> {
        self.call_raw_full(method_id, Vec::new(), channels, payload, args_debug)
            .await
    }

    async fn call_raw_with_channels_and_metadata(
        &self,
        method_id: u64,
        channels: Vec<u64>,
        payload: Vec<u8>,
        args_debug: Option<String>,
        metadata: roam_wire::Metadata,
    ) -> Result<ResponseData, TransportError> {
        self.call_raw_full(method_id, metadata, channels, payload, args_debug)
            .await
    }

    /// Make a raw RPC call with pre-serialized payload and metadata.
    ///
    /// Returns the raw response payload bytes.
    pub async fn call_raw_with_metadata(
        &self,
        method_id: u64,
        payload: Vec<u8>,
        metadata: Vec<(String, roam_wire::MetadataValue)>,
    ) -> Result<Vec<u8>, TransportError> {
        self.call_raw_full(method_id, metadata, Vec::new(), payload, None)
            .await
            .map(|r| r.payload)
    }

    /// Make a raw RPC call with all options.
    ///
    /// Returns ResponseData containing the payload and any response channel IDs.
    async fn call_raw_full(
        &self,
        method_id: u64,
        metadata: Vec<(String, roam_wire::MetadataValue)>,
        channels: Vec<u64>,
        payload: Vec<u8>,
        args_debug: Option<String>,
    ) -> Result<ResponseData, TransportError> {
        let request_id = self.shared.request_ids.next();
        let (response_tx, response_rx) = oneshot();

        // Track outgoing request for diagnostics
        if let Some(diag) = &self.shared.diagnostic_state {
            let args = args_debug.map(|s| {
                let mut map = std::collections::HashMap::new();
                map.insert("args".to_string(), s);
                map
            });
            diag.record_outgoing_request(request_id, method_id, args);
            // Associate channels with this request
            diag.associate_channels_with_request(&channels, request_id);
        }

        let msg = DriverMessage::Call {
            conn_id: self.shared.conn_id,
            request_id,
            method_id,
            metadata,
            channels,
            payload,
            response_tx,
        };

        self.shared
            .driver_tx
            .send(msg)
            .await
            .map_err(|_| TransportError::DriverGone)?;

        let result = response_rx
            .await
            .map_err(|_| TransportError::DriverGone)?
            .map_err(|_| TransportError::ConnectionClosed);

        // Mark request as complete
        if let Some(diag) = &self.shared.diagnostic_state {
            diag.complete_request(request_id);
        }

        result
    }

    /// Open a new virtual connection on the link.
    ///
    /// Sends a `Connect` message to the remote peer and waits for an
    /// `Accept` or `Reject` response. Returns a new `ConnectionHandle`
    /// for the virtual connection if accepted.
    ///
    /// r[impl core.conn.open]
    ///
    /// # Arguments
    ///
    /// * `metadata` - Optional metadata to send with the Connect request
    ///   (e.g., authentication tokens, routing hints).
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Open a new virtual connection
    /// let virtual_conn = handle.connect(vec![]).await?;
    ///
    /// // Use the new connection for calls
    /// let response = virtual_conn.call_raw(method_id, payload).await?;
    /// ```
    pub async fn connect(
        &self,
        metadata: roam_wire::Metadata,
    ) -> Result<ConnectionHandle, crate::ConnectError> {
        let request_id = self.shared.request_ids.next();
        let (response_tx, response_rx) = oneshot();

        let msg = DriverMessage::Connect {
            request_id,
            metadata,
            response_tx,
        };

        self.shared.driver_tx.send(msg).await.map_err(|_| {
            crate::ConnectError::ConnectFailed(std::io::Error::other("driver gone"))
        })?;

        response_rx
            .await
            .map_err(|_| crate::ConnectError::ConnectFailed(std::io::Error::other("driver gone")))?
    }

    /// Allocate a stream ID for an outgoing stream.
    ///
    /// Used internally when binding streams during call().
    pub fn alloc_channel_id(&self) -> ChannelId {
        self.shared.channel_ids.next()
    }

    /// Allocate a unique request ID for an outgoing call.
    ///
    /// Used when manually constructing DriverMessage::Call.
    pub fn alloc_request_id(&self) -> u64 {
        self.shared.request_ids.next()
    }

    /// Register an incoming stream (we receive data from peer).
    ///
    /// Used when schema has `Tx<T>` (callee sends to caller) - we receive that data.
    pub fn register_incoming(&self, channel_id: ChannelId, tx: Sender<Vec<u8>>) {
        // Track channel for diagnostics (request_id not available here)
        if let Some(diag) = &self.shared.diagnostic_state {
            diag.record_channel_open(channel_id, crate::diagnostic::ChannelDirection::Rx, None);
        }
        self.shared
            .channel_registry
            .lock()
            .unwrap()
            .register_incoming(channel_id, tx);
    }

    /// Register credit tracking for an outgoing stream.
    ///
    /// The actual receiver is owned by the driver, not the registry.
    pub fn register_outgoing_credit(&self, channel_id: ChannelId) {
        // Track channel for diagnostics (request_id not available here)
        if let Some(diag) = &self.shared.diagnostic_state {
            diag.record_channel_open(channel_id, crate::diagnostic::ChannelDirection::Tx, None);
        }
        self.shared
            .channel_registry
            .lock()
            .unwrap()
            .register_outgoing_credit(channel_id);
    }

    /// Route incoming stream data to the appropriate Rx.
    pub async fn route_data(
        &self,
        channel_id: ChannelId,
        payload: Vec<u8>,
    ) -> Result<(), ChannelError> {
        // Get the sender while holding the lock, then release before await
        let (tx, payload) = self
            .shared
            .channel_registry
            .lock()
            .unwrap()
            .prepare_route_data(channel_id, payload)?;
        // Send without holding the lock
        let _ = tx.send(payload).await;
        Ok(())
    }

    /// Close an incoming stream.
    pub fn close_channel(&self, channel_id: ChannelId) {
        // Track channel close for diagnostics
        if let Some(diag) = &self.shared.diagnostic_state {
            diag.record_channel_close(channel_id);
        }
        self.shared
            .channel_registry
            .lock()
            .unwrap()
            .close(channel_id);
    }

    /// Reset a stream.
    pub fn reset_channel(&self, channel_id: ChannelId) {
        // Track channel close for diagnostics
        if let Some(diag) = &self.shared.diagnostic_state {
            diag.record_channel_close(channel_id);
        }
        self.shared
            .channel_registry
            .lock()
            .unwrap()
            .reset(channel_id);
    }

    /// Check if a stream exists.
    pub fn contains_channel(&self, channel_id: ChannelId) -> bool {
        self.shared
            .channel_registry
            .lock()
            .unwrap()
            .contains(channel_id)
    }

    /// Receive credit for an outgoing stream.
    pub fn receive_credit(&self, channel_id: ChannelId, bytes: u32) {
        self.shared
            .channel_registry
            .lock()
            .unwrap()
            .receive_credit(channel_id, bytes);
    }

    /// Get a clone of the driver message sender.
    ///
    /// Used for forwarding/proxy scenarios where messages need to be sent
    /// on this connection's wire.
    pub fn driver_tx(&self) -> Sender<DriverMessage> {
        self.shared.channel_registry.lock().unwrap().driver_tx()
    }

    /// Bind receivers for `Rx<T>` streams in a deserialized response.
    ///
    /// After deserializing a response, any `Rx<T>` values are "hollow" - they have
    /// channel IDs but no actual receiver. This method walks the response using
    /// reflection and binds receivers for each `Rx<T>` so data can be received.
    ///
    /// # How it works
    ///
    /// For each `Rx<T>` found in the response:
    /// 1. Read the channel_id that was set during deserialization
    /// 2. Create a new channel (tx, rx)
    /// 3. Set the receiver slot on the Rx
    /// 4. Register the sender with the channel registry for incoming data routing
    ///
    /// This mirrors server-side `ChannelRegistry::bind_streams` but for responses.
    ///
    /// IMPORTANT: The `channels` parameter contains the authoritative channel IDs
    /// from the Response framing. For forwarded connections (via ForwardingDispatcher),
    /// these IDs may differ from the IDs serialized in the payload. We patch them first.
    pub fn bind_response_streams<T: Facet<'static>>(&self, response: &mut T, channels: &[u64]) {
        // Patch channel IDs from Response.channels into the deserialized response.
        // This is critical for ForwardingDispatcher where the payload contains upstream
        // channel IDs but channels[] contains the remapped downstream IDs.
        patch_channel_ids(response, channels);

        let poke = facet::Poke::new(response);
        self.bind_response_streams_recursive(poke);
    }

    /// Recursively walk a Poke value looking for Rx streams to bind in responses.
    #[allow(unsafe_code)]
    fn bind_response_streams_recursive(&self, mut poke: facet::Poke<'_, '_>) {
        use facet::Def;

        let shape = poke.shape();

        // Check if this is an Rx type - only Rx needs binding in responses
        // (Tx in responses would be outgoing, but that's uncommon for return types)
        if shape.module_path == Some("roam_session") && shape.type_identifier == "Rx" {
            self.bind_rx_response_stream(poke);
            return;
        }

        // Dispatch based on the shape's definition
        match shape.def {
            Def::Scalar => {}

            // Recurse into struct/tuple fields
            _ if poke.is_struct() => {
                let mut ps = poke.into_struct().expect("is_struct was true");
                let field_count = ps.field_count();
                for i in 0..field_count {
                    if let Ok(field_poke) = ps.field(i) {
                        self.bind_response_streams_recursive(field_poke);
                    }
                }
            }

            // Recurse into Option<T>
            Def::Option(_) => {
                // Option is represented as an enum, use into_enum to access its value
                if let Ok(mut pe) = poke.into_enum()
                    && let Ok(Some(inner_poke)) = pe.field(0)
                {
                    self.bind_response_streams_recursive(inner_poke);
                }
            }

            // Recurse into list elements (e.g., Vec<Rx<T>>)
            Def::List(list_def) => {
                let len = {
                    let peek = poke.as_peek();
                    peek.into_list().map(|pl| pl.len()).unwrap_or(0)
                };
                // Get mutable access to elements via VTable (no PokeList exists)
                if let Some(get_mut_fn) = list_def.vtable.get_mut {
                    let element_shape = list_def.t;
                    let data_ptr = poke.data_mut();
                    for i in 0..len {
                        // SAFETY: We have exclusive mutable access via poke, index < len, shape is correct
                        let element_ptr = unsafe { (get_mut_fn)(data_ptr, i, element_shape) };
                        if let Some(ptr) = element_ptr {
                            // SAFETY: ptr points to a valid element with the correct shape
                            let element_poke =
                                unsafe { facet::Poke::from_raw_parts(ptr, element_shape) };
                            self.bind_response_streams_recursive(element_poke);
                        }
                    }
                }
            }

            // Other enum variants
            _ if poke.is_enum() => {
                if let Ok(mut pe) = poke.into_enum()
                    && let Ok(Some(variant_poke)) = pe.field(0)
                {
                    self.bind_response_streams_recursive(variant_poke);
                }
            }

            _ => {}
        }
    }

    /// Bind a single Rx<T> stream from a response.
    ///
    /// Creates a channel, sets the receiver slot, and registers for incoming data.
    fn bind_rx_response_stream(&self, poke: facet::Poke<'_, '_>) {
        if let Ok(mut ps) = poke.into_struct() {
            // Get the channel_id that was deserialized from the wire
            let channel_id = if let Ok(channel_id_field) = ps.field_by_name("channel_id")
                && let Ok(id_ref) = channel_id_field.get::<ChannelId>()
            {
                *id_ref
            } else {
                return;
            };

            // Create channel and set receiver slot
            let (tx, rx) = crate::runtime::channel(RX_STREAM_BUFFER_SIZE);

            if let Ok(mut receiver_field) = ps.field_by_name("receiver")
                && let Ok(slot) = receiver_field.get_mut::<ReceiverSlot>()
            {
                slot.set(rx);
            }

            // Register for incoming data routing
            self.register_incoming(channel_id, tx);
        }
    }
}

#[derive(Debug)]
pub enum ClientError<TransportError> {
    Transport(TransportError),
    Encode(facet_postcard::SerializeError),
    Decode(facet_postcard::DeserializeError<facet_postcard::PostcardError>),
}

impl<TransportError> From<TransportError> for ClientError<TransportError> {
    fn from(value: TransportError) -> Self {
        Self::Transport(value)
    }
}

#[derive(Debug)]
pub enum DispatchError {
    Encode(facet_postcard::SerializeError),
}

// ============================================================================
// Tunnel Adapters for AsyncRead/AsyncWrite Streams (native only)
// ============================================================================

#[cfg(not(target_arch = "wasm32"))]
use std::io;
#[cfg(not(target_arch = "wasm32"))]
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
#[cfg(not(target_arch = "wasm32"))]
use tokio::task::JoinHandle;

/// Default chunk size for tunnel pumps (32KB).
///
/// Balances throughput with memory usage and slot consumption.
/// Larger values improve throughput but use more memory per read.
/// Smaller values improve latency but increase syscall overhead.
#[cfg(not(target_arch = "wasm32"))]
pub const DEFAULT_TUNNEL_CHUNK_SIZE: usize = 32 * 1024;

/// A bidirectional byte tunnel over roam channels.
///
/// From the perspective of whoever holds the tunnel:
/// - `tx`: Send bytes TO the remote end
/// - `rx`: Receive bytes FROM the remote end
///
/// Tunnels are typically used to bridge async byte streams (TCP, Unix sockets, etc.)
/// with roam's streaming channels. One side creates a tunnel pair with [`tunnel_pair()`],
/// passes one half to the remote via an RPC call, and uses the other half locally.
///
/// # Example
///
/// ```ignore
/// // Host side: create tunnel and pump to/from a socket
/// let (local, remote) = roam_session::tunnel_pair();
/// let (read_handle, write_handle) = roam_session::tunnel_stream(socket, local, 32 * 1024);
///
/// // Pass `remote` to cell via RPC
/// cell.handle_connection(remote).await?;
/// ```
#[derive(Facet)]
pub struct Tunnel {
    /// Channel for sending bytes to the remote end.
    pub tx: Tx<Vec<u8>>,
    /// Channel for receiving bytes from the remote end.
    pub rx: Rx<Vec<u8>>,
}

/// Create a pair of connected tunnels.
///
/// Returns `(local, remote)` where:
/// - Data sent on `local.tx` arrives at `remote.rx`
/// - Data sent on `remote.tx` arrives at `local.rx`
///
/// This is useful for creating a bidirectional channel that can be split
/// across an RPC boundary. One side keeps `local` and passes `remote` to
/// the other side via an RPC call.
///
/// # Example
///
/// ```ignore
/// let (local, remote) = tunnel_pair();
///
/// // Spawn tasks to pump data from local stream
/// tunnel_stream(tcp_stream, local, DEFAULT_TUNNEL_CHUNK_SIZE);
///
/// // Send remote to the other side via RPC
/// service.handle_tunnel(remote).await?;
/// ```
pub fn tunnel_pair() -> (Tunnel, Tunnel) {
    let (tx1, rx1) = channel::<Vec<u8>>();
    let (tx2, rx2) = channel::<Vec<u8>>();
    (Tunnel { tx: tx1, rx: rx2 }, Tunnel { tx: tx2, rx: rx1 })
}

/// Pump bytes from an `AsyncRead` into a `Tx<Vec<u8>>`.
///
/// Reads chunks up to `chunk_size` bytes and sends them on the channel.
/// Returns when the reader reaches EOF or the channel closes.
///
/// # Arguments
///
/// * `reader` - Any type implementing `AsyncRead + Unpin`
/// * `tx` - The transmit channel to send bytes to
/// * `chunk_size` - Maximum bytes to read per chunk
///
/// # Returns
///
/// * `Ok(())` - Reader reached EOF, channel closed gracefully
/// * `Err(io::Error)` - Read error occurred
///
/// # Example
///
/// ```ignore
/// let (tx, rx) = roam::channel::<Vec<u8>>();
/// let result = pump_read_to_tx(reader, tx, 32 * 1024).await;
/// ```
#[cfg(not(target_arch = "wasm32"))]
pub async fn pump_read_to_tx<R: AsyncRead + Unpin>(
    mut reader: R,
    tx: Tx<Vec<u8>>,
    chunk_size: usize,
) -> io::Result<()> {
    let mut buf = vec![0u8; chunk_size];
    loop {
        let n = reader.read(&mut buf).await?;
        if n == 0 {
            // EOF - drop tx to close the channel
            break;
        }
        // Send the bytes we read
        if tx.send(&buf[..n].to_vec()).await.is_err() {
            // Channel closed by receiver - treat as graceful shutdown
            break;
        }
    }
    Ok(())
}

/// Pump bytes from an `Rx<Vec<u8>>` into an `AsyncWrite`.
///
/// Receives chunks and writes them to the writer.
/// Returns when the channel closes or a write error occurs.
///
/// # Arguments
///
/// * `rx` - The receive channel to get bytes from
/// * `writer` - Any type implementing `AsyncWrite + Unpin`
///
/// # Returns
///
/// * `Ok(())` - Channel closed gracefully
/// * `Err(io::Error)` - Write error or deserialization error occurred
///
/// # Example
///
/// ```ignore
/// let (tx, rx) = roam::channel::<Vec<u8>>();
/// let result = pump_rx_to_write(rx, writer).await;
/// ```
#[cfg(not(target_arch = "wasm32"))]
pub async fn pump_rx_to_write<W: AsyncWrite + Unpin>(
    mut rx: Rx<Vec<u8>>,
    mut writer: W,
) -> io::Result<()> {
    loop {
        match rx.recv().await {
            Ok(Some(data)) => {
                writer.write_all(&data).await?;
            }
            Ok(None) => {
                // Channel closed - flush and exit
                writer.flush().await?;
                break;
            }
            Err(e) => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("tunnel receive error: {e}"),
                ));
            }
        }
    }
    Ok(())
}

/// Tunnel a bidirectional stream through a roam Tunnel.
///
/// Spawns two tasks to pump data in both directions:
/// - One task reads from `stream` and sends to `tunnel.tx`
/// - One task receives from `tunnel.rx` and writes to `stream`
///
/// Returns handles to join on completion. Both tasks run until their
/// respective direction completes (EOF/close) or an error occurs.
///
/// # Arguments
///
/// * `stream` - Any type implementing `AsyncRead + AsyncWrite + Unpin + Send + 'static`
/// * `tunnel` - The tunnel to pump data through
/// * `chunk_size` - Maximum bytes to read per chunk (see [`DEFAULT_TUNNEL_CHUNK_SIZE`])
///
/// # Returns
///
/// A tuple of `(read_handle, write_handle)`:
/// - `read_handle` - Completes when the stream reaches EOF or tx closes
/// - `write_handle` - Completes when rx closes or stream write fails
///
/// # Example
///
/// ```ignore
/// let (local, remote) = tunnel_pair();
/// let (read_handle, write_handle) = tunnel_stream(tcp_stream, local, DEFAULT_TUNNEL_CHUNK_SIZE);
///
/// // Wait for both directions to complete
/// let _ = read_handle.await;
/// let _ = write_handle.await;
/// ```
#[cfg(not(target_arch = "wasm32"))]
pub fn tunnel_stream<S>(
    stream: S,
    tunnel: Tunnel,
    chunk_size: usize,
) -> (JoinHandle<io::Result<()>>, JoinHandle<io::Result<()>>)
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let (reader, writer) = tokio::io::split(stream);
    let Tunnel { tx, rx } = tunnel;

    let read_handle = tokio::spawn(async move { pump_read_to_tx(reader, tx, chunk_size).await });

    let write_handle = tokio::spawn(async move { pump_rx_to_write(rx, writer).await });

    (read_handle, write_handle)
}

#[cfg(test)]
mod tests {
    use super::*;

    // r[verify channeling.id.parity]
    #[test]
    fn channel_id_allocator_initiator_uses_odd_ids() {
        let alloc = ChannelIdAllocator::new(Role::Initiator);
        assert_eq!(alloc.next(), 1);
        assert_eq!(alloc.next(), 3);
        assert_eq!(alloc.next(), 5);
        assert_eq!(alloc.next(), 7);
    }

    // r[verify channeling.id.parity]
    #[test]
    fn channel_id_allocator_acceptor_uses_even_ids() {
        let alloc = ChannelIdAllocator::new(Role::Acceptor);
        assert_eq!(alloc.next(), 2);
        assert_eq!(alloc.next(), 4);
        assert_eq!(alloc.next(), 6);
        assert_eq!(alloc.next(), 8);
    }

    // r[verify channeling.holder-semantics]
    #[tokio::test]
    async fn tx_serializes_and_rx_deserializes() {
        // Create a channel pair using roam::channel
        let (tx, mut rx) = channel::<i32>();

        // Simulate what ConnectionHandle::call would do: take the receiver
        let mut taken_rx = rx.receiver.take().expect("receiver should be present");

        // Now tx can send and we can receive on the taken receiver
        tx.send(&100).await.unwrap();
        tx.send(&200).await.unwrap();

        // Receive raw bytes and deserialize
        let bytes1 = taken_rx.recv().await.unwrap();
        let val1: i32 = facet_postcard::from_slice(&bytes1).unwrap();
        assert_eq!(val1, 100);

        let bytes2 = taken_rx.recv().await.unwrap();
        let val2: i32 = facet_postcard::from_slice(&bytes2).unwrap();
        assert_eq!(val2, 200);
    }

    /// Create a test registry with a dummy task channel.
    fn test_registry() -> ChannelRegistry {
        let (task_tx, _task_rx) = crate::runtime::channel(10);
        ChannelRegistry::new(task_tx)
    }

    // r[verify channeling.data-after-close]
    #[tokio::test]
    async fn data_after_close_is_rejected() {
        let mut registry = test_registry();
        let (tx, _rx) = crate::runtime::channel(10);
        registry.register_incoming(42, tx);

        // Close the stream
        registry.close(42);

        // Data after close should fail
        let result = registry.route_data(42, b"data".to_vec()).await;
        assert_eq!(result, Err(ChannelError::DataAfterClose));
    }

    // r[verify channeling.data]
    // r[verify channeling.unknown]
    #[tokio::test]
    async fn channel_registry_routes_data_to_registered_stream() {
        let mut registry = test_registry();

        // Register a stream
        let (tx, mut rx) = crate::runtime::channel(10);
        registry.register_incoming(42, tx);

        // Data to registered stream should succeed
        assert!(registry.route_data(42, b"hello".to_vec()).await.is_ok());

        // Should receive the data
        assert_eq!(rx.recv().await, Some(b"hello".to_vec()));

        // Data to unregistered stream should fail
        assert!(registry.route_data(999, b"nope".to_vec()).await.is_err());
    }

    // r[verify channeling.close]
    #[tokio::test]
    async fn channel_registry_close_terminates_stream() {
        let mut registry = test_registry();
        let (tx, mut rx) = crate::runtime::channel(10);
        registry.register_incoming(42, tx);

        // Send some data
        registry.route_data(42, b"data1".to_vec()).await.unwrap();

        // Close the stream
        registry.close(42);

        // Should still receive buffered data
        assert_eq!(rx.recv().await, Some(b"data1".to_vec()));

        // Then channel closes (sender dropped)
        assert_eq!(rx.recv().await, None);

        // Stream no longer registered
        assert!(!registry.contains(42));
    }

    #[test]
    fn tx_rx_shape_metadata() {
        use facet::Facet;

        let tx_shape = <Tx<i32> as Facet>::SHAPE;
        let rx_shape = <Rx<i32> as Facet>::SHAPE;

        // Verify module_path and type_identifier are set correctly
        assert_eq!(tx_shape.module_path, Some("roam_session"));
        assert_eq!(tx_shape.type_identifier, "Tx");
        assert_eq!(rx_shape.module_path, Some("roam_session"));
        assert_eq!(rx_shape.type_identifier, "Rx");

        // Verify type_params are populated
        assert_eq!(tx_shape.type_params.len(), 1);
        assert_eq!(rx_shape.type_params.len(), 1);
    }

    // ========================================================================
    // Tunnel Tests
    // ========================================================================

    #[tokio::test]
    async fn tunnel_pair_connects_bidirectionally() {
        let (local, remote) = tunnel_pair();

        // Send from local to remote
        local.tx.send(&b"hello".to_vec()).await.unwrap();

        // Receive on remote
        let mut remote_rx = remote.rx;
        let received = remote_rx.recv().await.unwrap().unwrap();
        assert_eq!(received, b"hello".to_vec());

        // Send from remote to local
        remote.tx.send(&b"world".to_vec()).await.unwrap();

        // Receive on local
        let mut local_rx = local.rx;
        let received = local_rx.recv().await.unwrap().unwrap();
        assert_eq!(received, b"world".to_vec());
    }

    #[tokio::test]
    async fn pump_read_to_tx_sends_chunks() {
        use std::io::Cursor;

        let data = b"hello world this is a test message";
        let reader = Cursor::new(data.to_vec());
        let (tx, mut rx) = channel::<Vec<u8>>();

        // Pump with small chunk size to force multiple chunks
        let handle = tokio::spawn(async move { pump_read_to_tx(reader, tx, 10).await });

        // Collect all received chunks
        let mut received = Vec::new();
        while let Ok(Some(chunk)) = rx.recv().await {
            received.extend(chunk);
        }

        // Verify we got all the data
        assert_eq!(received, data.to_vec());

        // Pump should complete successfully
        handle.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn pump_rx_to_write_writes_chunks() {
        use std::io::Cursor;

        let (tx, rx) = channel::<Vec<u8>>();
        let writer = Cursor::new(Vec::new());

        // Spawn pump task
        let handle = tokio::spawn(async move {
            let mut writer = writer;
            pump_rx_to_write(rx, &mut writer).await?;
            Ok::<_, io::Error>(writer)
        });

        // Send some chunks
        tx.send(&b"hello ".to_vec()).await.unwrap();
        tx.send(&b"world".to_vec()).await.unwrap();
        drop(tx); // Close the channel

        // Wait for pump to complete and get the writer
        let writer = handle.await.unwrap().unwrap();
        assert_eq!(writer.into_inner(), b"hello world".to_vec());
    }

    #[tokio::test]
    async fn tunnel_stream_bidirectional() {
        // Create a duplex stream (simulates a socket)
        let (client, server) = tokio::io::duplex(1024);

        // Create tunnel pair
        let (local, remote) = tunnel_pair();

        // Tunnel the client side
        let (client_read_handle, client_write_handle) =
            tunnel_stream(client, local, DEFAULT_TUNNEL_CHUNK_SIZE);

        // Use remote tunnel to send/receive
        tokio::spawn(async move {
            // Send data through the tunnel (will go to server side of duplex)
            remote.tx.send(&b"from tunnel".to_vec()).await.unwrap();
        });

        // Read from server side of duplex
        let mut server = server;
        let mut buf = vec![0u8; 1024];
        let n = tokio::io::AsyncReadExt::read(&mut server, &mut buf)
            .await
            .unwrap();
        assert!(n > 0);

        // Write to server side
        tokio::io::AsyncWriteExt::write_all(&mut server, b"to tunnel")
            .await
            .unwrap();
        drop(server); // Close to signal EOF

        // Wait for read task to complete
        client_read_handle.await.unwrap().unwrap();
        client_write_handle.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn tunnel_handles_empty_data() {
        let (tx, mut rx) = channel::<Vec<u8>>();

        // Sending empty vec should work
        tx.send(&Vec::new()).await.unwrap();

        let received = rx.recv().await.unwrap().unwrap();
        assert!(received.is_empty());
    }

    #[tokio::test]
    async fn tunnel_close_propagates() {
        let (local, remote) = tunnel_pair();

        // Drop the sender
        drop(local.tx);

        // Receiver should see channel closed
        let mut rx = remote.rx;
        let result = rx.recv().await;
        assert!(matches!(result, Ok(None)));
    }

    // ========================================================================
    // Channel ID Collection Tests
    // ========================================================================

    // r[verify call.request.channels]
    #[test]
    fn collect_channel_ids_simple_tx() {
        let tx: Tx<i32> = Tx::try_from(42u64).unwrap();
        let ids = collect_channel_ids(&tx);
        assert_eq!(ids, vec![42]);
    }

    // r[verify call.request.channels]
    #[test]
    fn collect_channel_ids_simple_rx() {
        let rx: Rx<i32> = Rx::try_from(99u64).unwrap();
        let ids = collect_channel_ids(&rx);
        assert_eq!(ids, vec![99]);
    }

    // r[verify call.request.channels]
    #[test]
    fn collect_channel_ids_tuple() {
        let rx: Rx<String> = Rx::try_from(10u64).unwrap();
        let tx: Tx<String> = Tx::try_from(20u64).unwrap();
        let args = (rx, tx);
        let ids = collect_channel_ids(&args);
        assert_eq!(ids, vec![10, 20]);
    }

    // r[verify call.request.channels]
    #[test]
    fn collect_channel_ids_nested_in_struct() {
        #[derive(facet::Facet)]
        struct StreamArgs {
            input: Rx<i32>,
            output: Tx<i32>,
            count: u32,
        }

        let args = StreamArgs {
            input: Rx::try_from(100u64).unwrap(),
            output: Tx::try_from(200u64).unwrap(),
            count: 5,
        };
        let ids = collect_channel_ids(&args);
        assert_eq!(ids, vec![100, 200]);
    }

    // r[verify call.request.channels]
    #[test]
    fn collect_channel_ids_option_some() {
        let tx: Tx<i32> = Tx::try_from(55u64).unwrap();
        let args: Option<Tx<i32>> = Some(tx);
        let ids = collect_channel_ids(&args);
        assert_eq!(ids, vec![55]);
    }

    // r[verify call.request.channels]
    #[test]
    fn collect_channel_ids_option_none() {
        let args: Option<Tx<i32>> = None;
        let ids = collect_channel_ids(&args);
        assert!(ids.is_empty());
    }

    // r[verify call.request.channels]
    #[test]
    fn collect_channel_ids_vec() {
        let tx1: Tx<i32> = Tx::try_from(1u64).unwrap();
        let tx2: Tx<i32> = Tx::try_from(2u64).unwrap();
        let tx3: Tx<i32> = Tx::try_from(3u64).unwrap();
        let args: Vec<Tx<i32>> = vec![tx1, tx2, tx3];
        let ids = collect_channel_ids(&args);
        assert_eq!(ids, vec![1, 2, 3]);
    }

    // r[verify call.request.channels]
    #[test]
    fn collect_channel_ids_deeply_nested() {
        #[derive(facet::Facet)]
        struct Outer {
            inner: Inner,
        }

        #[derive(facet::Facet)]
        struct Inner {
            stream: Tx<u8>,
        }

        let args = Outer {
            inner: Inner {
                stream: Tx::try_from(777u64).unwrap(),
            },
        };
        let ids = collect_channel_ids(&args);
        assert_eq!(ids, vec![777]);
    }
}
