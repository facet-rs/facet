//! Transport traits.

use std::future::Future;
use std::ops::Deref;
use std::pin::Pin;

use crate::{EncodeCtx, Frame, RecvFrame, TransportError};

/// A transport moves frames between two peers.
///
/// Transports are responsible for:
/// - Frame serialization/deserialization
/// - Flow control at the transport level
/// - Delivering frames reliably (within a session)
///
/// Transports are NOT responsible for:
/// - RPC semantics (channels, methods, deadlines)
/// - Service dispatch
/// - Schema management
///
/// Invariant: A transport may buffer internally, but must not reorder frames
/// within a channel.
pub trait Transport: Send + Sync {
    /// The payload handle type for received frames.
    ///
    /// This is transport-specific:
    /// - Non-SHM transports use a pooled buffer (returns to pool on drop)
    /// - SHM transport uses a slot guard (frees slot on drop)
    type RecvPayload: Deref<Target = [u8]> + Send + 'static;

    /// Send a frame to the peer.
    ///
    /// The frame is borrowed for the duration of the call. The transport
    /// may copy it (stream), reference it (in-proc), or encode it into
    /// SHM slots depending on implementation.
    fn send_frame(&self, frame: &Frame) -> impl Future<Output = Result<(), TransportError>> + Send;

    /// Receive the next frame from the peer.
    ///
    /// Returns a `RecvFrame` with owned descriptor and a payload handle.
    /// The payload handle releases resources (pool buffer or SHM slot) on drop.
    fn recv_frame(
        &self,
    ) -> impl Future<Output = Result<RecvFrame<Self::RecvPayload>, TransportError>> + Send;

    /// Create an encoder context for building outbound frames.
    ///
    /// The encoder is transport-specific: SHM encoders can reference
    /// existing SHM data; stream encoders always copy.
    fn encoder(&self) -> Box<dyn EncodeCtx + '_>;

    /// Graceful shutdown.
    fn close(&self) -> impl Future<Output = Result<(), TransportError>> + Send;
}

/// Boxed future type for object-safe transport.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Object-safe version of Transport for dynamic dispatch.
///
/// Use this when you need to store transports in a collection or
/// pass them through trait objects.
pub trait DynTransport: Send + Sync {
    /// Send a frame (boxed future version).
    fn send_frame_boxed(&self, frame: &Frame) -> BoxFuture<'_, Result<(), TransportError>>;

    /// Receive a frame (returns owned Frame).
    fn recv_frame_boxed(&self) -> BoxFuture<'_, Result<Frame, TransportError>>;

    /// Create an encoder context.
    fn encoder_boxed(&self) -> Box<dyn EncodeCtx + '_>;

    /// Graceful shutdown (boxed future version).
    fn close_boxed(&self) -> BoxFuture<'_, Result<(), TransportError>>;
}

// Note: A blanket impl for DynTransport would be nice but has lifetime issues
// with `frame: &Frame` in send_frame_boxed. Transports implement DynTransport
// directly when needed.
