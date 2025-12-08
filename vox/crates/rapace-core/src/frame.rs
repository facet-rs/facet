//! Frame types for sending and receiving.

use crate::MsgDescHot;

/// Owned frame for sending or storage.
#[derive(Debug, Clone)]
pub struct Frame {
    /// The frame descriptor.
    pub desc: MsgDescHot,
    /// Optional payload (None if inline or empty).
    pub payload: Option<Vec<u8>>,
}

impl Frame {
    /// Create a new frame with the given descriptor.
    pub fn new(desc: MsgDescHot) -> Self {
        Self {
            desc,
            payload: None,
        }
    }

    /// Create a frame with inline payload.
    pub fn with_inline_payload(mut desc: MsgDescHot, payload: &[u8]) -> Option<Self> {
        if payload.len() > crate::INLINE_PAYLOAD_SIZE {
            return None;
        }
        desc.payload_slot = crate::INLINE_PAYLOAD_SLOT;
        desc.payload_generation = 0;
        desc.payload_offset = 0;
        desc.payload_len = payload.len() as u32;
        desc.inline_payload[..payload.len()].copy_from_slice(payload);
        Some(Self {
            desc,
            payload: None,
        })
    }

    /// Create a frame with external payload.
    pub fn with_payload(mut desc: MsgDescHot, payload: Vec<u8>) -> Self {
        desc.payload_len = payload.len() as u32;
        Self {
            desc,
            payload: Some(payload),
        }
    }

    /// Get the payload bytes.
    pub fn payload(&self) -> &[u8] {
        if self.desc.is_inline() {
            self.desc.inline_payload()
        } else {
            self.payload.as_deref().unwrap_or(&[])
        }
    }
}

/// Borrowed view of a frame for zero-copy receive.
///
/// Lifetime is tied to the receive call that produced it.
/// Caller must process or copy before calling recv_frame again.
#[derive(Debug)]
pub struct FrameView<'a> {
    /// Reference to the frame descriptor.
    pub desc: &'a MsgDescHot,
    /// Reference to the payload bytes.
    pub payload: &'a [u8],
}

impl<'a> FrameView<'a> {
    /// Create a new frame view.
    pub fn new(desc: &'a MsgDescHot, payload: &'a [u8]) -> Self {
        Self { desc, payload }
    }

    /// Convert to an owned Frame (copies payload if needed).
    pub fn to_owned(&self) -> Frame {
        if self.desc.is_inline() {
            Frame {
                desc: *self.desc,
                payload: None,
            }
        } else {
            Frame {
                desc: *self.desc,
                payload: Some(self.payload.to_vec()),
            }
        }
    }
}
