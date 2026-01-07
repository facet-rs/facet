#![deny(unsafe_code)]

//! Session/state machine and RPC-level utilities.
//!
//! Canonical definitions live in `docs/content/spec/_index.md`,
//! `docs/content/rust-spec/_index.md`, and `docs/content/shm-spec/_index.md`.

use std::sync::atomic::{AtomicU64, Ordering};

use facet::Facet;

pub use roam_frame::{Frame, MsgDesc, OwnedMessage, Payload};

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
