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

// Re-export the service macro
pub use rapace_macros::service;

// Re-export core types
pub use rapace_core::{
    // Error types
    DecodeError,
    EncodeError,
    ErrorCode,
    // Frame types (for advanced use)
    Frame,
    FrameFlags,
    MsgDescHot,
    RpcError,
    // Streaming
    Streaming,
    // Transport trait (for advanced use)
    Transport,
    TransportError,
    ValidationError,
};

// Re-export serialization
pub use facet;
pub use facet_postcard;

/// Prelude module for convenient imports.
///
/// ```ignore
/// use rapace::prelude::*;
/// ```
pub mod prelude {
    pub use crate::{service, ErrorCode, RpcError, Streaming, Transport};

    // Re-export facet for derive macros in service types
    pub use facet::Facet;
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
///
/// Most users don't need this - the generated client/server handle it.
/// Enable with `features = ["session"]`.
#[cfg(feature = "session")]
pub mod session {
    pub use rapace_testkit::{ChannelLifecycle, ChannelState, Session, DEFAULT_INITIAL_CREDITS};
}

#[cfg(feature = "mem")]
pub use transport::InProcTransport;

#[cfg(feature = "stream")]
pub use transport::StreamTransport;

#[cfg(feature = "websocket")]
pub use transport::WebSocketTransport;
