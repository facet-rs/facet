#![doc = include_str!("../README.md")]
#![forbid(unsafe_op_in_unsafe_fn)]

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

// Re-export StreamExt for use by macro-generated streaming clients
pub use futures::StreamExt;

// Re-export try_stream for use by macro-generated streaming clients
pub use async_stream::try_stream;
