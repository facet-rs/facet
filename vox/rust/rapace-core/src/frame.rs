//! Unified frame representation.
//!
//! See spec: [Frame Format](https://rapace.dev/spec/frame-format/)

use crate::{MsgDescHot, buffer_pool::PooledBuf};
use bytes::Bytes;

/// Payload storage for a frame.
///
/// Spec: `[impl frame.structure]` - a frame consists of MsgDescHot + payload bytes.
///
/// The payload location varies by transport:
/// - Spec: `[impl frame.payload.inline]` - inline when ≤16 bytes
/// - Spec: `[impl frame.payload.out-of-line]` - slot-backed (SHM) or heap (stream)
/// - Spec: `[impl frame.shm.borrow-required]` - receivers MUST be able to borrow without copying
#[derive(Debug)]
pub enum Payload {
    /// Payload bytes live inside `MsgDescHot::inline_payload`.
    ///
    /// Spec: `[impl frame.payload.inline]` - used when payload_len ≤ 16.
    Inline,
    /// Payload bytes are owned as a heap allocation.
    Owned(Vec<u8>),
    /// Payload bytes are stored in a ref-counted buffer (cheap clone).
    Bytes(Bytes),
    /// Payload bytes backed by a buffer pool (returns to pool on drop).
    Pooled(PooledBuf),
    /// Payload bytes backed by a shared-memory slot guard (frees slot on drop).
    ///
    /// Spec: `[impl frame.shm.slot-guard]` - slot remains valid for guard lifetime.
    #[cfg(feature = "shm")]
    Shm(crate::transport::shm::SlotGuard),
}

impl Payload {
    /// Borrow the payload as a byte slice.
    pub fn as_slice<'a>(&'a self, desc: &'a MsgDescHot) -> &'a [u8] {
        match self {
            Payload::Inline => desc.inline_payload(),
            Payload::Owned(buf) => buf.as_slice(),
            Payload::Bytes(buf) => buf.as_ref(),
            Payload::Pooled(buf) => buf.as_ref(),
            #[cfg(feature = "shm")]
            Payload::Shm(guard) => guard.as_ref(),
        }
    }

    /// Borrow the payload as a byte slice without needing a descriptor.
    ///
    /// Returns `None` for [`Payload::Inline`], since inline bytes live inside
    /// `MsgDescHot`.
    pub fn external_slice(&self) -> Option<&[u8]> {
        match self {
            Payload::Inline => None,
            Payload::Owned(buf) => Some(buf.as_slice()),
            Payload::Bytes(buf) => Some(buf.as_ref()),
            Payload::Pooled(buf) => Some(buf.as_ref()),
            #[cfg(feature = "shm")]
            Payload::Shm(guard) => Some(guard.as_ref()),
        }
    }

    /// Returns the payload length in bytes.
    pub fn len(&self, desc: &MsgDescHot) -> usize {
        if let Some(ext) = self.external_slice() {
            ext.len()
        } else {
            desc.payload_len as usize
        }
    }

    /// Returns true if this payload is stored inline.
    pub fn is_inline(&self) -> bool {
        matches!(self, Payload::Inline)
    }
}

/// Owned frame for sending, receiving, or routing.
///
/// Spec: `[impl frame.structure]` - a Rapace frame consists of:
/// 1. `MsgDescHot` (64 bytes) - routing, flow control, payload location
/// 2. `Payload` - the Postcard-encoded payload bytes
#[derive(Debug)]
pub struct Frame {
    /// The frame descriptor (64 bytes, cache-line aligned).
    ///
    /// Spec: `[impl frame.desc.size]` - exactly 64 bytes.
    pub desc: MsgDescHot,
    /// Payload storage for this frame.
    pub payload: Payload,
}

impl Frame {
    /// Create a new frame with no payload (inline empty).
    pub fn new(desc: MsgDescHot) -> Self {
        Self {
            desc,
            payload: Payload::Inline,
        }
    }

    /// Create a frame with inline payload.
    ///
    /// Spec: `[impl frame.payload.inline]` - inline when payload ≤ 16 bytes.
    /// Returns `None` if payload is too large for inline storage.
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
            payload: Payload::Inline,
        })
    }

    /// Create a frame with an owned payload allocation.
    pub fn with_payload(mut desc: MsgDescHot, payload: Vec<u8>) -> Self {
        desc.payload_slot = 0;
        desc.payload_generation = 0;
        desc.payload_offset = 0;
        desc.payload_len = payload.len() as u32;
        Self {
            desc,
            payload: Payload::Owned(payload),
        }
    }

    /// Create a frame with a pooled buffer payload.
    pub fn with_pooled_payload(mut desc: MsgDescHot, payload: PooledBuf) -> Self {
        desc.payload_slot = 0;
        desc.payload_generation = 0;
        desc.payload_offset = 0;
        desc.payload_len = payload.len() as u32;
        Self {
            desc,
            payload: Payload::Pooled(payload),
        }
    }

    /// Borrow the payload as bytes.
    pub fn payload_bytes(&self) -> &[u8] {
        self.payload.as_slice(&self.desc)
    }

    /// Create a new frame with an owned copy of this frame's payload.
    ///
    /// This is useful when you need to extend the lifetime of frame data
    /// (e.g., moving into an async task) but the original payload is backed
    /// by shared memory or another non-cloneable source.
    pub fn to_owned(&self) -> Self {
        Self {
            desc: self.desc,
            payload: Payload::Owned(self.payload_bytes().to_vec()),
        }
    }
}
