//! Message descriptors (hot and cold paths).
//!
//! See spec: [Frame Format](https://rapace.dev/spec/frame-format/)

use crate::FrameFlags;

/// Size of inline payload in bytes.
///
/// Spec: `[impl frame.payload.inline]` - inline payloads are ≤16 bytes.
pub const INLINE_PAYLOAD_SIZE: usize = 16;

/// Sentinel value indicating payload is inline (not in a slot).
///
/// Spec: `[impl frame.sentinel.values]` - `payload_slot = 0xFFFFFFFF` means inline.
pub const INLINE_PAYLOAD_SLOT: u32 = u32::MAX;

/// Sentinel value indicating no deadline.
///
/// Spec: `[impl frame.sentinel.values]` - `deadline_ns = 0xFFFFFFFFFFFFFFFF` means no deadline.
pub const NO_DEADLINE: u64 = u64::MAX;

/// Hot-path message descriptor (64 bytes, one cache line).
///
/// This is the primary descriptor used for frame dispatch.
/// Fits in a single cache line for performance.
///
/// Spec: `[impl frame.desc.size]` - descriptor MUST be exactly 64 bytes.
/// Spec: `[impl frame.desc.encoding]` - raw bytes, little-endian, no padding.
#[derive(Clone, Copy)]
#[repr(C, align(64))]
pub struct MsgDescHot {
    // Identity (16 bytes)
    /// Unique message ID per connection, monotonically increasing.
    ///
    /// Spec: `[impl frame.msg-id.scope]` - scoped per connection, starts at 1, monotonic.
    /// Spec: `[impl frame.msg-id.call-echo]` - CALL responses MUST echo request's msg_id.
    pub msg_id: u64,
    /// Logical channel (0 = control channel).
    ///
    /// Spec: `[impl core.channel.id.zero-reserved]` - channel 0 reserved for control.
    /// Spec: `[impl core.channel.id.parity.initiator]` - initiator uses odd IDs.
    /// Spec: `[impl core.channel.id.parity.acceptor]` - acceptor uses even IDs.
    pub channel_id: u32,
    /// Method identifier (FNV-1a hash) for CALL channels, or control verb for channel 0.
    ///
    /// Spec: `[impl core.method-id.algorithm]` - FNV-1a folded to 32 bits.
    /// Spec: `[impl core.method-id.zero-reserved]` - 0 reserved for control/STREAM/TUNNEL.
    /// Spec: `[impl core.stream.frame.method-id-zero]` - STREAM frames use method_id=0.
    pub method_id: u32,

    // Payload location (16 bytes)
    /// Slot index for SHM, or `INLINE_PAYLOAD_SLOT` (0xFFFFFFFF) for inline.
    ///
    /// Spec: `[impl frame.sentinel.values]` - 0xFFFFFFFF = inline, 0xFFFFFFFE = reserved.
    /// Spec: `[impl frame.payload.inline]` - use inline when payload_len ≤ 16.
    pub payload_slot: u32,
    /// Generation counter for ABA safety (SHM only).
    ///
    /// Spec: `[impl frame.shm.slot-guard]` - generation prevents ABA problems.
    pub payload_generation: u32,
    /// Byte offset within slot (typically 0).
    pub payload_offset: u32,
    /// Payload length in bytes.
    ///
    /// Spec: `[impl frame.payload.empty]` - empty payloads (len=0) are valid.
    pub payload_len: u32,

    // Flow control & timing (16 bytes)
    /// Frame flags (EOS, CANCEL, ERROR, etc.).
    ///
    /// Spec: `[impl core.flags.reserved]` - reserved flags MUST NOT be set.
    pub flags: FrameFlags,
    /// Credits being granted to peer (valid if `CREDITS` flag set).
    ///
    /// Spec: `[impl core.flow.credit-semantics]` - credit-based flow control per channel.
    /// Spec: `[impl core.flow.credit-additive]` - credits are additive.
    pub credit_grant: u32,
    /// Absolute deadline in nanoseconds since epoch. `NO_DEADLINE` = no deadline.
    ///
    /// Spec: `[impl frame.sentinel.values]` - 0xFFFFFFFFFFFFFFFF = no deadline.
    pub deadline_ns: u64,

    // Inline payload for small messages (16 bytes)
    /// When `payload_slot == INLINE_PAYLOAD_SLOT`, payload lives here.
    /// No alignment guarantees beyond u8.
    ///
    /// Spec: `[impl frame.payload.inline]` - used when payload_len ≤ 16.
    pub inline_payload: [u8; INLINE_PAYLOAD_SIZE],
}

// Spec: `[impl frame.desc.sizeof]` - implementations MUST ensure sizeof(MsgDescHot) == 64.
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
            deadline_ns: NO_DEADLINE,
            inline_payload: [0; INLINE_PAYLOAD_SIZE],
        }
    }

    /// Returns true if this frame has a deadline set.
    #[inline]
    pub const fn has_deadline(&self) -> bool {
        self.deadline_ns != NO_DEADLINE
    }

    /// Check if the deadline has passed.
    ///
    /// Returns `true` if the frame has a deadline and it has expired.
    #[inline]
    pub fn is_expired(&self, now_ns: u64) -> bool {
        self.deadline_ns != NO_DEADLINE && now_ns > self.deadline_ns
    }

    /// Returns true if payload is inline (not in a slot).
    #[inline]
    pub const fn is_inline(&self) -> bool {
        self.payload_slot == INLINE_PAYLOAD_SLOT
    }

    /// Returns true if this is a control frame (channel 0).
    ///
    /// Spec: `[impl core.control.reserved]` - channel 0 reserved for control messages.
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
            .field("deadline_ns", &self.deadline_ns)
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
