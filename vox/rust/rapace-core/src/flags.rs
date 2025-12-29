//! Frame flags and encoding types.
//!
//! See spec: [Core Protocol: FrameFlags](https://rapace.dev/spec/core/#frameflags)

use bitflags::bitflags;

bitflags! {
    /// Flags carried in each frame descriptor.
    ///
    /// Spec: `[impl core.flags.reserved]` - reserved flags MUST NOT be set;
    /// receivers MUST ignore unknown flags.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct FrameFlags: u32 {
        /// Frame carries payload data.
        ///
        /// Spec: `[impl core.call.request.flags]` - requests have `DATA | EOS`.
        /// Spec: `[impl core.stream.frame.flags]` - stream items have `DATA`, final has `DATA | EOS`.
        const DATA          = 0b0000_0001;

        /// Control message (channel 0 only).
        ///
        /// Spec: `[impl core.control.flag-set]` - MUST be set on channel 0.
        /// Spec: `[impl core.control.flag-clear]` - MUST NOT be set on other channels.
        const CONTROL       = 0b0000_0010;

        /// End of stream (half-close).
        ///
        /// Spec: `[impl core.eos.after-send]` - sender MUST NOT send more DATA after EOS.
        /// Spec: `[impl core.flow.eos-no-credits]` - EOS-only frames don't consume credits.
        const EOS           = 0b0000_0100;

        /// Cancel this channel (reserved, use CancelChannel control message).
        const CANCEL        = 0b0000_1000;

        /// Error response.
        ///
        /// Spec: `[impl core.call.error.flags]` - set on error responses.
        /// Spec: `[impl core.call.error.flag-match]` - MUST match envelope status.code != 0.
        const ERROR         = 0b0001_0000;

        /// Priority scheduling hint.
        ///
        /// Spec: `[impl core.flags.high-priority]` - maps to priority level 192.
        const HIGH_PRIORITY = 0b0010_0000;

        /// The `credit_grant` field contains a valid credit grant.
        ///
        /// Spec: `[impl core.flow.credit-semantics]` - fast-path credit grant.
        const CREDITS       = 0b0100_0000;

        /// Headers/trailers only, no body.
        const METADATA_ONLY = 0b1000_0000;

        /// Don't send a reply frame for this request (fire-and-forget).
        ///
        /// This is intended for fire-and-forget notifications where the caller
        /// does not want to register a pending waiter or receive an error response.
        const NO_REPLY      = 0b0001_0000_0000;

        /// This is a response frame (not a request).
        ///
        /// Spec: `[impl core.call.response.flags]` - responses have `DATA | EOS | RESPONSE`.
        const RESPONSE      = 0b0010_0000_0000;
    }
}

/// Body encoding format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u16)]
pub enum Encoding {
    /// Default: postcard via facet (not serde).
    Postcard = 1,
    /// JSON for debugging and external tooling.
    Json = 2,
    /// Application-defined, no schema.
    Raw = 3,
}

impl Encoding {
    /// Try to convert from a raw u16 value.
    pub fn from_u16(value: u16) -> Option<Self> {
        match value {
            1 => Some(Self::Postcard),
            2 => Some(Self::Json),
            3 => Some(Self::Raw),
            _ => None,
        }
    }
}
