#![deny(unsafe_code)]

//! TCP transport layer for roam RPC.
//!
//! This crate provides the protocol machinery for running roam services over TCP:
//!
//! - COBS framing for message boundaries
//! - Hello exchange and parameter negotiation
//! - Message loop with request dispatch
//! - Stream ID validation
//! - Flow control enforcement
//!
//! # Example
//!
//! ```ignore
//! use roam_tcp::{Server, ServiceDispatcher};
//!
//! struct MyDispatcher { /* ... */ }
//! impl ServiceDispatcher for MyDispatcher { /* ... */ }
//!
//! let server = Server::new();
//! server.run_subject(&MyDispatcher).await?;
//! ```

mod connection;
mod framing;
mod server;

pub use connection::{
    Connection, ConnectionError, Negotiated, ServiceDispatcher, hello_exchange_acceptor,
    hello_exchange_initiator,
};
pub use framing::CobsFramed;
pub use server::{Server, ServerConfig};

// Re-export session types for convenience
pub use roam_session::{Role, StreamIdAllocator};

// Re-export wire types for convenience
pub use roam_wire::{Hello, Message};
