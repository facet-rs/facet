//! Frame types for sending and receiving.

use std::ops::Deref;

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
    ///
    /// Note: For owned frames with external payload, we use a sentinel slot value (0)
    /// to distinguish from inline. The actual slot is only meaningful for SHM transport.
    pub fn with_payload(mut desc: MsgDescHot, payload: Vec<u8>) -> Self {
        // Mark as non-inline by setting slot to 0 (or any non-MAX value)
        // This is a sentinel for "payload is in the Frame::payload field"
        desc.payload_slot = 0;
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

/// Received frame with owned descriptor and a payload handle.
///
/// The payload handle `P` is transport-specific:
/// - Non-SHM transports use a pooled buffer that returns to pool on drop
/// - SHM transport uses a slot guard that frees the slot on drop
///
/// For inline payloads (small frames), `payload` is `None` and the data
/// lives inside `desc.inline_payload`.
#[derive(Debug)]
pub struct RecvFrame<P> {
    /// The frame descriptor (owned, copied from transport).
    pub desc: MsgDescHot,
    /// Payload handle. `None` means payload is inline (or empty).
    pub payload: Option<P>,
}

impl<P: Deref<Target = [u8]>> RecvFrame<P> {
    /// Get the payload bytes.
    pub fn payload_bytes(&self) -> &[u8] {
        if self.desc.is_inline() {
            self.desc.inline_payload()
        } else {
            self.payload.as_deref().unwrap_or(&[])
        }
    }

    /// Convert to an owned Frame (copies payload if needed).
    pub fn to_owned(&self) -> Frame {
        if self.desc.is_inline() {
            Frame {
                desc: self.desc,
                payload: None,
            }
        } else {
            Frame {
                desc: self.desc,
                payload: Some(self.payload_bytes().to_vec()),
            }
        }
    }
}

impl<P> RecvFrame<P> {
    /// Create a new received frame with inline payload.
    pub fn inline(desc: MsgDescHot) -> Self {
        Self {
            desc,
            payload: None,
        }
    }

    /// Create a new received frame with external payload.
    pub fn with_payload(desc: MsgDescHot, payload: P) -> Self {
        Self {
            desc,
            payload: Some(payload),
        }
    }
}
