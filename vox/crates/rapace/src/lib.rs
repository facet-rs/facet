#![doc = include_str!("../README.md")]
#![forbid(unsafe_op_in_unsafe_fn)]

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
    // Transport types (for advanced use)
    Transport,
    TransportError,
    ValidationError,
    // Error payload parsing
    parse_error_payload,
};

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

// Re-export futures so macro-generated code can rely on a stable path.
#[doc(hidden)]
pub extern crate futures;

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

/// Serialize a value to postcard bytes using a pooled buffer.
///
/// This reduces allocation pressure by reusing buffers from the provided pool.
/// The returned `PooledBuf` automatically returns to the pool when dropped.
///
/// # Performance
///
/// This function serializes directly into a pooled buffer using `facet_postcard::to_slice`,
/// avoiding the intermediate Vec allocation that `to_vec` requires. For high-throughput
/// RPC scenarios, this significantly reduces allocator pressure.
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
    let mut buf = pool.get();

    // Ensure the buffer has capacity for serialization
    // We use the pool's buffer size as initial capacity
    buf.resize(pool.buffer_size(), 0);

    // Serialize directly into the buffer
    let used = match facet_postcard::to_slice(value, &mut buf) {
        Ok(size) => size,
        Err(e) => {
            // Check if it's a buffer size error by examining the error message
            let err_msg = e.to_string();
            if err_msg.contains("too small") || err_msg.contains("Buffer too small") {
                // Fallback: serialize to Vec to get the exact size needed
                let vec = facet_postcard::to_vec(value)?;
                let needed = vec.len();
                let available = buf.len();

                tracing::warn!(
                    needed_bytes = needed,
                    available_bytes = available,
                    "BufferPool buffer too small for payload, falling back to allocation. \
                     Consider creating a larger BufferPool with BufferPool::with_capacity(count, {}).",
                    needed.next_power_of_two().max(available * 2)
                );

                // Copy the serialized data into our pooled buffer
                buf.clear();
                buf.extend_from_slice(&vec);
                vec.len()
            } else {
                // Some other serialization error
                return Err(e.into());
            }
        }
    };

    // Trim to actual size
    buf.truncate(used);

    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_postcard_to_pooled_buf_oversized_payload() {
        use facet::Facet;

        #[derive(Facet)]
        struct LargePayload {
            // 80KB of data (larger than default 64KB buffer)
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
}
