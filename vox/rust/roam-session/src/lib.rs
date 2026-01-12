#![deny(unsafe_code)]

//! Session/state machine and RPC-level utilities.
//!
//! Canonical definitions live in `docs/content/spec/_index.md`,
//! `docs/content/rust-spec/_index.md`, and `docs/content/shm-spec/_index.md`.

use std::marker::PhantomData;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use facet::Facet;
use std::convert::Infallible;
use tokio::sync::mpsc;

pub use roam_frame::{Frame, MsgDesc, OwnedMessage, Payload};

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

/// A wrapper around `Option<mpsc::Sender<Vec<u8>>>` that implements Facet.
///
/// This allows `Poke::get_mut::<SenderSlot>()` to work, enabling `.take()`
/// via reflection. Used by `ConnectionHandle::call` to extract senders from
/// `Tx<T>` arguments and register them with the stream registry.
#[derive(Facet)]
#[facet(opaque)]
pub struct SenderSlot {
    /// The optional sender. Public within crate for `Tx::send()` access.
    pub(crate) inner: Option<mpsc::Sender<Vec<u8>>>,
}

impl SenderSlot {
    /// Create a slot containing a sender.
    pub fn new(tx: mpsc::Sender<Vec<u8>>) -> Self {
        Self { inner: Some(tx) }
    }

    /// Create an empty slot.
    pub fn empty() -> Self {
        Self { inner: None }
    }

    /// Take the sender out of the slot, leaving it empty.
    pub fn take(&mut self) -> Option<mpsc::Sender<Vec<u8>>> {
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
    pub fn set(&mut self, tx: mpsc::Sender<Vec<u8>>) {
        self.inner = Some(tx);
    }
}

// ============================================================================
// TaskTxSlot - Wrapper for Option<Sender<TaskMessage>> that implements Facet
// ============================================================================

/// A wrapper around `Option<mpsc::Sender<TaskMessage>>` that implements Facet.
///
/// This allows `Poke::get_mut::<TaskTxSlot>()` to work, enabling reflection-based
/// hydration of `Tx<T>` handles on the server side. The task_tx sends Data/Close
/// messages directly to the connection driver.
#[derive(Facet)]
#[facet(opaque)]
pub struct TaskTxSlot {
    /// The optional sender. Public within crate for `Tx::send()` access.
    pub(crate) inner: Option<mpsc::Sender<TaskMessage>>,
}

impl TaskTxSlot {
    /// Create a slot containing a task sender.
    pub fn new(tx: mpsc::Sender<TaskMessage>) -> Self {
        Self { inner: Some(tx) }
    }

    /// Create an empty slot.
    pub fn empty() -> Self {
        Self { inner: None }
    }

    /// Take the sender out of the slot, leaving it empty.
    pub fn take(&mut self) -> Option<mpsc::Sender<TaskMessage>> {
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
    pub fn set(&mut self, tx: mpsc::Sender<TaskMessage>) {
        self.inner = Some(tx);
    }

    /// Clone the sender if present.
    pub fn clone_inner(&self) -> Option<mpsc::Sender<TaskMessage>> {
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
///   `ChannelRegistry::bind_streams` sets this, and `send()` writes `TaskMessage::Data`.
#[derive(Facet)]
#[facet(proxy = u64)]
pub struct Tx<T: 'static> {
    /// The unique stream ID for this stream.
    /// Public so Connection can poke it when binding streams.
    pub channel_id: ChannelId,
    /// Channel sender for outgoing data (client-side mode).
    /// Used when Tx is created via `roam::channel()`.
    pub sender: SenderSlot,
    /// Direct task message sender (server-side mode).
    /// Used when Tx is hydrated by `ChannelRegistry::bind_streams`.
    pub task_tx: TaskTxSlot,
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
        Ok(Tx {
            channel_id,
            sender: SenderSlot::empty(),
            task_tx: TaskTxSlot::empty(),
            _marker: PhantomData,
        })
    }
}

impl<T: 'static> Tx<T> {
    /// Create a new Tx stream with the given ID and sender channel (client-side mode).
    pub fn new(channel_id: ChannelId, tx: mpsc::Sender<Vec<u8>>) -> Self {
        Self {
            channel_id,
            sender: SenderSlot::new(tx),
            task_tx: TaskTxSlot::empty(),
            _marker: PhantomData,
        }
    }

    /// Create an unbound Tx with a sender but channel_id 0.
    ///
    /// Used by `roam::channel()` to create a pair before binding.
    /// Connection will poke the channel_id when binding.
    pub fn unbound(tx: mpsc::Sender<Vec<u8>>) -> Self {
        Self {
            channel_id: 0,
            sender: SenderSlot::new(tx),
            task_tx: TaskTxSlot::empty(),
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
    /// - Server-side: sends `TaskMessage::Data` directly to connection driver
    /// - Client-side: sends raw bytes to intermediate channel (drained by connection)
    pub async fn send(&self, value: &T) -> Result<(), TxError>
    where
        T: Facet<'static>,
    {
        let bytes = facet_postcard::to_vec(value).map_err(TxError::Serialize)?;

        // Server-side mode: send TaskMessage::Data directly
        if let Some(task_tx) = self.task_tx.inner.as_ref() {
            task_tx
                .send(TaskMessage::Data {
                    channel_id: self.channel_id,
                    payload: bytes,
                })
                .await
                .map_err(|_| TxError::Closed)
        }
        // Client-side mode: send raw bytes to drain task
        else if let Some(tx) = self.sender.inner.as_ref() {
            tx.send(bytes).await.map_err(|_| TxError::Closed)
        } else {
            Err(TxError::Taken)
        }
    }
}

/// When a Tx is dropped, send a Close message if in server-side mode.
///
/// r[impl channeling.close] - Close terminates the stream.
impl<T: 'static> Drop for Tx<T> {
    fn drop(&mut self) {
        // Only send Close in server-side mode (task_tx is set)
        if let Some(task_tx) = self.task_tx.inner.take() {
            let channel_id = self.channel_id;
            // Use try_send for synchronous Close delivery.
            // This ensures Close is queued before Response in dispatch_call.
            // If the channel is full, we still need to send Close, so spawn as fallback.
            if task_tx.try_send(TaskMessage::Close { channel_id }).is_err() {
                // Channel full or closed - spawn as fallback
                tokio::spawn(async move {
                    let _ = task_tx.send(TaskMessage::Close { channel_id }).await;
                });
            }
        }
        // Client-side mode: dropping the sender closes the channel,
        // which signals the drain task to finish and send Close
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

/// A wrapper around `Option<mpsc::Receiver<Vec<u8>>>` that implements Facet.
///
/// This allows `Poke::get_mut::<ReceiverSlot>()` to work, enabling `.take()`
/// via reflection. Used by `ConnectionHandle::call` to extract receivers from
/// `Rx<T>` arguments and register them with the stream registry.
#[derive(Facet)]
#[facet(opaque)]
pub struct ReceiverSlot {
    /// The optional receiver. Public within crate for `Rx::recv()` access.
    pub(crate) inner: Option<mpsc::Receiver<Vec<u8>>>,
}

impl ReceiverSlot {
    /// Create a slot containing a receiver.
    pub fn new(rx: mpsc::Receiver<Vec<u8>>) -> Self {
        Self { inner: Some(rx) }
    }

    /// Create an empty slot.
    pub fn empty() -> Self {
        Self { inner: None }
    }

    /// Take the receiver out of the slot, leaving it empty.
    pub fn take(&mut self) -> Option<mpsc::Receiver<Vec<u8>>> {
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
    pub fn set(&mut self, rx: mpsc::Receiver<Vec<u8>>) {
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
    pub fn new(channel_id: ChannelId, rx: mpsc::Receiver<Vec<u8>>) -> Self {
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
    pub fn unbound(rx: mpsc::Receiver<Vec<u8>>) -> Self {
        Self {
            channel_id: 0,
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
    let (sender, receiver) = mpsc::channel::<Vec<u8>>(64);
    (Tx::unbound(sender), Rx::unbound(receiver))
}

// ============================================================================
// Stream Registry
// ============================================================================

use std::collections::{HashMap, HashSet};

/// Message from spawned handler tasks to the connection driver.
///
/// All messages from tasks go through a single channel to preserve ordering.
/// This ensures Data/Close messages are sent before the Response.
#[derive(Debug)]
pub enum TaskMessage {
    /// Send a Data message on a stream.
    Data {
        channel_id: ChannelId,
        payload: Vec<u8>,
    },
    /// Send a Close message to end a stream.
    Close { channel_id: ChannelId },
    /// Send a Response message (call completed).
    Response { request_id: u64, payload: Vec<u8> },
}

/// Registry of active streams for a connection.
///
/// Handles incoming streams (Data from wire → `Rx<T>` / `Tx<T>` handles).
/// For outgoing streams (server `Tx<T>` args), spawned tasks drain receivers
/// and send Data/Close messages via `task_tx`.
///
/// r[impl channeling.unknown] - Unknown stream IDs cause Goodbye.
pub struct ChannelRegistry {
    /// Streams where we receive Data messages (backing `Rx<T>` or `Tx<T>` handles on our side).
    /// Key: channel_id, Value: sender to route Data payloads to the handle.
    incoming: HashMap<ChannelId, mpsc::Sender<Vec<u8>>>,

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

    /// Channel for spawned tasks to send messages (Data/Close/Response).
    /// The driver owns the receiving end and sends these on the wire.
    /// Using a single channel ensures correct ordering (Data/Close before Response).
    task_tx: mpsc::Sender<TaskMessage>,
}

impl ChannelRegistry {
    /// Create a new registry with the given initial credit and task message channel.
    ///
    /// The `task_tx` is used by spawned tasks to send Data/Close/Response messages
    /// back to the driver for transmission on the wire.
    ///
    /// r[impl flow.channel.initial-credit] - Each stream starts with this credit.
    pub fn new_with_credit(initial_credit: u32, task_tx: mpsc::Sender<TaskMessage>) -> Self {
        Self {
            incoming: HashMap::new(),
            closed: HashSet::new(),
            incoming_credit: HashMap::new(),
            outgoing_credit: HashMap::new(),
            initial_credit,
            task_tx,
        }
    }

    /// Create a new registry with default infinite credit.
    ///
    /// r[impl flow.channel.infinite-credit] - Implementations MAY use very large credit.
    /// r[impl flow.channel.zero-credit] - With infinite credit, zero-credit never occurs.
    /// This disables backpressure but simplifies implementation.
    pub fn new(task_tx: mpsc::Sender<TaskMessage>) -> Self {
        Self::new_with_credit(u32::MAX, task_tx)
    }

    /// Get a clone of the task message sender.
    ///
    /// Used by codegen to spawn tasks that send Data/Close/Response messages.
    pub fn task_tx(&self) -> mpsc::Sender<TaskMessage> {
        self.task_tx.clone()
    }

    /// Register an incoming stream.
    ///
    /// The connection layer will route Data messages for this channel_id to the sender.
    /// Used for both `Rx<T>` (caller receives from callee) and `Tx<T>` (callee sends to caller).
    ///
    /// r[impl flow.channel.initial-credit] - Stream starts with initial credit.
    pub fn register_incoming(&mut self, channel_id: ChannelId, tx: mpsc::Sender<Vec<u8>>) {
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
    ) -> Result<(mpsc::Sender<Vec<u8>>, Vec<u8>), ChannelError> {
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
    fn bind_streams_recursive(&mut self, poke: facet::Poke<'_, '_>) {
        let shape = poke.shape();

        // Check if this is an Rx or Tx type
        if shape.module_path == Some("roam_session") {
            if shape.type_identifier == "Rx" {
                self.bind_rx_stream(poke);
                return;
            } else if shape.type_identifier == "Tx" {
                self.bind_tx_stream(poke);
                return;
            }
        }

        // Recurse into struct/tuple fields
        // (Tuples are represented as structs with numeric field indices in facet)
        if let Ok(mut ps) = poke.into_struct() {
            let field_count = ps.field_count();
            for i in 0..field_count {
                if let Ok(field_poke) = ps.field(i) {
                    self.bind_streams_recursive(field_poke);
                }
            }
        }
        // TODO: Handle enums, arrays, etc. if needed
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
                return;
            };

            // Create channel and set receiver slot
            let (tx, rx) = mpsc::channel::<Vec<u8>>(64);

            if let Ok(mut receiver_field) = ps.field_by_name("receiver")
                && let Ok(slot) = receiver_field.get_mut::<ReceiverSlot>()
            {
                slot.set(rx);
            }

            // Register for incoming data routing
            self.register_incoming(channel_id, tx);
        }
    }

    /// Bind a Tx<T> stream for server-side dispatch.
    ///
    /// Server sends data to client on this stream.
    /// Sets the task_tx directly so Tx::send() writes TaskMessage::Data to the wire.
    /// When the Tx is dropped, it sends TaskMessage::Close automatically.
    fn bind_tx_stream(&mut self, poke: facet::Poke<'_, '_>) {
        if let Ok(mut ps) = poke.into_struct() {
            // Set task_tx so Tx::send() can write directly to the wire
            if let Ok(mut task_tx_field) = ps.field_by_name("task_tx")
                && let Ok(slot) = task_tx_field.get_mut::<TaskTxSlot>()
            {
                slot.set(self.task_tx.clone());
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
/// r[impl unary.request-id.uniqueness] - monotonically increasing counter starting at 1
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
/// - `E`: Result error type (must implement Facet for serialization)
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
pub fn dispatch_call<A, R, E, F, Fut>(
    payload: Vec<u8>,
    request_id: u64,
    registry: &mut ChannelRegistry,
    handler: F,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'static>>
where
    A: Facet<'static> + Send,
    R: Facet<'static> + Send,
    E: Facet<'static> + Send,
    F: FnOnce(A) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = Result<R, RoamError<E>>> + Send + 'static,
{
    // Deserialize args
    let mut args: A = match facet_postcard::from_slice(&payload) {
        Ok(args) => args,
        Err(_) => {
            let task_tx = registry.task_tx();
            return Box::pin(async move {
                // InvalidPayload error
                let _ = task_tx
                    .send(TaskMessage::Response {
                        request_id,
                        payload: vec![1, 2],
                    })
                    .await;
            });
        }
    };

    // Bind streams via reflection
    registry.bind_streams(&mut args);

    let task_tx = registry.task_tx();

    Box::pin(async move {
        let result = handler(args).await;
        let payload = match result {
            Ok(result) => {
                let mut out = vec![0u8];
                match facet_postcard::to_vec(&result) {
                    Ok(bytes) => out.extend(bytes),
                    Err(_) => return,
                }
                out
            }
            Err(_e) => vec![1, 1],
        };
        let _ = task_tx
            .send(TaskMessage::Response {
                request_id,
                payload,
            })
            .await;
    })
}

/// Send an "unknown method" error response.
///
/// Used by dispatchers when the method_id doesn't match any known method.
pub fn dispatch_unknown_method(
    request_id: u64,
    registry: &mut ChannelRegistry,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'static>> {
    let task_tx = registry.task_tx();
    Box::pin(async move {
        // UnknownMethod error
        let _ = task_tx
            .send(TaskMessage::Response {
                request_id,
                payload: vec![1, 1],
            })
            .await;
    })
}

// ============================================================================
// Service Dispatcher
// ============================================================================

/// Trait for dispatching requests to a service.
///
/// The dispatcher handles both unary and streaming methods uniformly.
/// Stream binding is done via reflection (Poke) on the deserialized args.
pub trait ServiceDispatcher: Send + Sync {
    /// Dispatch a request and send the response via the task channel.
    ///
    /// The dispatcher is responsible for:
    /// - Looking up the method by method_id
    /// - Deserializing arguments from payload
    /// - Binding any Tx/Rx streams via the registry
    /// - Calling the service method
    /// - Sending Data/Close messages for any Tx streams
    /// - Sending the Response message via TaskMessage::Response
    ///
    /// By using a single channel for Data/Close/Response, correct ordering is guaranteed:
    /// all stream Data and Close messages are sent before the Response.
    ///
    /// Returns a boxed future with `'static` lifetime so it can be spawned.
    /// Implementations should clone their service into the future to achieve this.
    ///
    /// r[impl channeling.allocation.caller] - Stream IDs are decoded from payload (caller allocated).
    fn dispatch(
        &self,
        method_id: u64,
        payload: Vec<u8>,
        request_id: u64,
        registry: &mut ChannelRegistry,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'static>>;
}

/// A dispatcher that routes to one of two dispatchers based on method ID.
///
/// Methods listed in `first_methods` are routed to the first dispatcher,
/// all others to the second.
pub struct RoutedDispatcher<A, B> {
    first: A,
    second: B,
    first_methods: &'static [u64],
}

impl<A, B> RoutedDispatcher<A, B> {
    /// Create a new routed dispatcher.
    ///
    /// Methods in `first_methods` are routed to `first`, all others to `second`.
    pub fn new(first: A, second: B, first_methods: &'static [u64]) -> Self {
        Self {
            first,
            second,
            first_methods,
        }
    }
}

impl<A, B> ServiceDispatcher for RoutedDispatcher<A, B>
where
    A: ServiceDispatcher,
    B: ServiceDispatcher,
{
    fn dispatch(
        &self,
        method_id: u64,
        payload: Vec<u8>,
        request_id: u64,
        registry: &mut ChannelRegistry,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'static>> {
        if self.first_methods.contains(&method_id) {
            self.first
                .dispatch(method_id, payload, request_id, registry)
        } else {
            self.second
                .dispatch(method_id, payload, request_id, registry)
        }
    }
}

// TODO: Remove this shim once facet implements `Facet` for `core::convert::Infallible`
// and for the never type `!` (facet-rs/facet#1668), then use `Infallible`.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct Never;

/// Call error type encoded in unary responses.
///
/// r[impl core.error.roam-error] - Wraps call results to distinguish app vs protocol errors
/// r[impl unary.response.encoding] - Response is `Result<T, RoamError<E>>`
/// r[impl unary.error.roam-error] - Protocol errors use RoamError variants
/// r[impl unary.error.protocol] - Discriminants 1-3 are protocol-level errors
///
/// Spec: `docs/content/spec/_index.md` "RoamError".
#[repr(u8)]
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub enum RoamError<E> {
    /// r[impl core.error.call-vs-connection] - User errors affect only this call
    /// r[impl unary.error.user] - User(E) carries the application's error type
    User(E) = 0,
    /// r[impl unary.error.unknown-method] - Method ID not recognized
    UnknownMethod = 1,
    /// r[impl unary.error.invalid-payload] - Request payload deserialization failed
    InvalidPayload = 2,
    Cancelled = 3,
}

pub type CallResult<T, E> = ::core::result::Result<T, RoamError<E>>;
pub type BorrowedCallResult<T, E> = OwnedMessage<CallResult<T, E>>;

// ============================================================================
// Connection Handle (Client-side API)
// ============================================================================

/// Error from making an outgoing call.
#[derive(Debug)]
pub enum CallError {
    /// Failed to encode request payload.
    Encode(facet_postcard::SerializeError),
    /// Failed to decode response payload.
    Decode(facet_postcard::DeserializeError<facet_postcard::PostcardError>),
    /// Connection was closed before response.
    ConnectionClosed,
    /// Driver task is gone.
    DriverGone,
}

impl CallError {
    /// Decode a response payload into the expected type.
    ///
    /// This is a convenience method for the common pattern of deserializing
    /// the response payload after a call.
    pub fn decode_response<T: Facet<'static>>(payload: &[u8]) -> Result<T, CallError> {
        facet_postcard::from_slice(payload).map_err(CallError::Decode)
    }
}

impl std::fmt::Display for CallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CallError::Encode(e) => write!(f, "encode error: {e}"),
            CallError::Decode(e) => write!(f, "decode error: {e}"),
            CallError::ConnectionClosed => write!(f, "connection closed"),
            CallError::DriverGone => write!(f, "driver task stopped"),
        }
    }
}

impl std::error::Error for CallError {}

/// Command sent from ConnectionHandle to the Driver.
#[derive(Debug)]
pub enum HandleCommand {
    /// Send a request and expect a response.
    Call {
        request_id: u64,
        method_id: u64,
        metadata: Vec<(String, roam_wire::MetadataValue)>,
        payload: Vec<u8>,
        response_tx: tokio::sync::oneshot::Sender<Result<Vec<u8>, CallError>>,
    },
}

/// Shared state between ConnectionHandle and Driver.
struct HandleShared {
    /// Channel to send commands to the driver.
    command_tx: mpsc::Sender<HandleCommand>,
    /// Request ID generator.
    request_ids: RequestIdGenerator,
    /// Stream ID allocator.
    channel_ids: ChannelIdAllocator,
    /// Stream registry for routing incoming data.
    /// Protected by a mutex since handles may create streams concurrently.
    channel_registry: std::sync::Mutex<ChannelRegistry>,
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
    /// Create a new handle with the given command channel, role, and task message sender.
    pub fn new(
        command_tx: mpsc::Sender<HandleCommand>,
        role: Role,
        initial_credit: u32,
        task_tx: mpsc::Sender<TaskMessage>,
    ) -> Self {
        let channel_registry = ChannelRegistry::new_with_credit(initial_credit, task_tx);
        Self {
            shared: Arc::new(HandleShared {
                command_tx,
                request_ids: RequestIdGenerator::new(),
                channel_ids: ChannelIdAllocator::new(role),
                channel_registry: std::sync::Mutex::new(channel_registry),
            }),
        }
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
    pub async fn call<T: Facet<'static>>(
        &self,
        method_id: u64,
        args: &mut T,
    ) -> Result<Vec<u8>, CallError> {
        // Walk args and bind any streams
        self.bind_streams(args);

        let payload = facet_postcard::to_vec(args).map_err(CallError::Encode)?;
        self.call_raw(method_id, payload).await
    }

    /// Walk args and bind any Rx<T> or Tx<T> streams.
    fn bind_streams<T: Facet<'static>>(&self, args: &mut T) {
        let poke = facet::Poke::new(args);
        self.bind_streams_recursive(poke);
    }

    /// Recursively walk a Poke value looking for Rx/Tx streams to bind.
    fn bind_streams_recursive(&self, poke: facet::Poke<'_, '_>) {
        let shape = poke.shape();

        // Check if this is an Rx or Tx type
        if shape.module_path == Some("roam_session") {
            if shape.type_identifier == "Rx" {
                self.bind_rx_stream(poke);
                return;
            } else if shape.type_identifier == "Tx" {
                self.bind_tx_stream(poke);
                return;
            }
        }

        // Recurse into struct fields
        if let Ok(mut ps) = poke.into_struct() {
            let field_count = ps.field_count();
            for i in 0..field_count {
                if let Ok(field_poke) = ps.field(i) {
                    self.bind_streams_recursive(field_poke);
                }
            }
        }
        // TODO: Handle tuples, enums, arrays, etc.
    }

    /// Bind an Rx<T> stream - caller passes receiver, keeps sender.
    /// We take the receiver and spawn a drain task.
    fn bind_rx_stream(&self, poke: facet::Poke<'_, '_>) {
        let channel_id = self.alloc_channel_id();

        if let Ok(mut ps) = poke.into_struct() {
            // Set channel_id field by getting mutable access to the u64
            if let Ok(mut channel_id_field) = ps.field_by_name("channel_id")
                && let Ok(id_ref) = channel_id_field.get_mut::<ChannelId>()
            {
                *id_ref = channel_id;
            }

            // Take the receiver from ReceiverSlot
            if let Ok(mut receiver_field) = ps.field_by_name("receiver")
                && let Ok(slot) = receiver_field.get_mut::<ReceiverSlot>()
                && let Some(mut rx) = slot.take()
            {
                // Spawn task to drain rx and send Data messages
                let task_tx = self.shared.channel_registry.lock().unwrap().task_tx();
                tokio::spawn(async move {
                    while let Some(data) = rx.recv().await {
                        let _ = task_tx
                            .send(TaskMessage::Data {
                                channel_id,
                                payload: data,
                            })
                            .await;
                    }
                    // Stream ended, send Close
                    let _ = task_tx.send(TaskMessage::Close { channel_id }).await;
                });
            }
        }
    }

    /// Bind a Tx<T> stream - caller passes sender, keeps receiver.
    /// We take the sender and register for incoming Data routing.
    fn bind_tx_stream(&self, poke: facet::Poke<'_, '_>) {
        let channel_id = self.alloc_channel_id();

        if let Ok(mut ps) = poke.into_struct() {
            // Set channel_id field by getting mutable access to the u64
            if let Ok(mut channel_id_field) = ps.field_by_name("channel_id")
                && let Ok(id_ref) = channel_id_field.get_mut::<ChannelId>()
            {
                *id_ref = channel_id;
            }

            // Take the sender from SenderSlot
            if let Ok(mut sender_field) = ps.field_by_name("sender")
                && let Ok(slot) = sender_field.get_mut::<SenderSlot>()
                && let Some(tx) = slot.take()
            {
                // Register for incoming Data routing
                self.register_incoming(channel_id, tx);
            }
        }
    }

    /// Make a raw RPC call with pre-serialized payload.
    ///
    /// Returns the raw response payload bytes.
    pub async fn call_raw(&self, method_id: u64, payload: Vec<u8>) -> Result<Vec<u8>, CallError> {
        let request_id = self.shared.request_ids.next();
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();

        let cmd = HandleCommand::Call {
            request_id,
            method_id,
            metadata: Vec::new(),
            payload,
            response_tx,
        };

        self.shared
            .command_tx
            .send(cmd)
            .await
            .map_err(|_| CallError::DriverGone)?;

        response_rx.await.map_err(|_| CallError::DriverGone)?
    }

    /// Allocate a stream ID for an outgoing stream.
    ///
    /// Used internally when binding streams during call().
    pub fn alloc_channel_id(&self) -> ChannelId {
        self.shared.channel_ids.next()
    }

    /// Register an incoming stream (we receive data from peer).
    ///
    /// Used when schema has `Tx<T>` (callee sends to caller) - we receive that data.
    pub fn register_incoming(&self, channel_id: ChannelId, tx: mpsc::Sender<Vec<u8>>) {
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
        self.shared
            .channel_registry
            .lock()
            .unwrap()
            .close(channel_id);
    }

    /// Reset a stream.
    pub fn reset_channel(&self, channel_id: ChannelId) {
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

/// Minimal async RPC caller for unary requests.
///
/// This is intentionally small: it deals only in `method_id` + payload bytes, and
/// returns a `Frame` so callers can do zero-copy deserialization (borrow from the
/// response buffer / SHM slot).
///
/// r[impl unary.initiate] - call_unary sends a Request message to initiate a call
/// r[impl unary.lifecycle.ordering] - implementations correlate responses by request_id
#[allow(async_fn_in_trait)]
pub trait UnaryCaller {
    type Error;

    async fn call_unary(&mut self, method_id: u64, payload: Vec<u8>) -> Result<Frame, Self::Error>;
}

// ============================================================================
// Tunnel Adapters for AsyncRead/AsyncWrite Streams
// ============================================================================

use std::io;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::task::JoinHandle;

/// Default chunk size for tunnel pumps (32KB).
///
/// Balances throughput with memory usage and slot consumption.
/// Larger values improve throughput but use more memory per read.
/// Smaller values improve latency but increase syscall overhead.
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
        let (task_tx, _task_rx) = mpsc::channel(10);
        ChannelRegistry::new(task_tx)
    }

    // r[verify channeling.data-after-close]
    #[tokio::test]
    async fn data_after_close_is_rejected() {
        let mut registry = test_registry();
        let (tx, _rx) = mpsc::channel::<Vec<u8>>(10);
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
        let (tx, mut rx) = mpsc::channel::<Vec<u8>>(10);
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
        let (tx, mut rx) = mpsc::channel::<Vec<u8>>(10);
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
}
