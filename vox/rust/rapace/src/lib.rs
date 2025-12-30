#![doc = include_str!("../README.md")]
#![forbid(unsafe_op_in_unsafe_fn)]
#![deny(unsafe_code)]

// Macro hygiene: Allow `::rapace::` paths to work both externally and internally.
// When used in demos/tests within this crate, `::rapace::` would normally
// fail because it would look for a `rapace` module within `rapace`. This
// self-referential module makes `::rapace::rapace_core` etc. work everywhere.
#[doc(hidden)]
pub mod rapace {
    pub use crate::*;
}

// Re-export the service macro
pub use rapace_macros::service;

// Re-export rapace_core for macro-generated code
// The macro generates `::rapace_core::` paths, so users need this
#[doc(hidden)]
pub extern crate rapace_core;

// Re-export core types
pub use rapace_core::{
    // Transport types (for advanced use)
    AnyTransport,
    // Buffer pooling (for optimization)
    BufferPool,
    // Error types
    DecodeError,
    EncodeError,
    ErrorCode,
    // Frame types (for advanced use)
    Frame,
    FrameFlags,
    MsgDescHot,
    PooledBuf,
    RpcError,
    RpcSession,
    // Streaming
    Streaming,
    Transport,
    TransportError,
    ValidationError,
    // Error payload parsing
    parse_error_payload,
};

/// Convenience type alias for a type-erased RPC session.
///
/// This is equivalent to `RpcSession<AnyTransport>` and provides runtime polymorphism
/// when you need to handle multiple transport types dynamically.
///
/// # When to Use
///
/// - **Use this** when you need runtime flexibility (e.g., supporting multiple transports)
/// - **Use `RpcSession<ConcreteTransport>`** when the transport type is known at compile time
///   for zero-cost abstraction and dead code elimination
///
/// # Example
///
/// ```ignore
/// use rapace::{Session, AnyTransport};
/// use std::sync::Arc;
///
/// // Type-erased session (runtime polymorphism)
/// let session = Arc::new(Session::new(AnyTransport::new(transport)));
///
/// // Or be explicit with the type
/// let session: Arc<RpcSession<AnyTransport>> = Arc::new(RpcSession::new(AnyTransport::new(transport)));
/// ```
pub type Session = RpcSession<AnyTransport>;

// Tunnels are not supported on wasm.
#[cfg(not(target_arch = "wasm32"))]
pub use rapace_core::{TunnelHandle, TunnelStream};

// Re-export serialization crates for macro-generated code
// The macro generates `::rapace::facet_core::` etc paths, so we need extern crate
pub use facet;
#[doc(hidden)]
pub extern crate facet_core;
pub use facet_postcard;

/// Serialize a value to postcard bytes, with Display error on panic.
///
/// This is a wrapper around `facet_postcard::to_vec` that provides better
/// error messages by using Display instead of Debug when panicking.
#[track_caller]
pub fn postcard_to_vec<T: facet::Facet<'static>>(value: &T) -> Vec<u8> {
    facet_postcard::to_vec(value)
        .unwrap_or_else(|e| panic!("failed to serialize to postcard: {}", e))
}

// Re-export tracing for macro-generated code
#[doc(hidden)]
pub extern crate tracing;

// Re-export futures-util so macro-generated code can rely on a stable path.
// We alias it as `futures` for backward compatibility with existing macro-generated code.
#[doc(hidden)]
pub extern crate futures_util as futures;

// Re-export registry
pub use rapace_registry as registry;

/// Prelude module for convenient imports.
///
/// ```ignore
/// use rapace::prelude::*;
/// ```
pub mod prelude {
    pub use crate::{ErrorCode, RpcError, Streaming, Transport, service};

    // Re-export facet for derive macros in service types
    pub use facet::Facet;

    // Re-export registry types for multi-service scenarios
    pub use rapace_registry::ServiceRegistry;
}

/// Transport implementations.
///
/// Each transport is behind a feature flag. Enable the ones you need:
///
/// ```toml
/// [dependencies]
/// rapace = { version = "0.1", features = ["mem", "stream"] }
/// ```
pub mod transport {
    #[cfg(feature = "mem")]
    pub use rapace_core::mem::MemTransport;

    #[cfg(feature = "stream")]
    pub use rapace_core::stream::StreamTransport;

    #[cfg(feature = "websocket")]
    pub use rapace_core::websocket::WebSocketTransport;

    // Note: SHM transport requires more setup, exposed separately
    #[cfg(feature = "shm")]
    pub mod shm {
        pub use rapace_core::shm::*;
    }
}

#[doc(hidden)]
pub mod helper_binary;
/// Session layer for flow control and channel management.
pub mod session;

#[cfg(feature = "mem")]
pub use transport::MemTransport;

#[cfg(feature = "stream")]
pub use transport::StreamTransport;

#[cfg(feature = "websocket")]
pub use transport::WebSocketTransport;

/// Server helpers for running RPC services.
///
/// This module provides convenience functions for setting up servers
/// with various transports.
#[cfg(feature = "stream")]
pub mod server {
    use std::sync::Arc;
    use tokio::net::{TcpListener, TcpStream};

    /// Serve a single TCP connection.
    ///
    /// This is a low-level helper that wraps a TCP stream in a `StreamTransport`
    /// and is intended to be used with a generated server's `serve()` method.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use rapace::server::serve_connection;
    ///
    /// let listener = TcpListener::bind("127.0.0.1:9000").await?;
    /// loop {
    ///     let (socket, _) = listener.accept().await?;
    ///     let server = CalculatorServer::new(CalculatorImpl);
    ///     tokio::spawn(async move {
    ///         let transport = serve_connection(socket);
    ///         server.serve(transport).await
    ///     });
    /// }
    /// ```
    pub fn serve_connection(stream: TcpStream) -> Arc<crate::StreamTransport> {
        Arc::new(crate::StreamTransport::new(stream))
    }

    /// Run a TCP server that accepts connections and spawns a handler for each.
    ///
    /// This is a convenience function that ties together a TCP listener,
    /// transport creation, and server spawning.
    ///
    /// # Arguments
    ///
    /// * `addr` - The address to bind to (e.g., "127.0.0.1:9000")
    /// * `make_server` - A function that creates a new server instance for each connection
    ///
    /// # Example
    ///
    /// ```ignore
    /// use rapace::server::run_tcp_server;
    ///
    /// run_tcp_server("127.0.0.1:9000", || {
    ///     CalculatorServer::new(CalculatorImpl)
    /// }).await?;
    /// ```
    pub async fn run_tcp_server<S, F>(addr: &str, make_server: F) -> Result<(), std::io::Error>
    where
        F: Fn() -> S + Send + Sync + 'static,
        S: TcpServable + Send + 'static,
    {
        let listener = TcpListener::bind(addr).await?;
        println!("Listening on {}", addr);

        loop {
            let (socket, peer_addr) = listener.accept().await?;
            println!("Accepted connection from {}", peer_addr);

            let server = make_server();
            tokio::spawn(async move {
                let transport = serve_connection(socket);
                if let Err(e) = server.serve_tcp(transport).await {
                    eprintln!("Connection error from {}: {}", peer_addr, e);
                }
            });
        }
    }

    /// Trait for servers that can serve over TCP.
    ///
    /// This is implemented by all generated servers and allows `run_tcp_server`
    /// to be generic over any service type.
    pub trait TcpServable {
        /// Serve requests from the TCP transport until the connection closes.
        fn serve_tcp(
            self,
            transport: Arc<crate::StreamTransport>,
        ) -> impl std::future::Future<Output = Result<(), crate::RpcError>> + Send;
    }
}

/// A writer that tries to write to a pooled buffer, but transparently falls back
/// to heap allocation if the buffer is too small.
///
/// This enables single-pass serialization with automatic overflow handling.
struct PooledWriter {
    /// The pooled buffer we're writing to
    buf: rapace_core::PooledBuf,
    /// If we overflow the pooled buffer, we allocate a Vec and switch to that
    overflow: Option<Vec<u8>>,
}

impl PooledWriter {
    fn new(buf: rapace_core::PooledBuf) -> Self {
        // Debug assertion: pooled buffer should have capacity
        debug_assert!(
            buf.capacity() > 0,
            "PooledBuf should have non-zero capacity, got {}",
            buf.capacity()
        );
        Self {
            buf,
            overflow: None,
        }
    }

    /// Convert this writer into a PooledBuf, handling overflow if needed
    fn into_pooled_buf(self, pool: &rapace_core::BufferPool) -> rapace_core::PooledBuf {
        if let Some(overflow) = self.overflow {
            // We overflowed - wrap the Vec directly without copying
            rapace_core::PooledBuf::from_vec_unpooled(overflow, pool.buffer_size())
        } else {
            // No overflow - return the original buffer
            self.buf
        }
    }
}

impl facet_postcard::Writer for PooledWriter {
    fn write_byte(&mut self, byte: u8) -> Result<(), facet_postcard::SerializeError> {
        if let Some(overflow) = &mut self.overflow {
            // Already overflowed - write to Vec
            overflow.push(byte);
            Ok(())
        } else {
            // Try to write to pooled buffer
            let capacity = self.buf.capacity();
            if self.buf.len() < capacity {
                self.buf.push(byte);
                Ok(())
            } else {
                // Overflow! Switch to Vec
                tracing::debug!(
                    capacity_bytes = capacity,
                    "PooledBuf capacity exceeded during serialization, falling back to heap allocation"
                );
                let mut overflow = Vec::with_capacity(capacity * 2);
                overflow.extend_from_slice(&self.buf);
                overflow.push(byte);
                self.overflow = Some(overflow);
                Ok(())
            }
        }
    }

    fn write_bytes(&mut self, bytes: &[u8]) -> Result<(), facet_postcard::SerializeError> {
        if let Some(overflow) = &mut self.overflow {
            // Already overflowed - write to Vec
            overflow.extend_from_slice(bytes);
            Ok(())
        } else {
            // Try to write to pooled buffer
            let capacity = self.buf.capacity();
            let needed = self.buf.len() + bytes.len();
            if needed <= capacity {
                self.buf.extend_from_slice(bytes);
                Ok(())
            } else {
                // Overflow! Switch to Vec
                tracing::debug!(
                    capacity_bytes = capacity,
                    needed_bytes = needed,
                    "PooledBuf capacity exceeded during serialization, falling back to heap allocation. \
                     Consider creating a larger BufferPool with BufferPool::with_capacity(count, {}).",
                    needed.max(capacity * 2).next_power_of_two()
                );
                let mut overflow = Vec::with_capacity(needed.max(capacity * 2));
                overflow.extend_from_slice(&self.buf);
                overflow.extend_from_slice(bytes);
                self.overflow = Some(overflow);
                Ok(())
            }
        }
    }
}

/// Serialize a value to postcard bytes using a pooled buffer.
///
/// This reduces allocation pressure by reusing buffers from the provided pool.
/// The returned `PooledBuf` automatically returns to the pool when dropped.
///
/// # Performance
///
/// This function serializes directly into a pooled buffer using a custom writer that
/// can transparently fall back to heap allocation if the buffer is too small. This
/// provides a single-pass serialization that optimizes for the common case (payload
/// fits in pooled buffer) while gracefully handling oversized payloads.
///
/// # Example
///
/// ```ignore
/// use rapace::{postcard_to_pooled_buf, rapace_core::BufferPool};
/// use facet::Facet;
///
/// #[derive(Facet)]
/// struct Request { id: u32, data: Vec<u8> }
///
/// let pool = BufferPool::new();
/// let req = Request { id: 42, data: vec![1, 2, 3] };
/// let buf = postcard_to_pooled_buf(&pool, &req)?;
/// # Ok::<_, rapace_core::EncodeError>(())
/// ```
pub fn postcard_to_pooled_buf<T: facet::Facet<'static>>(
    pool: &rapace_core::BufferPool,
    value: &T,
) -> Result<rapace_core::PooledBuf, rapace_core::EncodeError> {
    // Create a pooled writer that can transparently overflow to heap
    let buf = pool.get();
    let mut writer = PooledWriter::new(buf);

    // Serialize using the custom writer - single pass!
    facet_postcard::to_writer_fallible(value, &mut writer)?;

    // Convert the writer back into a PooledBuf
    Ok(writer.into_pooled_buf(pool))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_postcard_to_pooled_buf_oversized_payload() {
        use facet::Facet;

        #[derive(Facet)]
        struct LargePayload {
            // Large payload data; in this test we use 16KB, larger than the 8KB pool buffer
            data: Vec<u8>,
        }

        // Create a small buffer pool (8KB buffers)
        let pool = BufferPool::with_capacity(4, 8 * 1024);

        // Create a payload larger than the buffer size (16KB)
        let payload = LargePayload {
            data: vec![42u8; 16 * 1024],
        };

        // This should succeed with the auto-fallback mechanism
        let result = postcard_to_pooled_buf(&pool, &payload);
        assert!(
            result.is_ok(),
            "Should successfully serialize oversized payload"
        );

        let buf = result.unwrap();
        // Verify we got the data
        assert!(
            buf.len() > 8 * 1024,
            "Buffer should contain the large payload"
        );

        // Verify we can deserialize it back
        let deserialized: LargePayload = facet_postcard::from_slice(&buf).unwrap();
        assert_eq!(deserialized.data.len(), 16 * 1024);
        assert_eq!(deserialized.data[0], 42);
    }

    #[test]
    fn test_postcard_to_pooled_buf_normal_payload() {
        use facet::Facet;

        #[derive(Facet)]
        struct SmallPayload {
            id: u32,
            data: Vec<u8>,
        }

        let pool = BufferPool::new();

        let payload = SmallPayload {
            id: 123,
            data: vec![1, 2, 3, 4, 5],
        };

        // This should succeed normally without fallback
        let result = postcard_to_pooled_buf(&pool, &payload);
        assert!(result.is_ok());

        let buf = result.unwrap();
        // Verify we can deserialize it back
        let deserialized: SmallPayload = facet_postcard::from_slice(&buf).unwrap();
        assert_eq!(deserialized.id, 123);
        assert_eq!(deserialized.data, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn test_postcard_to_pooled_buf_uses_unpooled_for_overflow() {
        use facet::Facet;

        #[derive(Facet)]
        struct LargePayload {
            data: Vec<u8>,
        }

        // Create a small buffer pool (8KB buffers)
        let pool = BufferPool::with_capacity(4, 8 * 1024);

        // Create a payload larger than the buffer size (16KB)
        let payload = LargePayload {
            data: vec![42u8; 16 * 1024],
        };

        // Serialize the oversized payload
        let result = postcard_to_pooled_buf(&pool, &payload);
        assert!(
            result.is_ok(),
            "Should successfully serialize oversized payload"
        );

        let buf = result.unwrap();

        // Verify the buffer is unpooled (since it overflowed)
        assert!(
            !buf.is_pooled(),
            "Oversized payload should use unpooled storage"
        );

        // Verify we got the data
        assert!(
            buf.len() > 8 * 1024,
            "Buffer should contain the large payload"
        );

        // Verify we can deserialize it back
        let deserialized: LargePayload = facet_postcard::from_slice(&buf).unwrap();
        assert_eq!(deserialized.data.len(), 16 * 1024);
        assert_eq!(deserialized.data[0], 42);
    }
}
