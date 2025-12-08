//! Encoding and decoding context traits.
//!
//! These traits define the transport-specific encoding interface.
//! Actual facet-based serialization happens at a higher level (in codegen or rapace-codec).

use crate::{DecodeError, EncodeError, Frame};

/// Context for encoding values into frames.
///
/// Each transport provides its own implementation that knows how to
/// best represent data for that transport.
///
/// Note: Type-aware encoding (via facet) is handled by the RPC layer,
/// not directly by EncodeCtx. This trait handles raw byte encoding.
pub trait EncodeCtx: Send {
    /// Encode raw bytes into the frame.
    ///
    /// For SHM: checks if bytes are already in SHM and references them zero-copy.
    /// For stream: copies into the output buffer.
    fn encode_bytes(&mut self, bytes: &[u8]) -> Result<(), EncodeError>;

    /// Finish encoding and return the frame.
    fn finish(self: Box<Self>) -> Result<Frame, EncodeError>;
}

/// Context for decoding frames into values.
///
/// Note: Type-aware decoding (via facet) is handled by the RPC layer,
/// not directly by DecodeCtx. This trait handles raw byte access.
pub trait DecodeCtx<'a>: Send {
    /// Borrow raw bytes from the frame.
    ///
    /// Lifetime is tied to the frame — caller must copy if needed longer.
    fn decode_bytes(&mut self) -> Result<&'a [u8], DecodeError>;

    /// Get remaining bytes without consuming them.
    fn remaining(&self) -> &'a [u8];
}
