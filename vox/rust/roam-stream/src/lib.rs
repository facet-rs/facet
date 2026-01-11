#![deny(unsafe_code)]

//! Stream transport layer for roam RPC.
//!
//! This crate provides the protocol machinery for running roam services over any
//! async byte stream (TCP, Unix sockets, etc.):
//!
//! - COBS framing for message boundaries
//! - Hello exchange and parameter negotiation
//! - Message loop with request dispatch
//! - Stream ID validation
//! - Flow control enforcement
//!
//! # Generic Transport
//!
//! The core types (`CobsFramed`, `Connection`) are generic over any transport that
//! implements `AsyncRead + AsyncWrite + Unpin`. This allows the same code to work
//! with TCP sockets, Unix domain sockets, or any other async byte stream.
//!
//! # Example (TCP)
//!
//! ```ignore
//! use roam_stream::{Server, ServiceDispatcher};
//!
//! struct MyDispatcher { /* ... */ }
//! impl ServiceDispatcher for MyDispatcher { /* ... */ }
//!
//! let server = Server::new();
//! server.run_subject(&MyDispatcher).await?;
//! ```
//!
//! # Example (Unix Socket)
//!
//! ```ignore
//! use roam_stream::{CobsFramed, hello_exchange_acceptor};
//! use tokio::net::UnixStream;
//!
//! let stream: UnixStream = listener.accept().await?.0;
//! let io = CobsFramed::new(stream);
//! let mut conn = hello_exchange_acceptor(io, hello).await?;
//! conn.run(&dispatcher).await?;
//! ```

mod connection;
mod driver;
mod framing;
mod server;
mod transport;

pub use connection::{
    Connection, ConnectionError, Negotiated, RoutedDispatcher, ServiceDispatcher,
    hello_exchange_acceptor, hello_exchange_initiator,
};
pub use driver::{Driver, establish_acceptor, establish_initiator};
pub use framing::CobsFramed;
pub use server::{Server, ServerConfig, TcpConnection};
pub use transport::MessageTransport;

// Re-export session types for convenience
pub use roam_session::{CallError, ChannelIdAllocator, ChannelRegistry, ConnectionHandle, Role};

// Re-export wire types for convenience
pub use roam_wire::{Hello, Message};

// Re-export tokio IO traits for convenience when using with custom streams
pub use tokio::io::{AsyncRead, AsyncWrite};
