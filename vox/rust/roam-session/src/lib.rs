#![deny(unsafe_code)]

//! Session/state machine and RPC-level utilities.
//!
//! Canonical definitions live in `docs/content/spec/_index.md`,
//! `docs/content/rust-spec/_index.md`, and `docs/content/shm-spec/_index.md`.

#[macro_use]
mod macros;

::peeps::facade!();

pub mod diagnostic;
pub mod diagnostic_snapshot;
pub mod driver;
pub mod request_response_spy;
pub mod runtime;
pub mod transport;

pub use driver::{
    ConnectError, ConnectionError, Driver, FramedClient, HandshakeConfig, IncomingConnection,
    IncomingConnections, MessageConnector, Negotiated, NoDispatcher, RetryPolicy, accept_framed,
    connect_framed, connect_framed_with_policy, initiate_framed,
};
pub use transport::{DiagnosticTransport, MessageTransport};

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
// Re-export internal items needed by channel binding
pub(crate) use dispatch::get_dispatch_context;

mod forwarding;
pub use forwarding::*;

mod extensions;
pub use extensions::*;

mod middleware;
pub use middleware::*;

mod rpc_plan;
pub use rpc_plan::*;

pub(crate) const CHANNEL_SIZE: usize = 1024;
pub(crate) const RX_STREAM_BUFFER_SIZE: usize = 1024;
pub const PEEPS_TASK_ID_METADATA_KEY: &str = "peeps.task_id";
pub const PEEPS_TASK_NAME_METADATA_KEY: &str = "peeps.task_name";
pub const PEEPS_CHAIN_ID_METADATA_KEY: &str = "peeps.chain_id";
pub const PEEPS_SPAN_ID_METADATA_KEY: &str = "peeps.span_id";
pub const PEEPS_PARENT_SPAN_ID_METADATA_KEY: &str = "peeps.parent_span_id";
pub const PEEPS_METHOD_NAME_METADATA_KEY: &str = "peeps.method_name";
pub const PEEPS_REQUEST_ENTITY_ID_METADATA_KEY: &str = "peeps.request_entity_id";
pub const PEEPS_CONNECTION_CORRELATION_ID_METADATA_KEY: &str = "peeps.connection_correlation_id";

/// Re-export `Infallible` for use as the error type in infallible methods.
pub use std::convert::Infallible;

pub use ::peeps::{SourceId, SourceLeft, SourceRight};

/// Resolve a caller location against an explicit crate identity.
#[track_caller]
pub fn source_id_from_left(left: SourceLeft) -> SourceId {
    left.join(SourceRight::caller()).into()
}

/// Source id for the current `roam-session` crate.
#[track_caller]
pub fn source_id_for_current_crate() -> SourceId {
    source_id_from_left(SourceLeft::new(
        env!("CARGO_MANIFEST_DIR"),
        env!("CARGO_PKG_NAME"),
    ))
}

#[cfg(test)]
mod tests;
