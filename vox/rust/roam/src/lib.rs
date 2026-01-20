//! roam - High-performance RPC framework
//!
//! This crate provides a unified API for the roam RPC protocol.
//! Users should depend on this crate rather than the individual component crates.

#![deny(unsafe_code)]

// Macro hygiene: Allow `::roam::` paths to work both externally and internally.
// When used in tests within this workspace, `::roam::` would normally
// fail because it would look for a `roam` module within `roam`. This
// self-referential module makes `::roam::session::...` etc. work everywhere.
#[doc(hidden)]
pub mod roam {
    pub use crate::*;
}

// Re-export the service macro
pub use roam_service_macros::service;

// Re-export session types for macro-generated code
pub use roam_session as session;

// Re-export streaming types for user-facing API
pub use roam_session::{
    ChannelError, ChannelId, ChannelIdAllocator, ChannelRegistry, DriverMessage, ReceiverSlot,
    Role, Rx, RxError, SenderSlot, Tx, TxError, channel,
};

// Re-export tunnel types for byte stream bridging
pub use roam_session::{Tunnel, tunnel_pair};

#[cfg(not(target_arch = "wasm32"))]
pub use roam_session::{
    DEFAULT_TUNNEL_CHUNK_SIZE, pump_read_to_tx, pump_rx_to_write, tunnel_stream,
};

// Re-export schema types
pub use roam_schema as schema;

// Re-export hash utilities
pub use roam_hash as hash;

// Re-export facet for derive macros in service types
pub use facet;

// Re-export facet-pretty for macro-generated logging
pub use facet_pretty;
pub use facet_pretty::PrettyPrinter;

// Re-export tracing for macro-generated logging
pub use tracing;

/// Private module for proc-macro re-exports. Not part of the public API.
#[doc(hidden)]
pub mod __private {
    pub use facet_postcard;
}

/// Prelude module for convenient imports.
///
/// ```ignore
/// use roam::prelude::*;
/// ```
pub mod prelude {
    pub use crate::service;
    pub use facet::Facet;
}
