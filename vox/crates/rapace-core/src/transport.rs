//! Transport enum and internal backend trait.
//!
//! The public API is the [`Transport`] enum. Each backend lives in its own
//! module under `transport/` and implements the internal [`TransportBackend`]
//! trait.

use crate::{BufferPool, Frame, TransportError};

pub(crate) trait TransportBackend: Send + Sync + Clone + 'static {
    async fn send_frame(&self, frame: Frame) -> Result<(), TransportError>;
    async fn recv_frame(&self) -> Result<Frame, TransportError>;
    fn close(&self);
    fn is_closed(&self) -> bool;
    fn buffer_pool(&self) -> &BufferPool;
}

#[derive(Clone, Debug)]
pub enum Transport {
    #[cfg(feature = "mem")]
    Mem(mem::MemTransport),
    #[cfg(all(feature = "stream", not(target_arch = "wasm32")))]
    Stream(stream::StreamTransport),
    #[cfg(all(feature = "shm", not(target_arch = "wasm32")))]
    Shm(shm::ShmTransport),
    #[cfg(feature = "websocket")]
    WebSocket(websocket::WebSocketTransport),
}

impl Transport {
    pub async fn send_frame(&self, frame: Frame) -> Result<(), TransportError> {
        match self {
            #[cfg(feature = "mem")]
            Transport::Mem(t) => t.send_frame(frame).await,
            #[cfg(all(feature = "stream", not(target_arch = "wasm32")))]
            Transport::Stream(t) => t.send_frame(frame).await,
            #[cfg(all(feature = "shm", not(target_arch = "wasm32")))]
            Transport::Shm(t) => t.send_frame(frame).await,
            #[cfg(feature = "websocket")]
            Transport::WebSocket(t) => t.send_frame(frame).await,
        }
    }

    pub async fn recv_frame(&self) -> Result<Frame, TransportError> {
        match self {
            #[cfg(feature = "mem")]
            Transport::Mem(t) => t.recv_frame().await,
            #[cfg(all(feature = "stream", not(target_arch = "wasm32")))]
            Transport::Stream(t) => t.recv_frame().await,
            #[cfg(all(feature = "shm", not(target_arch = "wasm32")))]
            Transport::Shm(t) => t.recv_frame().await,
            #[cfg(feature = "websocket")]
            Transport::WebSocket(t) => t.recv_frame().await,
        }
    }

    pub fn close(&self) {
        match self {
            #[cfg(feature = "mem")]
            Transport::Mem(t) => t.close(),
            #[cfg(all(feature = "stream", not(target_arch = "wasm32")))]
            Transport::Stream(t) => t.close(),
            #[cfg(all(feature = "shm", not(target_arch = "wasm32")))]
            Transport::Shm(t) => t.close(),
            #[cfg(feature = "websocket")]
            Transport::WebSocket(t) => t.close(),
        }
    }

    pub fn is_closed(&self) -> bool {
        match self {
            #[cfg(feature = "mem")]
            Transport::Mem(t) => t.is_closed(),
            #[cfg(all(feature = "stream", not(target_arch = "wasm32")))]
            Transport::Stream(t) => t.is_closed(),
            #[cfg(all(feature = "shm", not(target_arch = "wasm32")))]
            Transport::Shm(t) => t.is_closed(),
            #[cfg(feature = "websocket")]
            Transport::WebSocket(t) => t.is_closed(),
        }
    }

    /// Get the buffer pool for this transport.
    ///
    /// This pool is used for optimized serialization and deserialization,
    /// avoiding allocations by reusing buffers across multiple RPCs.
    pub fn buffer_pool(&self) -> &BufferPool {
        match self {
            #[cfg(feature = "mem")]
            Transport::Mem(t) => t.buffer_pool(),
            #[cfg(all(feature = "stream", not(target_arch = "wasm32")))]
            Transport::Stream(t) => t.buffer_pool(),
            #[cfg(all(feature = "shm", not(target_arch = "wasm32")))]
            Transport::Shm(t) => t.buffer_pool(),
            #[cfg(feature = "websocket")]
            Transport::WebSocket(t) => t.buffer_pool(),
        }
    }

    #[cfg(feature = "mem")]
    pub fn mem_pair() -> (Self, Self) {
        let (a, b) = mem::MemTransport::pair();
        (Transport::Mem(a), Transport::Mem(b))
    }

    #[cfg(all(feature = "shm", not(target_arch = "wasm32")))]
    pub fn shm(session: std::sync::Arc<shm::ShmSession>) -> Self {
        Transport::Shm(shm::ShmTransport::new(session))
    }

    #[cfg(all(feature = "shm", not(target_arch = "wasm32")))]
    pub fn shm_pair() -> (Self, Self) {
        let (a, b) = shm::ShmTransport::pair().expect("failed to create shm transport pair");
        (Transport::Shm(a), Transport::Shm(b))
    }

    #[cfg(all(feature = "stream", not(target_arch = "wasm32")))]
    pub fn stream<S>(stream: S) -> Self
    where
        S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + Sync + 'static,
    {
        Transport::Stream(stream::StreamTransport::new(stream))
    }

    #[cfg(all(feature = "stream", not(target_arch = "wasm32")))]
    pub fn stream_pair() -> (Self, Self) {
        let (a, b) = stream::StreamTransport::pair();
        (Transport::Stream(a), Transport::Stream(b))
    }

    #[cfg(all(feature = "websocket", not(target_arch = "wasm32")))]
    pub fn websocket<S>(ws: tokio_tungstenite::WebSocketStream<S>) -> Self
    where
        S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
    {
        Transport::WebSocket(websocket::WebSocketTransport::new(ws))
    }

    #[cfg(all(feature = "websocket", not(target_arch = "wasm32")))]
    pub async fn websocket_pair() -> (Self, Self) {
        let (a, b) = websocket::WebSocketTransport::pair().await;
        (Transport::WebSocket(a), Transport::WebSocket(b))
    }

    #[cfg(all(feature = "websocket-axum", not(target_arch = "wasm32")))]
    pub fn websocket_axum(ws: axum::extract::ws::WebSocket) -> Self {
        Transport::WebSocket(websocket::WebSocketTransport::from_axum(ws))
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
