//! Resource limits for validation.

/// Limits for descriptor validation.
#[derive(Debug, Clone)]
pub struct DescriptorLimits {
    /// Maximum payload length (default: 1MB).
    pub max_payload_len: u32,
    /// Maximum number of channels (default: 1024).
    pub max_channels: u32,
    /// Maximum frames in flight per channel (default: 64).
    pub max_frames_in_flight_per_channel: u32,
    /// Maximum frames in flight total (default: 4096).
    pub max_frames_in_flight_total: u32,
    /// Number of slots (for SHM validation).
    pub slot_count: u32,
    /// Size of each slot in bytes (for SHM validation).
    pub slot_size: u32,
}

impl Default for DescriptorLimits {
    fn default() -> Self {
        Self {
            max_payload_len: 1024 * 1024, // 1MB
            max_channels: 1024,
            max_frames_in_flight_per_channel: 64,
            max_frames_in_flight_total: 4096,
            slot_count: 0, // No slots by default (non-SHM)
            slot_size: 0,
        }
    }
}

impl DescriptorLimits {
    /// Create limits for non-SHM transports (no slot validation).
    pub fn without_slots() -> Self {
        Self::default()
    }

    /// Create limits for SHM transport with the given slot configuration.
    pub fn with_slots(slot_count: u32, slot_size: u32) -> Self {
        Self {
            slot_count,
            slot_size,
            ..Default::default()
        }
    }
}
