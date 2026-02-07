#![deny(unsafe_code)]

//! Stream transport layer for roam RPC.
//!
//! This crate provides length-prefixed framing and byte-stream specific machinery for running
//! roam services over TCP, Unix sockets, or any async byte stream.
//!
//! For message-based transports (like WebSocket) that already provide framing,
//! use `roam_session` directly - it has the Driver and accept_framed/connect_framed.
//!
//! # Example (Accepted connection)
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
//! let client = MyServiceClient::new(handle);
//! let response = client.echo("hello").await?;
//! ```
//!
//! # Example (Initiated connection with reconnection)
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
//! let service = MyServiceClient::new(client);
//! let response = service.echo("hello").await?;
//! ```

mod driver;
mod framing;

// Byte-stream specific API (stays here)
pub use driver::{Client, Connector, accept, connect, connect_with_policy};

// length-prefixed framing
pub use framing::LengthPrefixedFramed;
// Re-export types that moved to roam-session (backwards compat)
pub use roam_session::{
    ConnectError, ConnectionError, Driver, FramedClient, HandshakeConfig, IncomingConnection,
    IncomingConnections, MessageConnector, MessageTransport, Negotiated, NoDispatcher, RetryPolicy,
    accept_framed, connect_framed, connect_framed_with_policy, initiate_framed,
};

// Re-export session types for convenience
pub use roam_session::{
    CallError, Caller, ChannelIdAllocator, ChannelRegistry, ConnectionHandle, Role,
    ServiceDispatcher, TransportError,
};

// Re-export wire types for convenience
pub use roam_wire::{Hello, Message};

// Re-export tokio IO traits for convenience
pub use tokio::io::{AsyncRead, AsyncWrite};
