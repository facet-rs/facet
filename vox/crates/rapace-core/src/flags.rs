//! Frame flags and encoding types.

use bitflags::bitflags;

bitflags! {
    /// Flags carried in each frame descriptor.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct FrameFlags: u32 {
        /// Regular data frame.
        const DATA          = 0b0000_0001;
        /// Control frame (channel 0).
        const CONTROL       = 0b0000_0010;
        /// End of stream (half-close).
        const EOS           = 0b0000_0100;
        /// Cancel this channel.
        const CANCEL        = 0b0000_1000;
        /// Error response.
        const ERROR         = 0b0001_0000;
        /// Priority scheduling hint.
        const HIGH_PRIORITY = 0b0010_0000;
        /// Contains credit grant.
        const CREDITS       = 0b0100_0000;
        /// Headers/trailers only, no body.
        const METADATA_ONLY = 0b1000_0000;
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
