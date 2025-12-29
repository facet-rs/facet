//! Descriptor validation.
//!
//! See spec: [Frame Format](https://rapace.dev/spec/frame-format/)

use crate::{
    DescriptorLimits, INLINE_PAYLOAD_SIZE, INLINE_PAYLOAD_SLOT, MsgDescHot, ValidationError,
};

/// Validate a descriptor against the given limits.
///
/// This performs transport-agnostic validation:
/// - Bounds checking for slots (if slot_count > 0)
/// - Payload length limits
/// - Inline payload constraints
///
/// Transport-specific validation (e.g., generation checks) happens in the transport.
///
/// Related spec rules:
/// - `[impl frame.payload.inline]` - inline payloads ≤16 bytes, slot=0xFFFFFFFF
/// - `[impl frame.payload.out-of-line]` - slot-based for larger payloads
/// - `[impl frame.sentinel.values]` - sentinel value semantics
pub fn validate_descriptor(
    desc: &MsgDescHot,
    limits: &DescriptorLimits,
) -> Result<(), ValidationError> {
    // Check payload length limit
    if desc.payload_len > limits.max_payload_len {
        return Err(ValidationError::PayloadTooLarge {
            len: desc.payload_len,
            max: limits.max_payload_len,
        });
    }

    // Check channel bounds
    if desc.channel_id > limits.max_channels {
        return Err(ValidationError::ChannelOutOfBounds {
            channel: desc.channel_id,
            max: limits.max_channels,
        });
    }

    if desc.payload_slot == INLINE_PAYLOAD_SLOT {
        // Inline payload validation
        // Spec: `[impl frame.payload.inline]` - inline when payload_len ≤ 16
        if desc.payload_len > INLINE_PAYLOAD_SIZE as u32 {
            return Err(ValidationError::InlinePayloadTooLarge {
                len: desc.payload_len,
                max: INLINE_PAYLOAD_SIZE as u32,
            });
        }
        // Inline descriptors must have zero slot-related fields
        // (payload_offset and payload_generation are ignored per spec)
        if desc.payload_generation != 0 || desc.payload_offset != 0 {
            return Err(ValidationError::InvalidInlineDescriptor);
        }
    } else if limits.slot_count > 0 {
        // Slot-based payload validation (only if we have slots)
        if desc.payload_slot >= limits.slot_count {
            return Err(ValidationError::SlotOutOfBounds {
                slot: desc.payload_slot,
                max: limits.slot_count,
            });
        }

        let end = desc.payload_offset.saturating_add(desc.payload_len);
        if end > limits.slot_size {
            return Err(ValidationError::PayloadOutOfBounds {
                offset: desc.payload_offset,
                len: desc.payload_len,
                slot_size: limits.slot_size,
            });
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_desc() -> MsgDescHot {
        MsgDescHot::new()
    }

    #[test]
    fn test_valid_inline_payload() {
        let mut desc = make_desc();
        desc.payload_len = 10;
        desc.inline_payload[..10].fill(0xAB);

        let limits = DescriptorLimits::default();
        assert!(validate_descriptor(&desc, &limits).is_ok());
    }

    #[test]
    fn test_inline_payload_too_large() {
        let mut desc = make_desc();
        desc.payload_len = 30; // > INLINE_PAYLOAD_SIZE (24)

        let limits = DescriptorLimits::default();
        let err = validate_descriptor(&desc, &limits).unwrap_err();
        assert!(matches!(err, ValidationError::InlinePayloadTooLarge { .. }));
    }

    #[test]
    fn test_invalid_inline_descriptor() {
        let mut desc = make_desc();
        desc.payload_len = 10;
        desc.payload_generation = 1; // Should be 0 for inline

        let limits = DescriptorLimits::default();
        let err = validate_descriptor(&desc, &limits).unwrap_err();
        assert!(matches!(err, ValidationError::InvalidInlineDescriptor));
    }

    #[test]
    fn test_payload_too_large() {
        let mut desc = make_desc();
        desc.payload_slot = 0;
        desc.payload_len = 2 * 1024 * 1024; // 2MB > default 1MB limit

        let limits = DescriptorLimits::with_slots(16, 4096);
        let err = validate_descriptor(&desc, &limits).unwrap_err();
        assert!(matches!(err, ValidationError::PayloadTooLarge { .. }));
    }

    #[test]
    fn test_slot_out_of_bounds() {
        let mut desc = make_desc();
        desc.payload_slot = 100;
        desc.payload_len = 100;

        let limits = DescriptorLimits::with_slots(16, 4096);
        let err = validate_descriptor(&desc, &limits).unwrap_err();
        assert!(matches!(err, ValidationError::SlotOutOfBounds { .. }));
    }

    #[test]
    fn test_payload_out_of_bounds() {
        let mut desc = make_desc();
        desc.payload_slot = 0;
        desc.payload_offset = 4000;
        desc.payload_len = 200; // 4000 + 200 = 4200 > 4096

        let limits = DescriptorLimits::with_slots(16, 4096);
        let err = validate_descriptor(&desc, &limits).unwrap_err();
        assert!(matches!(err, ValidationError::PayloadOutOfBounds { .. }));
    }

    #[test]
    fn test_channel_out_of_bounds() {
        let mut desc = make_desc();
        desc.channel_id = 2000;

        let limits = DescriptorLimits::default();
        let err = validate_descriptor(&desc, &limits).unwrap_err();
        assert!(matches!(err, ValidationError::ChannelOutOfBounds { .. }));
    }

    #[test]
    fn test_valid_slot_payload() {
        let mut desc = make_desc();
        desc.payload_slot = 5;
        desc.payload_generation = 42;
        desc.payload_offset = 100;
        desc.payload_len = 500;

        let limits = DescriptorLimits::with_slots(16, 4096);
        assert!(validate_descriptor(&desc, &limits).is_ok());
    }
}
