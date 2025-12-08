//! Transport traits.

use std::future::Future;
use std::pin::Pin;

use crate::{EncodeCtx, Frame, FrameView, TransportError};

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
/// within a channel, and must uphold the lifetime guarantees implied by FrameView.
pub trait Transport: Send + Sync {
    /// Send a frame to the peer.
    ///
    /// The frame is borrowed for the duration of the call. The transport
    /// may copy it (stream), reference it (in-proc), or encode it into
    /// SHM slots depending on implementation.
    fn send_frame(&self, frame: &Frame) -> impl Future<Output = Result<(), TransportError>> + Send;

    /// Receive the next frame from the peer.
    ///
    /// Returns a FrameView with lifetime tied to internal buffers.
    /// Caller must process or copy before calling recv_frame again.
    fn recv_frame(&self) -> impl Future<Output = Result<FrameView<'_>, TransportError>> + Send;

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

    /// Receive a frame (returns owned Frame, not FrameView).
    fn recv_frame_boxed(&self) -> BoxFuture<'_, Result<Frame, TransportError>>;

    /// Create an encoder context.
    fn encoder_boxed(&self) -> Box<dyn EncodeCtx + '_>;

    /// Graceful shutdown (boxed future version).
    fn close_boxed(&self) -> BoxFuture<'_, Result<(), TransportError>>;
}

// Note: Blanket impl for DynTransport requires concrete types due to
// lifetime issues with FrameView. Transports implement DynTransport directly
// when needed.
