#![deny(unsafe_code)]

//! Stream transport layer for roam RPC.
//!
//! This crate provides the protocol machinery for running roam services over any
//! async byte stream (TCP, Unix sockets, etc.):
//!
//! - COBS framing for message boundaries
//! - Hello exchange and parameter negotiation
//! - Bidirectional RPC with automatic reconnection
//! - Stream ID validation and flow control
//!
//! # Two Connection Modes
//!
//! ## Accepted Connections
//!
//! For connections you accepted (e.g., from a listener), use [`accept()`].
//! No reconnection is possible since you don't control how to re-establish.
//!
//! ```ignore
//! use roam_stream::{accept, HandshakeConfig};
//! use tokio::net::TcpListener;
//!
//! let listener = TcpListener::bind("127.0.0.1:9000").await?;
//! let (stream, _) = listener.accept().await?;
//!
//! let (handle, driver) = accept(stream, HandshakeConfig::default(), dispatcher).await?;
//! tokio::spawn(driver.run());
//!
//! // Use handle with a generated client
//! let client = MyServiceClient::new(handle);
//! let response = client.echo("hello").await?;
//! ```
//!
//! ## Initiated Connections
//!
//! For connections you initiate, use [`connect()`]. It returns a [`Client`] that
//! automatically reconnects on failure.
//!
//! ```ignore
//! use roam_stream::{connect, Connector, HandshakeConfig, NoDispatcher};
//! use tokio::net::TcpStream;
//!
//! struct MyConnector { addr: String }
//!
//! impl Connector for MyConnector {
//!     type Transport = TcpStream;
//!     async fn connect(&self) -> io::Result<TcpStream> {
//!         TcpStream::connect(&self.addr).await
//!     }
//! }
//!
//! let connector = MyConnector { addr: "127.0.0.1:9000".into() };
//! let client = connect(connector, HandshakeConfig::default(), NoDispatcher);
//!
//! // Client automatically reconnects on failure
//! let service = MyServiceClient::new(client);
//! let response = service.echo("hello").await?;
//! ```

mod driver;
mod framing;

// MessageTransport moved to roam-session
// mod transport;

// Main API
pub use driver::{
    // Types
    Client,
    ConnectError,
    ConnectionError,
    Connector,
    Driver,
    FramedClient,
    HandshakeConfig,
    MessageConnector,
    Negotiated,
    NoDispatcher,
    RetryPolicy,
    // Entry points for byte-stream transports (TCP, Unix)
    accept,
    // Entry points for message transports (WebSocket)
    accept_framed,
    connect,
    connect_framed,
    connect_framed_with_policy,
    connect_with_policy,
};

pub use framing::CobsFramed;

// MessageTransport now lives in roam-session
pub use roam_session::MessageTransport;

// Re-export session types for convenience
pub use roam_session::{
    CallError, Caller, ChannelIdAllocator, ChannelRegistry, ConnectionHandle, Role,
    ServiceDispatcher, TransportError,
};

// Re-export wire types for convenience
pub use roam_wire::{Hello, Message};

// Re-export tokio IO traits for convenience
pub use tokio::io::{AsyncRead, AsyncWrite};

// Legacy compatibility - deprecated
#[deprecated(note = "Use accept_framed() instead")]
#[allow(deprecated)]
pub use driver::establish_acceptor;

#[deprecated(note = "Use connect() instead")]
#[allow(deprecated)]
pub use driver::establish_initiator;
