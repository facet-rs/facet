//! rapace-core: Core types and traits for the rapace RPC system.
//!
//! This crate defines:
//! - Frame descriptors ([`MsgDescHot`], [`MsgDescCold`])
//! - Frame types ([`Frame`], [`RecvFrame`])
//! - Message header ([`MsgHeader`])
//! - Transport traits ([`Transport`], [`DynTransport`])
//! - Encoding traits ([`EncodeCtx`], [`DecodeCtx`])
//! - Error codes and flags ([`ErrorCode`], [`FrameFlags`], [`Encoding`])
//! - Control payloads ([`ControlPayload`])
//! - Validation ([`validate_descriptor`], [`DescriptorLimits`])

#![forbid(unsafe_op_in_unsafe_fn)]

mod control;
mod descriptor;
mod encoding;
mod error;
mod flags;
mod frame;
mod handle;
mod header;
mod limits;
mod session;
mod streaming;
mod transport;
mod validation;

pub use control::*;
pub use descriptor::*;
pub use encoding::*;
pub use error::*;
pub use flags::*;
pub use frame::*;
pub use handle::*;
pub use header::*;
pub use limits::*;
pub use session::*;
pub use streaming::*;
pub use transport::*;
pub use validation::*;

// Re-export StreamExt for use by macro-generated streaming clients
pub use futures::StreamExt;

// Re-export try_stream for use by macro-generated streaming clients
pub use async_stream::try_stream;
