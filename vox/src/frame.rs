// src/frame.rs

use crate::layout::{MsgDescHot, INLINE_PAYLOAD_SIZE};
use crate::types::{ChannelId, MethodId, MsgId, SlotIndex, Generation};
use crate::alloc::{CommittedSlot, DataSegment, InboundPayload};

// Re-export ControlMethod from dispatch module
pub use crate::dispatch::ControlMethod;

/// Frame flags matching the spec
bitflags::bitflags! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct FrameFlags: u32 {
        const DATA          = 0b0000_0001;
        const CONTROL       = 0b0000_0010;
        const EOS           = 0b0000_0100;
        const CANCEL        = 0b0000_1000;
        const ERROR         = 0b0001_0000;
        const HIGH_PRIORITY = 0b0010_0000;
        const CREDITS       = 0b0100_0000;
        const METADATA_ONLY = 0b1000_0000;
    }
}

/// Builder for outbound frames. Enforces invariants at construction time.
pub struct FrameBuilder {
    desc: MsgDescHot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PayloadTooLarge;

impl FrameBuilder {
    /// Create a new data frame builder.
    pub fn data(channel: ChannelId, method: MethodId, msg_id: MsgId) -> Self {
        FrameBuilder {
            desc: MsgDescHot {
                msg_id: msg_id.get(),
                channel_id: channel.get(),
                method_id: method.get(),
                payload_slot: u32::MAX,
                payload_generation: 0,
                payload_offset: 0,
                payload_len: 0,
                flags: FrameFlags::DATA.bits(),
                credit_grant: 0,
                inline_payload: [0; INLINE_PAYLOAD_SIZE],
            },
        }
    }

    /// Create a control frame builder (channel 0).
    pub fn control(method: ControlMethod, msg_id: MsgId) -> Self {
        FrameBuilder {
            desc: MsgDescHot {
                msg_id: msg_id.get(),
                channel_id: 0, // Control channel
                method_id: method as u32,
                payload_slot: u32::MAX,
                payload_generation: 0,
                payload_offset: 0,
                payload_len: 0,
                flags: FrameFlags::CONTROL.bits(),
                credit_grant: 0,
                inline_payload: [0; INLINE_PAYLOAD_SIZE],
            },
        }
    }

    /// Set inline payload. Enforces: slot=MAX, generation=0, offset=0.
    pub fn inline_payload(mut self, payload: &[u8]) -> Result<Self, PayloadTooLarge> {
        if payload.len() > INLINE_PAYLOAD_SIZE {
            return Err(PayloadTooLarge);
        }

        // Invariants enforced by construction:
        self.desc.payload_slot = u32::MAX;
        self.desc.payload_generation = 0; // MUST be 0 for inline
        self.desc.payload_offset = 0;     // MUST be 0 for inline
        self.desc.payload_len = payload.len() as u32;
        self.desc.inline_payload[..payload.len()].copy_from_slice(payload);

        Ok(self)
    }

    /// Set slot payload from a committed slot.
    pub fn slot_payload(mut self, slot: CommittedSlot) -> Self {
        let (idx, gen, offset, len) = slot.to_descriptor_fields();
        self.desc.payload_slot = idx;
        self.desc.payload_generation = gen;
        self.desc.payload_offset = offset;
        self.desc.payload_len = len;
        self
    }

    /// Add frame flags.
    pub fn with_flags(mut self, flags: FrameFlags) -> Self {
        self.desc.flags |= flags.bits();
        self
    }

    /// Set end-of-stream flag.
    pub fn eos(self) -> Self {
        self.with_flags(FrameFlags::EOS)
    }

    /// Set cancel flag.
    pub fn cancel(self) -> Self {
        self.with_flags(FrameFlags::CANCEL)
    }

    /// Set error flag.
    pub fn error(self) -> Self {
        self.with_flags(FrameFlags::ERROR)
    }

    /// Set credit grant.
    pub fn credits(mut self, credits: u32) -> Self {
        self.desc.credit_grant = credits;
        self.desc.flags |= FrameFlags::CREDITS.bits();
        self
    }

    /// Build the final descriptor.
    pub fn build(self) -> MsgDescHot {
        self.desc
    }
}

// === Descriptor Validation ===

/// Raw descriptor from the wire. Untrusted.
pub struct RawDescriptor(pub(crate) MsgDescHot);

/// Validated descriptor. Safe to access payload.
pub struct ValidDescriptor {
    raw: MsgDescHot,
}

/// Descriptor validation limits.
#[derive(Debug, Clone)]
pub struct DescriptorLimits {
    pub max_payload_len: u32,
    pub max_channels: u32,
}

impl Default for DescriptorLimits {
    fn default() -> Self {
        DescriptorLimits {
            max_payload_len: 1024 * 1024, // 1MB
            max_channels: 1024,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationError {
    SlotOutOfBounds,
    PayloadOutOfBounds,
    StaleGeneration,
    InlinePayloadTooLarge,
    PayloadTooLarge,
}

impl RawDescriptor {
    /// Create from a MsgDescHot (e.g., after dequeuing from ring)
    pub fn new(desc: MsgDescHot) -> Self {
        RawDescriptor(desc)
    }

    /// Validate descriptor against segment and limits.
    /// On success, returns a ValidDescriptor that's safe to use.
    pub fn validate(
        self,
        seg: &DataSegment,
        limits: &DescriptorLimits,
    ) -> Result<ValidDescriptor, ValidationError> {
        let desc = &self.0;

        // Check payload length limit
        if desc.payload_len > limits.max_payload_len {
            return Err(ValidationError::PayloadTooLarge);
        }

        if desc.payload_slot == u32::MAX {
            // Inline payload
            if desc.payload_len > INLINE_PAYLOAD_SIZE as u32 {
                return Err(ValidationError::InlinePayloadTooLarge);
            }
            // Note: we don't strictly enforce generation=0, offset=0 on receive
            // (be liberal in what you accept), but we DO enforce on send
        } else {
            // Slot payload - validate bounds
            if desc.payload_slot >= seg.slot_count() {
                return Err(ValidationError::SlotOutOfBounds);
            }

            let end = desc.payload_offset.saturating_add(desc.payload_len);
            if end > seg.slot_size() {
                return Err(ValidationError::PayloadOutOfBounds);
            }

            // Generation check
            if seg.generation(SlotIndex::new(desc.payload_slot)) != Generation::new(desc.payload_generation) {
                return Err(ValidationError::StaleGeneration);
            }
        }

        Ok(ValidDescriptor { raw: self.0 })
    }

    /// Validate without a data segment (for inline-only or control frames)
    pub fn validate_inline_only(
        self,
        limits: &DescriptorLimits,
    ) -> Result<ValidDescriptor, ValidationError> {
        let desc = &self.0;

        if desc.payload_len > limits.max_payload_len {
            return Err(ValidationError::PayloadTooLarge);
        }

        if desc.payload_slot != u32::MAX {
            // Has slot reference but we can't validate it
            return Err(ValidationError::SlotOutOfBounds);
        }

        if desc.payload_len > INLINE_PAYLOAD_SIZE as u32 {
            return Err(ValidationError::InlinePayloadTooLarge);
        }

        Ok(ValidDescriptor { raw: self.0 })
    }
}

impl ValidDescriptor {
    /// Get the channel ID (None for control channel)
    pub fn channel_id(&self) -> Option<ChannelId> {
        ChannelId::new(self.raw.channel_id)
    }

    /// Check if this is a control frame
    pub fn is_control(&self) -> bool {
        self.raw.channel_id == 0
    }

    /// Get the method ID
    pub fn method_id(&self) -> MethodId {
        MethodId::new(self.raw.method_id)
    }

    /// Get the message ID
    pub fn msg_id(&self) -> MsgId {
        MsgId::new(self.raw.msg_id)
    }

    /// Get frame flags
    pub fn flags(&self) -> FrameFlags {
        FrameFlags::from_bits_truncate(self.raw.flags)
    }

    /// Check if EOS flag is set
    pub fn is_eos(&self) -> bool {
        self.flags().contains(FrameFlags::EOS)
    }

    /// Check if CANCEL flag is set
    pub fn is_cancel(&self) -> bool {
        self.flags().contains(FrameFlags::CANCEL)
    }

    /// Check if ERROR flag is set
    pub fn is_error(&self) -> bool {
        self.flags().contains(FrameFlags::ERROR)
    }

    /// Get credit grant (if CREDITS flag set)
    pub fn credit_grant(&self) -> Option<u32> {
        if self.flags().contains(FrameFlags::CREDITS) {
            Some(self.raw.credit_grant)
        } else {
            None
        }
    }

    /// Check if payload is inline
    pub fn is_inline(&self) -> bool {
        self.raw.payload_slot == u32::MAX
    }

    /// Get inline payload (panics if not inline)
    pub fn inline_payload(&self) -> &[u8] {
        assert!(self.is_inline(), "not an inline payload");
        &self.raw.inline_payload[..self.raw.payload_len as usize]
    }

    /// Get payload as InboundPayload (takes ownership for freeing)
    pub fn into_inbound_payload(self, seg: &DataSegment) -> InboundPayload<'_> {
        if self.is_inline() {
            InboundPayload::inline(seg)
        } else {
            InboundPayload::from_slot(
                seg,
                SlotIndex::new(self.raw.payload_slot),
                Generation::new(self.raw.payload_generation),
            )
        }
    }

    /// Get slot info for non-inline payloads
    pub fn slot_info(&self) -> Option<(SlotIndex, Generation, u32, u32)> {
        if self.is_inline() {
            None
        } else {
            Some((
                SlotIndex::new(self.raw.payload_slot),
                Generation::new(self.raw.payload_generation),
                self.raw.payload_offset,
                self.raw.payload_len,
            ))
        }
    }

    /// Get payload length
    pub fn payload_len(&self) -> u32 {
        self.raw.payload_len
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_builder_data() {
        let channel = ChannelId::new(1).unwrap();
        let method = MethodId::new(42);
        let msg_id = MsgId::new(100);

        let desc = FrameBuilder::data(channel, method, msg_id)
            .inline_payload(b"hello")
            .unwrap()
            .eos()
            .build();

        assert_eq!(desc.channel_id, 1);
        assert_eq!(desc.method_id, 42);
        assert_eq!(desc.msg_id, 100);
        assert_eq!(desc.payload_len, 5);
        assert_eq!(&desc.inline_payload[..5], b"hello");
        assert!(desc.flags & FrameFlags::EOS.bits() != 0);
    }

    #[test]
    fn frame_builder_control() {
        let desc = FrameBuilder::control(ControlMethod::Ping, MsgId::new(1))
            .inline_payload(&[1, 2, 3, 4, 5, 6, 7, 8])
            .unwrap()
            .build();

        assert_eq!(desc.channel_id, 0);
        assert_eq!(desc.method_id, ControlMethod::Ping as u32);
        assert!(desc.flags & FrameFlags::CONTROL.bits() != 0);
    }

    #[test]
    fn inline_payload_too_large() {
        let channel = ChannelId::new(1).unwrap();
        let result = FrameBuilder::data(channel, MethodId::new(1), MsgId::new(1))
            .inline_payload(&[0u8; 25]); // > 24 bytes

        assert_eq!(result.err(), Some(PayloadTooLarge));
    }

    #[test]
    fn frame_builder_credits() {
        let channel = ChannelId::new(1).unwrap();
        let desc = FrameBuilder::data(channel, MethodId::new(1), MsgId::new(1))
            .credits(1024)
            .build();

        assert_eq!(desc.credit_grant, 1024);
        assert!(desc.flags & FrameFlags::CREDITS.bits() != 0);
    }

    #[test]
    fn validate_inline_descriptor() {
        let channel = ChannelId::new(1).unwrap();
        let desc = FrameBuilder::data(channel, MethodId::new(1), MsgId::new(1))
            .inline_payload(b"test")
            .unwrap()
            .build();

        let raw = RawDescriptor::new(desc);
        let limits = DescriptorLimits::default();
        let valid = raw.validate_inline_only(&limits).unwrap();

        assert_eq!(valid.channel_id(), Some(channel));
        assert!(valid.is_inline());
        assert_eq!(valid.inline_payload(), b"test");
    }

    #[test]
    fn validate_rejects_oversized_inline() {
        let mut desc = MsgDescHot::default();
        desc.payload_slot = u32::MAX;
        desc.payload_len = 100; // Lies about size

        let raw = RawDescriptor::new(desc);
        let limits = DescriptorLimits::default();

        assert_eq!(
            raw.validate_inline_only(&limits).err(),
            Some(ValidationError::InlinePayloadTooLarge)
        );
    }
}
