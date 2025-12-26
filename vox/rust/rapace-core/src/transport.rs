//! Transport trait and type-erased wrapper.
//!
//! The [`Transport`] trait defines the interface for all transport implementations.
//! Each backend lives in its own module under `transport/` and implements this trait.
//!
//! For type erasure when you need to handle multiple transport types dynamically,
//! use [`AnyTransport`].

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use crate::{BufferPool, Frame, TransportError};

/// Trait for transport implementations.
///
/// This trait uses RPITIT (return position impl trait in trait) for async methods,
/// enabling zero-cost abstraction when the concrete transport type is known at compile time.
///
/// # Example
///
/// ```ignore
/// // Monomorphized - no dynamic dispatch overhead
/// async fn serve<T: Transport>(transport: T) {
///     loop {
///         let frame = transport.recv_frame().await?;
///         // ...
///     }
/// }
///
/// // Or use AnyTransport when you need type erasure
/// let transport = AnyTransport::new(ShmTransport::new(session));
/// ```
pub trait Transport: Send + Sync + Clone + 'static {
    /// Send a frame over this transport.
    fn send_frame(
        &self,
        frame: Frame,
    ) -> impl Future<Output = Result<(), TransportError>> + Send + '_;

    /// Receive a frame from this transport.
    fn recv_frame(&self) -> impl Future<Output = Result<Frame, TransportError>> + Send + '_;

    /// Close this transport.
    ///
    /// After closing, `send_frame` and `recv_frame` will return `TransportError::Closed`.
    fn close(&self);

    /// Check if this transport is closed.
    fn is_closed(&self) -> bool;

    /// Get the buffer pool for this transport.
    ///
    /// The buffer pool is used for optimized serialization/deserialization,
    /// avoiding allocations by reusing buffers across multiple RPCs.
    fn buffer_pool(&self) -> &BufferPool;
}

/// Object-safe version of [`Transport`] for dynamic dispatch.
///
/// This trait boxes the async methods, enabling use with `dyn DynTransport`.
/// Use [`AnyTransport`] as a convenient wrapper.
pub trait DynTransport: Send + Sync + 'static {
    /// Send a frame (boxed future for object safety).
    fn send_frame_dyn(
        &self,
        frame: Frame,
    ) -> Pin<Box<dyn Future<Output = Result<(), TransportError>> + Send + '_>>;

    /// Receive a frame (boxed future for object safety).
    fn recv_frame_dyn(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<Frame, TransportError>> + Send + '_>>;

    /// Close this transport.
    fn close(&self);

    /// Check if this transport is closed.
    fn is_closed(&self) -> bool;

    /// Get the buffer pool.
    fn buffer_pool(&self) -> &BufferPool;

    /// Clone into a boxed trait object.
    fn clone_box(&self) -> Box<dyn DynTransport>;
}

/// Blanket impl: any `Transport` can be used as `DynTransport`.
impl<T: Transport> DynTransport for T {
    fn send_frame_dyn(
        &self,
        frame: Frame,
    ) -> Pin<Box<dyn Future<Output = Result<(), TransportError>> + Send + '_>> {
        Box::pin(self.send_frame(frame))
    }

    fn recv_frame_dyn(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<Frame, TransportError>> + Send + '_>> {
        Box::pin(self.recv_frame())
    }

    fn close(&self) {
        Transport::close(self)
    }

    fn is_closed(&self) -> bool {
        Transport::is_closed(self)
    }

    fn buffer_pool(&self) -> &BufferPool {
        Transport::buffer_pool(self)
    }

    fn clone_box(&self) -> Box<dyn DynTransport> {
        Box::new(self.clone())
    }
}

/// Type-erased transport wrapper.
///
/// This wraps any [`Transport`] implementation in an `Arc<dyn DynTransport>`,
/// providing a single concrete type that can be used when you need to handle
/// multiple transport types dynamically.
///
/// # Example
///
/// ```ignore
/// use rapace::{AnyTransport, Transport};
///
/// // Create from any concrete transport
/// let shm = ShmTransport::new(session);
/// let transport = AnyTransport::new(shm);
///
/// // Or use helper constructors
/// let transport = AnyTransport::shm(session);
/// let (transport_a, transport_b) = AnyTransport::mem_pair();
///
/// // Use like any other transport
/// transport.send_frame(frame).await?;
/// ```
///
/// # Performance
///
/// `AnyTransport` adds one level of dynamic dispatch (vtable lookup) compared
/// to using a concrete transport type directly. For most RPC workloads, this
/// overhead is negligible compared to actual I/O and serialization costs.
///
/// If you need maximum performance and know the transport type at compile time,
/// use the concrete type directly with generic code.
#[derive(Clone)]
pub struct AnyTransport {
    inner: Arc<dyn DynTransport>,
}

impl std::fmt::Debug for AnyTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AnyTransport")
            .field("is_closed", &self.inner.is_closed())
            .finish_non_exhaustive()
    }
}

impl AnyTransport {
    /// Create a new type-erased transport from any [`Transport`] implementation.
    pub fn new<T: Transport>(transport: T) -> Self {
        Self {
            inner: Arc::new(transport),
        }
    }

    /// Send a frame over this transport.
    pub async fn send_frame(&self, frame: Frame) -> Result<(), TransportError> {
        self.inner.send_frame_dyn(frame).await
    }

    /// Receive a frame from this transport.
    pub async fn recv_frame(&self) -> Result<Frame, TransportError> {
        self.inner.recv_frame_dyn().await
    }

    /// Close this transport.
    pub fn close(&self) {
        self.inner.close()
    }

    /// Check if this transport is closed.
    pub fn is_closed(&self) -> bool {
        self.inner.is_closed()
    }

    /// Get the buffer pool for this transport.
    pub fn buffer_pool(&self) -> &BufferPool {
        self.inner.buffer_pool()
    }

    // ========================================================================
    // Backwards-compatible constructors
    // ========================================================================

    /// Create a pair of in-memory transports for testing.
    ///
    /// The two transports are connected: data sent on one is received on the other.
    #[cfg(feature = "mem")]
    pub fn mem_pair() -> (Self, Self) {
        let (a, b) = mem::MemTransport::pair();
        (Self::new(a), Self::new(b))
    }

    /// Create a transport from a stream (TCP socket, Unix socket, etc.).
    #[cfg(all(feature = "stream", not(target_arch = "wasm32")))]
    pub fn stream<S>(stream: S) -> Self
    where
        S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + Sync + 'static,
    {
        Self::new(stream::StreamTransport::new(stream))
    }

    /// Create a pair of stream transports for testing.
    #[cfg(all(feature = "stream", not(target_arch = "wasm32")))]
    pub fn stream_pair() -> (Self, Self) {
        let (a, b) = stream::StreamTransport::pair();
        (Self::new(a), Self::new(b))
    }

    /// Create a transport from an SHM session.
    ///
    /// Accepts an `Arc<ShmSession>` since `ShmSession::open_file()` and
    /// `ShmSession::create_file()` return `Arc<ShmSession>`.
    #[cfg(all(feature = "shm", not(target_arch = "wasm32")))]
    pub fn shm(session: std::sync::Arc<shm::ShmSession>) -> Self {
        Self::new(shm::ShmTransport::new(session))
    }

    /// Create a transport from a WebSocket stream (native version).
    ///
    /// This accepts any WebSocket stream, whether it's over a raw `TcpStream`
    /// (server-side from `accept_async`) or `MaybeTlsStream<TcpStream>` (client-side).
    #[cfg(all(feature = "websocket", not(target_arch = "wasm32")))]
    pub fn websocket<S>(stream: tokio_tungstenite::WebSocketStream<S>) -> Self
    where
        S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
    {
        Self::new(websocket::WebSocketTransport::new(stream))
    }

    /// Create a transport from a WebSocketTransport (WASM version).
    ///
    /// In WASM, use `WebSocketTransport::connect(url).await` first, then wrap it.
    #[cfg(all(feature = "websocket", target_arch = "wasm32"))]
    pub fn websocket(transport: websocket::WebSocketTransport) -> Self {
        Self::new(transport)
    }
}

/// Implement `Transport` for `AnyTransport` so it can be used generically.
impl Transport for AnyTransport {
    fn send_frame(
        &self,
        frame: Frame,
    ) -> impl Future<Output = Result<(), TransportError>> + Send + '_ {
        self.inner.send_frame_dyn(frame)
    }

    fn recv_frame(&self) -> impl Future<Output = Result<Frame, TransportError>> + Send + '_ {
        self.inner.recv_frame_dyn()
    }

    fn close(&self) {
        self.inner.close()
    }

    fn is_closed(&self) -> bool {
        self.inner.is_closed()
    }

    fn buffer_pool(&self) -> &BufferPool {
        self.inner.buffer_pool()
    }
}

#[cfg(feature = "mem")]
pub mod mem;
#[cfg(all(feature = "shm", not(target_arch = "wasm32")))]
pub mod shm;
#[cfg(all(feature = "stream", not(target_arch = "wasm32")))]
pub mod stream;
#[cfg(feature = "websocket")]
pub mod websocket;
