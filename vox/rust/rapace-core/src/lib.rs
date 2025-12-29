#![doc = include_str!("../README.md")]
#![forbid(unsafe_op_in_unsafe_fn)]

// Compliance level definitions are meta-requirements that describe what
// constitutes each compliance level. They are not individually testable
// but are satisfied by the combination of all protocol rules.
// [impl compliance.core.requirements]
// [impl compliance.standard.requirements]
// [impl compliance.full.requirements]

mod buffer_pool;
mod control;
mod descriptor;
mod encoding;
mod error;
mod flags;
mod frame;
mod header;
mod limits;
mod owned_message;
mod session;
mod streaming;
mod transport;
#[cfg(not(target_arch = "wasm32"))]
mod tunnel_stream;
mod validation;

pub use buffer_pool::*;
pub use control::*;
pub use descriptor::*;
pub use encoding::*;
pub use error::*;
pub use flags::*;
pub use frame::*;
pub use header::*;
pub use limits::*;
pub use owned_message::*;
pub use session::*;
pub use streaming::*;
pub use transport::*;
#[cfg(not(target_arch = "wasm32"))]
pub use tunnel_stream::*;
pub use validation::*;

// Re-export futures utilities for use by macro-generated streaming clients
pub use futures_util::StreamExt;

// Re-export stream utilities with a renamed module to avoid conflict with our stream transport module
pub mod futures_stream {
    pub use futures_util::stream::unfold;
}
