#![deny(unsafe_code)]

//! Session/state machine and RPC-level utilities.
//!
//! Canonical definitions live in `docs/content/spec/_index.md`,
//! `docs/content/rust-spec/_index.md`, and `docs/content/shm-spec/_index.md`.

#[macro_use]
mod macros;

pub mod diagnostic;
pub mod driver;
pub mod runtime;
pub mod transport;

pub use driver::{
    ConnectError, ConnectionError, Driver, FramedClient, HandshakeConfig, IncomingConnection,
    IncomingConnections, MessageConnector, Negotiated, NoDispatcher, RetryPolicy, accept_framed,
    connect_framed, connect_framed_with_policy, initiate_framed,
};
pub use transport::MessageTransport;

pub use roam_frame::{Frame, MsgDesc, OwnedMessage, Payload};

mod connection_handle;
pub use connection_handle::*;

mod caller;
pub use caller::*;

mod errors;
pub use errors::*;

mod types;
pub use types::*;

mod channel;
pub use channel::*;

mod tunnel;
pub use tunnel::*;

mod flow_control;
pub use flow_control::*;

mod dispatch;
pub use dispatch::*;
// Re-export internal items needed by other modules
pub(crate) use dispatch::{DispatchContext, get_dispatch_context};

mod forwarding;
pub use forwarding::*;

pub(crate) const CHANNEL_SIZE: usize = 1024;
pub(crate) const RX_STREAM_BUFFER_SIZE: usize = 1024;

/// Re-export `Infallible` for use as the error type in infallible methods.
pub use std::convert::Infallible;

#[cfg(test)]
mod tests;
