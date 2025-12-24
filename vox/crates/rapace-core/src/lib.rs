#![doc = include_str!("../README.md")]
#![forbid(unsafe_op_in_unsafe_fn)]

mod buffer_pool;
mod control;
mod descriptor;
mod encoding;
mod error;
mod flags;
mod frame;
mod header;
mod limits;
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
pub use session::*;
pub use streaming::*;
pub use transport::*;
#[cfg(not(target_arch = "wasm32"))]
pub use tunnel_stream::*;
pub use validation::*;

// Re-export futures utilities for use by macro-generated streaming clients
pub use futures::StreamExt;

// Re-export stream utilities with a renamed module to avoid conflict with our stream transport module
pub mod futures_stream {
    pub use futures::stream::unfold;
}
