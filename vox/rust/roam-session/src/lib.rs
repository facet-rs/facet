#![deny(unsafe_code)]

//! Session/state machine and RPC-level utilities.
//!
//! Canonical definitions live in `docs/content/spec/_index.md`,
//! `docs/content/rust-spec/_index.md`, and `docs/content/shm-spec/_index.md`.

use std::sync::atomic::{AtomicU64, Ordering};

use facet::Facet;
use tokio::sync::mpsc;

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

/// Push stream handle - caller sends data to callee.
///
/// r[impl streaming.caller-pov] - From caller's perspective, Push means "I send".
/// r[impl streaming.type] - Serializes as u64 stream ID on wire.
///
/// When dropped, the channel closes and the connection layer sends a Close message.
pub struct Push<T> {
    stream_id: StreamId,
    tx: mpsc::Sender<T>,
}

impl<T> Push<T> {
    /// Create a new Push stream with the given ID and sender channel.
    pub fn new(stream_id: StreamId, tx: mpsc::Sender<T>) -> Self {
        Self { stream_id, tx }
    }

    /// Get the stream ID.
    pub fn stream_id(&self) -> StreamId {
        self.stream_id
    }

    /// Send a value on this stream.
    pub async fn send(&self, value: T) -> Result<(), mpsc::error::SendError<T>> {
        self.tx.send(value).await
    }
}

/// Pull stream handle - caller receives data from callee.
///
/// r[impl streaming.caller-pov] - From caller's perspective, Pull means "I receive".
/// r[impl streaming.type] - Serializes as u64 stream ID on wire.
pub struct Pull<T> {
    stream_id: StreamId,
    rx: mpsc::Receiver<T>,
}

impl<T> Pull<T> {
    /// Create a new Pull stream with the given ID and receiver channel.
    pub fn new(stream_id: StreamId, rx: mpsc::Receiver<T>) -> Self {
        Self { stream_id, rx }
    }

    /// Get the stream ID.
    pub fn stream_id(&self) -> StreamId {
        self.stream_id
    }

    /// Receive the next value from this stream.
    ///
    /// Returns `None` when the stream is closed.
    pub async fn recv(&mut self) -> Option<T> {
        self.rx.recv().await
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

// TODO: Remove this shim once facet implements `Facet` for `core::convert::Infallible`
// and for the never type `!` (facet-rs/facet#1668), then use `Infallible`.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct Never;

/// Call error type encoded in unary responses.
///
/// r\[impl unary.response.encoding\] - Response is `Result<T, RoamError<E>>`
/// r\[impl unary.error.roam-error\] - Protocol errors use RoamError variants
///
/// Spec: `docs/content/spec/_index.md` "RoamError".
#[repr(u8)]
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub enum RoamError<E> {
    User(E) = 0,
    UnknownMethod = 1,
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

    #[tokio::test]
    async fn push_pull_channel_roundtrip() {
        let (tx, rx) = mpsc::channel::<i32>(10);
        let push = Push::new(42, tx);
        let mut pull = Pull::new(42, rx);

        assert_eq!(push.stream_id(), 42);
        assert_eq!(pull.stream_id(), 42);

        push.send(100).await.unwrap();
        push.send(200).await.unwrap();

        assert_eq!(pull.recv().await, Some(100));
        assert_eq!(pull.recv().await, Some(200));

        drop(push);
        assert_eq!(pull.recv().await, None); // channel closed
    }
}
