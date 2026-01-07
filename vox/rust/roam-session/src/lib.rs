#![deny(unsafe_code)]

//! Session/state machine and RPC-level utilities.
//!
//! Canonical definitions live in `docs/content/spec/_index.md`,
//! `docs/content/rust-spec/_index.md`, and `docs/content/shm-spec/_index.md`.

use std::marker::PhantomData;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use facet::{Attr, Def, Facet, Shape, ShapeBuilder, Type, TypeParam, UserType};
use tokio::sync::{Notify, mpsc};

pub use roam_frame::{Frame, MsgDesc, OwnedMessage, Payload};

// ============================================================================
// Streaming types
// ============================================================================

/// Stream ID type.
pub type StreamId = u64;

/// Connection role - determines stream ID parity.
///
/// The initiator is whoever opened the connection (e.g. connected to a TCP socket,
/// or opened an SHM channel). The acceptor is whoever accepted/received the connection.
///
/// r[impl streaming.id.parity]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// Initiator uses odd stream IDs (1, 3, 5, ...).
    Initiator,
    /// Acceptor uses even stream IDs (2, 4, 6, ...).
    Acceptor,
}

/// Allocates unique stream IDs with correct parity.
///
/// r[impl streaming.id.uniqueness] - IDs are unique within a connection.
/// r[impl streaming.id.parity] - Initiator uses odd, Acceptor uses even.
pub struct StreamIdAllocator {
    next: AtomicU64,
}

impl StreamIdAllocator {
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
    pub fn next(&self) -> StreamId {
        self.next.fetch_add(2, Ordering::Relaxed)
    }
}

// ============================================================================
// Outgoing Stream Infrastructure
// ============================================================================

/// Message sent on an outgoing stream channel.
///
/// r[impl streaming.data] - Data contains serialized stream element.
/// r[impl streaming.close] - Close terminates the stream.
#[derive(Debug)]
pub enum OutgoingMessage {
    /// Serialized data to send on the stream.
    Data(Vec<u8>),
    /// Close the stream gracefully.
    Close,
}

/// Sender handle for outgoing stream data.
///
/// This is the internal channel that `Push<T>` writes to.
/// The connection layer reads from the corresponding receiver.
#[derive(Clone)]
pub struct OutgoingSender {
    stream_id: StreamId,
    tx: mpsc::Sender<OutgoingMessage>,
    /// Notify the connection loop that data is available.
    notify: Arc<Notify>,
}

impl OutgoingSender {
    /// Create a new outgoing sender.
    pub fn new(
        stream_id: StreamId,
        tx: mpsc::Sender<OutgoingMessage>,
        notify: Arc<Notify>,
    ) -> Self {
        Self {
            stream_id,
            tx,
            notify,
        }
    }

    /// Get the stream ID.
    pub fn stream_id(&self) -> StreamId {
        self.stream_id
    }

    /// Send serialized data.
    pub async fn send_data(
        &self,
        data: Vec<u8>,
    ) -> Result<(), mpsc::error::SendError<OutgoingMessage>> {
        let result = self.tx.send(OutgoingMessage::Data(data)).await;
        if result.is_ok() {
            self.notify.notify_one();
        }
        result
    }

    /// Send close signal (used by Push Drop impl).
    ///
    /// r[impl streaming.lifecycle.caller-closes-pushes] - Caller sends Close when done.
    pub fn send_close(&self) {
        // Use try_send since Drop can't be async
        if self.tx.try_send(OutgoingMessage::Close).is_ok() {
            self.notify.notify_one();
        }
    }
}

/// Push stream handle - caller sends data to callee.
///
/// r[impl streaming.caller-pov] - From caller's perspective, Push means "I send".
/// r[impl streaming.type] - Serializes as u64 stream ID on wire.
/// r[impl streaming.holder-semantics] - The holder sends on this stream.
/// r[impl streaming.streams-outlive-response] - Push streams may outlive Response.
/// r[impl streaming.lifecycle.immediate-data] - Can send Data before Response.
/// r[impl streaming.lifecycle.speculative] - Early Data may be wasted on error.
///
/// When dropped, a Close message is sent automatically.
///
/// This type implements `Facet` manually with the `roam::push` attribute marker
/// so that `roam_reflect::type_detail` recognizes it and generates `TypeDetail::Push`.
pub struct Push<T: Facet<'static>> {
    /// The unique stream ID for this stream.
    stream_id: StreamId,
    /// Channel sender for outgoing data.
    sender: OutgoingSender,
    /// Phantom data for the element type.
    _marker: PhantomData<fn(T)>,
}

/// Static marker for the roam::push attribute.
static PUSH_MARKER: () = ();

/// Static marker for the roam::pull attribute.
static PULL_MARKER: () = ();

/// Static attribute array for roam::push marker.
static ROAM_PUSH_ATTRS: [Attr; 1] = [Attr::new(Some("roam"), "push", &PUSH_MARKER)];

/// Static attribute array for roam::pull marker.
static ROAM_PULL_ATTRS: [Attr; 1] = [Attr::new(Some("roam"), "pull", &PULL_MARKER)];

// SAFETY: Push<T> is a handle type that doesn't expose T directly in its shape.
// The roam::push attribute marks it for special handling by roam_reflect.
#[allow(unsafe_code)]
unsafe impl<T: Facet<'static>> Facet<'static> for Push<T> {
    const SHAPE: &'static Shape = &const {
        ShapeBuilder::for_sized::<Push<T>>("Push")
            .ty(Type::User(UserType::Opaque))
            .def(Def::Scalar)
            .type_params(&[TypeParam {
                name: "T",
                shape: T::SHAPE,
            }])
            .attributes(&ROAM_PUSH_ATTRS)
            .build()
    };
}

impl<T: Facet<'static>> Push<T> {
    /// Create a new Push stream with the given sender.
    pub fn new(sender: OutgoingSender) -> Self {
        Self {
            stream_id: sender.stream_id(),
            sender,
            _marker: PhantomData,
        }
    }

    /// Get the stream ID.
    pub fn stream_id(&self) -> StreamId {
        self.stream_id
    }

    /// Send a value on this stream.
    ///
    /// r[impl streaming.data] - Data messages carry serialized values.
    pub async fn send(&self, value: &T) -> Result<(), PushError> {
        let bytes = facet_postcard::to_vec(value).map_err(PushError::Serialize)?;
        self.sender
            .send_data(bytes)
            .await
            .map_err(|_| PushError::Closed)
    }
}

impl<T: Facet<'static>> Drop for Push<T> {
    /// r[impl streaming.lifecycle.caller-closes-pushes] - Send Close when Push is dropped.
    fn drop(&mut self) {
        self.sender.send_close();
    }
}

/// Error when sending on a Push stream.
#[derive(Debug)]
pub enum PushError {
    /// Failed to serialize the value.
    Serialize(facet_postcard::SerializeError),
    /// The stream channel is closed.
    Closed,
}

impl std::fmt::Display for PushError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PushError::Serialize(e) => write!(f, "serialize error: {e}"),
            PushError::Closed => write!(f, "stream closed"),
        }
    }
}

impl std::error::Error for PushError {}

/// Pull stream handle - caller receives data from callee.
///
/// r[impl streaming.caller-pov] - From caller's perspective, Pull means "I receive".
/// r[impl streaming.type] - Serializes as u64 stream ID on wire.
/// r[impl streaming.holder-semantics] - The holder receives from this stream.
///
/// This type implements `Facet` manually with the `roam::pull` attribute marker
/// so that `roam_reflect::type_detail` recognizes it and generates `TypeDetail::Pull`.
pub struct Pull<T: Facet<'static>> {
    /// The unique stream ID for this stream.
    stream_id: StreamId,
    /// Channel receiver for incoming data.
    rx: mpsc::Receiver<Vec<u8>>,
    /// Phantom data for the element type.
    _marker: PhantomData<fn() -> T>,
}

// SAFETY: Pull<T> is a handle type that doesn't expose T directly in its shape.
// The roam::pull attribute marks it for special handling by roam_reflect.
#[allow(unsafe_code)]
unsafe impl<T: Facet<'static>> Facet<'static> for Pull<T> {
    const SHAPE: &'static Shape = &const {
        ShapeBuilder::for_sized::<Pull<T>>("Pull")
            .ty(Type::User(UserType::Opaque))
            .def(Def::Scalar)
            .type_params(&[TypeParam {
                name: "T",
                shape: T::SHAPE,
            }])
            .attributes(&ROAM_PULL_ATTRS)
            .build()
    };
}

impl<T: Facet<'static>> Pull<T> {
    /// Create a new Pull stream with the given ID and receiver channel.
    pub fn new(stream_id: StreamId, rx: mpsc::Receiver<Vec<u8>>) -> Self {
        Self {
            stream_id,
            rx,
            _marker: PhantomData,
        }
    }

    /// Get the stream ID.
    pub fn stream_id(&self) -> StreamId {
        self.stream_id
    }

    /// Receive the next value from this stream.
    ///
    /// Returns `Ok(Some(value))` for each received value,
    /// `Ok(None)` when the stream is closed,
    /// or `Err` if deserialization fails.
    ///
    /// r[impl streaming.data] - Deserialize Data message payloads.
    /// r[impl streaming.data.invalid] - Caller must send Goodbye on deserialize error.
    pub async fn recv(&mut self) -> Result<Option<T>, PullError> {
        match self.rx.recv().await {
            Some(bytes) => {
                let value = facet_postcard::from_slice(&bytes).map_err(PullError::Deserialize)?;
                Ok(Some(value))
            }
            None => Ok(None),
        }
    }
}

/// Error when receiving from a Pull stream.
#[derive(Debug)]
pub enum PullError {
    /// Failed to deserialize the value.
    Deserialize(facet_postcard::DeserializeError<facet_postcard::PostcardError>),
}

impl std::fmt::Display for PullError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PullError::Deserialize(e) => write!(f, "deserialize error: {e}"),
        }
    }
}

impl std::error::Error for PullError {}

// ============================================================================
// Stream Registry
// ============================================================================

use std::collections::{HashMap, HashSet};

/// Result of polling an outgoing stream.
#[derive(Debug)]
pub enum OutgoingPoll {
    /// A Data message should be sent.
    Data {
        stream_id: StreamId,
        payload: Vec<u8>,
    },
    /// A Close message should be sent.
    Close { stream_id: StreamId },
    /// No data available (would block).
    Pending,
    /// All outgoing streams are closed.
    Done,
}

/// Registry of active streams for a connection.
///
/// Handles both incoming streams (Data from wire → `Pull<T>`) and
/// outgoing streams (`Push<T>` → Data to wire).
///
/// r[impl streaming.unknown] - Unknown stream IDs cause Goodbye.
pub struct StreamRegistry {
    /// Streams where we receive Data messages (backing `Pull<T>` handles on our side).
    /// Key: stream_id, Value: sender to route Data payloads to the `Pull<T>`.
    incoming: HashMap<StreamId, mpsc::Sender<Vec<u8>>>,

    /// Streams where we send Data messages (backing `Push<T>` handles on our side).
    /// Key: stream_id, Value: receiver to drain data from `Push<T>`.
    outgoing: HashMap<StreamId, mpsc::Receiver<OutgoingMessage>>,

    /// Stream IDs that have been closed.
    /// Used to detect data-after-close violations.
    ///
    /// r[impl streaming.data-after-close] - Track closed streams.
    closed: HashSet<StreamId>,

    /// Notify the connection loop when outgoing data is available.
    /// All OutgoingSenders share this and call notify_one() after enqueuing.
    outgoing_notify: Arc<Notify>,
}

impl StreamRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self {
            incoming: HashMap::new(),
            outgoing: HashMap::new(),
            closed: HashSet::new(),
            outgoing_notify: Arc::new(Notify::new()),
        }
    }

    /// Get the notify handle for the connection loop to wait on.
    ///
    /// When notified, call `poll_outgoing()` in a loop until it returns `Pending`.
    pub fn outgoing_notify(&self) -> Arc<Notify> {
        self.outgoing_notify.clone()
    }

    /// Register an incoming stream and return the receiver for `Pull<T>`.
    ///
    /// The connection layer will route Data messages for this stream_id to the
    /// returned receiver. The caller wraps this in a `Pull<T>`.
    ///
    /// r[impl streaming.allocation.caller] - Caller allocates stream IDs.
    pub fn register_incoming(&mut self, stream_id: StreamId) -> mpsc::Receiver<Vec<u8>> {
        // TODO: make buffer size configurable
        let (tx, rx) = mpsc::channel(64);
        self.incoming.insert(stream_id, tx);
        rx
    }

    /// Register an outgoing stream and return the sender for `Push<T>`.
    ///
    /// The connection layer will drain messages from this channel and send
    /// them as Data/Close wire messages.
    ///
    /// r[impl streaming.allocation.caller] - Caller allocates stream IDs.
    pub fn register_outgoing(&mut self, stream_id: StreamId) -> OutgoingSender {
        // TODO: make buffer size configurable
        let (tx, rx) = mpsc::channel(64);
        self.outgoing.insert(stream_id, rx);
        OutgoingSender::new(stream_id, tx, self.outgoing_notify.clone())
    }

    /// Route a Data message payload to the appropriate incoming stream.
    ///
    /// Returns Ok(()) if routed successfully, Err(StreamError) otherwise.
    ///
    /// r[impl streaming.data] - Data messages routed by stream_id.
    /// r[impl streaming.data-after-close] - Reject data on closed streams.
    pub async fn route_data(
        &self,
        stream_id: StreamId,
        payload: Vec<u8>,
    ) -> Result<(), StreamError> {
        // Check for data-after-close
        if self.closed.contains(&stream_id) {
            return Err(StreamError::DataAfterClose);
        }

        if let Some(tx) = self.incoming.get(&stream_id) {
            // If send fails, the Pull<T> was dropped - that's okay, just drop the data
            let _ = tx.send(payload).await;
            Ok(())
        } else {
            Err(StreamError::Unknown)
        }
    }

    /// Poll all outgoing streams for data to send.
    ///
    /// Returns the first available message, or Pending if none are ready.
    /// Call this in a loop in the connection's message processing.
    pub fn poll_outgoing(&mut self) -> OutgoingPoll {
        if self.outgoing.is_empty() {
            return OutgoingPoll::Done;
        }

        // Collect stream IDs first to avoid borrowing issues
        let stream_ids: Vec<StreamId> = self.outgoing.keys().copied().collect();

        for stream_id in stream_ids {
            let rx = self.outgoing.get_mut(&stream_id).unwrap();
            match rx.try_recv() {
                Ok(OutgoingMessage::Data(payload)) => {
                    return OutgoingPoll::Data { stream_id, payload };
                }
                Ok(OutgoingMessage::Close) => {
                    // Remove immediately before returning (fixes bug where we'd return
                    // before the deferred removal, leaving stale entries)
                    self.outgoing.remove(&stream_id);
                    self.closed.insert(stream_id);
                    return OutgoingPoll::Close { stream_id };
                }
                Err(mpsc::error::TryRecvError::Empty) => {
                    // No data ready, continue to next stream
                }
                Err(mpsc::error::TryRecvError::Disconnected) => {
                    // Sender dropped without sending Close - treat as implicit close
                    self.outgoing.remove(&stream_id);
                    self.closed.insert(stream_id);
                    return OutgoingPoll::Close { stream_id };
                }
            }
        }

        OutgoingPoll::Pending
    }

    /// Close an incoming stream (remove from registry).
    ///
    /// Dropping the sender will cause the `Pull<T>`'s recv() to return None.
    ///
    /// r[impl streaming.close] - Close terminates the stream.
    pub fn close(&mut self, stream_id: StreamId) {
        self.incoming.remove(&stream_id);
        self.closed.insert(stream_id);
    }

    /// Check if a stream ID is registered (either incoming or outgoing).
    pub fn contains(&self, stream_id: StreamId) -> bool {
        self.incoming.contains_key(&stream_id) || self.outgoing.contains_key(&stream_id)
    }

    /// Check if a stream ID is registered as incoming.
    pub fn contains_incoming(&self, stream_id: StreamId) -> bool {
        self.incoming.contains_key(&stream_id)
    }

    /// Check if a stream ID is registered as outgoing.
    pub fn contains_outgoing(&self, stream_id: StreamId) -> bool {
        self.outgoing.contains_key(&stream_id)
    }

    /// Check if a stream has been closed.
    pub fn is_closed(&self, stream_id: StreamId) -> bool {
        self.closed.contains(&stream_id)
    }

    /// Get the number of active outgoing streams.
    pub fn outgoing_count(&self) -> usize {
        self.outgoing.len()
    }
}

impl Default for StreamRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Error when routing stream data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamError {
    /// Stream ID not found in registry.
    Unknown,
    /// Data received after stream was closed.
    DataAfterClose,
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

#[cfg(test)]
mod tests {
    use super::*;

    // r[verify streaming.id.parity]
    #[test]
    fn stream_id_allocator_initiator_uses_odd_ids() {
        let alloc = StreamIdAllocator::new(Role::Initiator);
        assert_eq!(alloc.next(), 1);
        assert_eq!(alloc.next(), 3);
        assert_eq!(alloc.next(), 5);
        assert_eq!(alloc.next(), 7);
    }

    // r[verify streaming.id.parity]
    #[test]
    fn stream_id_allocator_acceptor_uses_even_ids() {
        let alloc = StreamIdAllocator::new(Role::Acceptor);
        assert_eq!(alloc.next(), 2);
        assert_eq!(alloc.next(), 4);
        assert_eq!(alloc.next(), 6);
        assert_eq!(alloc.next(), 8);
    }

    // r[verify streaming.holder-semantics]
    #[tokio::test]
    async fn push_serializes_and_pull_deserializes() {
        // Create a channel pair for Push → Pull communication
        let (tx, rx) = mpsc::channel::<Vec<u8>>(10);
        let notify = Arc::new(Notify::new());

        // Wrap in Push/Pull types (normally StreamRegistry would do this)
        let outgoing_sender = OutgoingSender::new(
            42,
            {
                // Create a channel that converts OutgoingMessage to raw bytes for Pull
                let (out_tx, mut out_rx) = mpsc::channel::<OutgoingMessage>(10);
                // Spawn a task to bridge OutgoingMessage to raw bytes
                tokio::spawn(async move {
                    while let Some(msg) = out_rx.recv().await {
                        match msg {
                            OutgoingMessage::Data(bytes) => {
                                let _ = tx.send(bytes).await;
                            }
                            OutgoingMessage::Close => break,
                        }
                    }
                });
                out_tx
            },
            notify,
        );

        let push: Push<i32> = Push::new(outgoing_sender);
        let mut pull: Pull<i32> = Pull::new(42, rx);

        assert_eq!(push.stream_id(), 42);
        assert_eq!(pull.stream_id(), 42);

        push.send(&100).await.unwrap();
        push.send(&200).await.unwrap();

        assert_eq!(pull.recv().await.unwrap(), Some(100));
        assert_eq!(pull.recv().await.unwrap(), Some(200));

        drop(push);
        // Give the spawned task time to process the Close message
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        assert_eq!(pull.recv().await.unwrap(), None); // channel closed
    }

    // r[verify streaming.lifecycle.caller-closes-pushes]
    #[tokio::test]
    async fn push_drop_sends_close() {
        let mut registry = StreamRegistry::new();
        let sender = registry.register_outgoing(42);

        let push: Push<i32> = Push::new(sender);

        // Send some data
        push.send(&100).await.unwrap();

        // Drop the push - should trigger Close
        drop(push);

        // Poll should return Data first, then Close
        match registry.poll_outgoing() {
            OutgoingPoll::Data { stream_id, payload } => {
                assert_eq!(stream_id, 42);
                let value: i32 = facet_postcard::from_slice(&payload).unwrap();
                assert_eq!(value, 100);
            }
            other => panic!("expected Data, got {:?}", other),
        }

        match registry.poll_outgoing() {
            OutgoingPoll::Close { stream_id } => {
                assert_eq!(stream_id, 42);
            }
            other => panic!("expected Close, got {:?}", other),
        }
    }

    // r[verify streaming.data-after-close]
    #[tokio::test]
    async fn data_after_close_is_rejected() {
        let mut registry = StreamRegistry::new();
        let _rx = registry.register_incoming(42);

        // Close the stream
        registry.close(42);

        // Data after close should fail
        let result = registry.route_data(42, b"data".to_vec()).await;
        assert_eq!(result, Err(StreamError::DataAfterClose));
    }

    // r[verify streaming.data]
    // r[verify streaming.unknown]
    #[tokio::test]
    async fn stream_registry_routes_data_to_registered_stream() {
        let mut registry = StreamRegistry::new();

        // Register a stream
        let mut rx = registry.register_incoming(42);

        // Data to registered stream should succeed
        assert!(registry.route_data(42, b"hello".to_vec()).await.is_ok());

        // Should receive the data
        assert_eq!(rx.recv().await, Some(b"hello".to_vec()));

        // Data to unregistered stream should fail
        assert!(registry.route_data(999, b"nope".to_vec()).await.is_err());
    }

    // r[verify streaming.close]
    #[tokio::test]
    async fn stream_registry_close_terminates_stream() {
        let mut registry = StreamRegistry::new();
        let mut rx = registry.register_incoming(42);

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
}
