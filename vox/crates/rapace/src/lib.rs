//! rapace: High-performance RPC framework with shared memory transport.
//!
//! # Quick Start
//!
//! Define a service using the `#[rapace::service]` attribute:
//!
//! ```ignore
//! use rapace::prelude::*;
//!
//! #[rapace::service]
//! trait Calculator {
//!     async fn add(&self, a: i32, b: i32) -> i32;
//!     async fn multiply(&self, a: i32, b: i32) -> i32;
//! }
//!
//! // Server implementation
//! struct CalculatorImpl;
//!
//! impl Calculator for CalculatorImpl {
//!     async fn add(&self, a: i32, b: i32) -> i32 {
//!         a + b
//!     }
//!     async fn multiply(&self, a: i32, b: i32) -> i32 {
//!         a * b
//!     }
//! }
//! ```
//!
//! The macro generates `CalculatorClient<T>` and `CalculatorServer<S>` types.
//!
//! # Sessions
//!
//! Each transport half must be wrapped in an [`RpcSession`] that you own:
//!
//! ```ignore
//! let (client_transport, server_transport) = rapace::InProcTransport::pair();
//! let client_session = Arc::new(rapace::RpcSession::with_channel_start(Arc::new(client_transport), 2));
//! let server_session = Arc::new(rapace::RpcSession::with_channel_start(Arc::new(server_transport), 1));
//! tokio::spawn(client_session.clone().run());
//! tokio::spawn(server_session.clone().run());
//!
//! server_session.set_dispatcher(move |_channel_id, method_id, payload| {
//!     let server = CalculatorServer::new(CalculatorImpl);
//!     Box::pin(async move { server.dispatch(method_id, &payload).await })
//! });
//!
//! let client = CalculatorClient::new(client_session.clone());
//! let result = client.add(2, 3).await?;
//! ```
//!
//! Pick distinct starting channel IDs (odd vs. even) when both peers initiate RPCs on the
//! same transport.
//!
//! # Streaming RPCs
//!
//! For server-streaming RPCs, return `Streaming<T>`:
//!
//! ```ignore
//! use rapace::prelude::*;
//!
//! #[rapace::service]
//! trait Numbers {
//!     async fn range(&self, start: i32, end: i32) -> Streaming<i32>;
//! }
//! ```
//!
//! # Transports
//!
//! rapace supports multiple transports:
//!
//! - **mem** (default): In-memory transport for testing
//! - **stream**: TCP/Unix socket transport
//! - **websocket**: WebSocket transport for browser clients
//! - **shm**: Shared memory transport for maximum performance
//!
//! Enable transports via feature flags:
//!
//! ```toml
//! [dependencies]
//! rapace = { version = "0.1", features = ["stream", "shm"] }
//! ```
//!
//! # Error Handling
//!
//! All RPC methods return `Result<T, RpcError>`. Error codes align with gRPC
//! for familiarity:
//!
//! ```ignore
//! use rapace::prelude::*;
//!
//! match client.add(1, 2).await {
//!     Ok(result) => println!("Result: {}", result),
//!     Err(RpcError::Status { code: ErrorCode::InvalidArgument, message }) => {
//!         eprintln!("Invalid argument: {}", message);
//!     }
//!     Err(e) => eprintln!("RPC failed: {}", e),
//! }
//! ```

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
    // Error payload parsing
    parse_error_payload,
    // Error types
    DecodeError,
    EncodeError,
    ErrorCode,
    // Frame types (for advanced use)
    Frame,
    FrameFlags,
    MsgDescHot,
    RpcError,
    RpcSession,
    // Streaming
    Streaming,
    // Transport trait (for advanced use)
    Transport,
    TransportError,
    ValidationError,
};

// Re-export serialization crates for macro-generated code
// The macro generates `::rapace::facet_core::` etc paths, so we need extern crate
pub use facet;
#[doc(hidden)]
pub extern crate facet_core;
pub use facet_postcard;

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
    pub use crate::{service, ErrorCode, RpcError, Streaming, Transport};

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
    pub use rapace_transport_mem::InProcTransport;

    #[cfg(feature = "stream")]
    pub use rapace_transport_stream::StreamTransport;

    #[cfg(feature = "websocket")]
    pub use rapace_transport_websocket::WebSocketTransport;

    // Note: SHM transport requires more setup, exposed separately
    #[cfg(feature = "shm")]
    pub mod shm {
        pub use rapace_transport_shm::*;
    }
}

/// Session layer for flow control and channel management.
pub mod session;

#[cfg(feature = "mem")]
pub use transport::InProcTransport;

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
    pub fn serve_connection(
        stream: TcpStream,
    ) -> Arc<crate::StreamTransport<tokio::io::ReadHalf<TcpStream>, tokio::io::WriteHalf<TcpStream>>>
    {
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
            transport: Arc<
                crate::StreamTransport<
                    tokio::io::ReadHalf<TcpStream>,
                    tokio::io::WriteHalf<TcpStream>,
                >,
            >,
        ) -> impl std::future::Future<Output = Result<(), crate::RpcError>> + Send;
    }
}
