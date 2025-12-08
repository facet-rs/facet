//! Message descriptors (hot and cold paths).

use crate::FrameFlags;

/// Size of inline payload in bytes.
pub const INLINE_PAYLOAD_SIZE: usize = 24;

/// Sentinel value indicating payload is inline (not in a slot).
pub const INLINE_PAYLOAD_SLOT: u32 = u32::MAX;

/// Hot-path message descriptor (64 bytes, one cache line).
///
/// This is the primary descriptor used for frame dispatch.
/// Fits in a single cache line for performance.
#[derive(Clone, Copy)]
#[repr(C, align(64))]
pub struct MsgDescHot {
    // Identity (16 bytes)
    /// Unique message ID per session, monotonic.
    pub msg_id: u64,
    /// Logical stream (0 = control channel).
    pub channel_id: u32,
    /// For RPC dispatch, or control verb.
    pub method_id: u32,

    // Payload location (16 bytes)
    /// Slot index (u32::MAX = inline).
    pub payload_slot: u32,
    /// Generation counter for ABA safety.
    pub payload_generation: u32,
    /// Offset within slot.
    pub payload_offset: u32,
    /// Actual payload length.
    pub payload_len: u32,

    // Flow control & flags (8 bytes)
    /// Frame flags (EOS, CANCEL, ERROR, etc.).
    pub flags: FrameFlags,
    /// Credits being granted to peer.
    pub credit_grant: u32,

    // Inline payload for small messages (24 bytes)
    /// When payload_slot == u32::MAX, payload lives here.
    /// No alignment guarantees beyond u8.
    pub inline_payload: [u8; INLINE_PAYLOAD_SIZE],
}

const _: () = assert!(core::mem::size_of::<MsgDescHot>() == 64);

impl MsgDescHot {
    /// Create a new descriptor with default values.
    pub const fn new() -> Self {
        Self {
            msg_id: 0,
            channel_id: 0,
            method_id: 0,
            payload_slot: INLINE_PAYLOAD_SLOT,
            payload_generation: 0,
            payload_offset: 0,
            payload_len: 0,
            flags: FrameFlags::empty(),
            credit_grant: 0,
            inline_payload: [0; INLINE_PAYLOAD_SIZE],
        }
    }

    /// Returns true if payload is inline (not in a slot).
    #[inline]
    pub const fn is_inline(&self) -> bool {
        self.payload_slot == INLINE_PAYLOAD_SLOT
    }

    /// Returns true if this is a control frame (channel 0).
    #[inline]
    pub const fn is_control(&self) -> bool {
        self.channel_id == 0
    }

    /// Get inline payload slice (only valid if is_inline()).
    #[inline]
    pub fn inline_payload(&self) -> &[u8] {
        &self.inline_payload[..self.payload_len as usize]
    }
}

impl Default for MsgDescHot {
    fn default() -> Self {
        Self::new()
    }
}

impl core::fmt::Debug for MsgDescHot {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("MsgDescHot")
            .field("msg_id", &self.msg_id)
            .field("channel_id", &self.channel_id)
            .field("method_id", &self.method_id)
            .field("payload_slot", &self.payload_slot)
            .field("payload_generation", &self.payload_generation)
            .field("payload_offset", &self.payload_offset)
            .field("payload_len", &self.payload_len)
            .field("flags", &self.flags)
            .field("credit_grant", &self.credit_grant)
            .field("is_inline", &self.is_inline())
            .finish()
    }
}

/// Cold-path message descriptor (observability data).
///
/// Stored in a parallel array or separate telemetry ring.
/// Can be disabled for performance.
#[derive(Debug, Clone, Copy, Default)]
#[repr(C, align(64))]
pub struct MsgDescCold {
    /// Correlates with hot descriptor.
    pub msg_id: u64,
    /// Distributed tracing ID.
    pub trace_id: u64,
    /// Span within trace.
    pub span_id: u64,
    /// Parent span ID.
    pub parent_span_id: u64,
    /// When enqueued (nanos since epoch).
    pub timestamp_ns: u64,
    /// 0=off, 1=metadata, 2=full payload.
    pub debug_level: u32,
    pub _reserved: u32,
}

const _: () = assert!(core::mem::size_of::<MsgDescCold>() == 64);
