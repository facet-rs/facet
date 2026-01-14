//! Segment layout types.
//!
//! Defines the segment header structure and layout computation for SHM segments.

use core::mem::size_of;
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};

/// Magic bytes for SHM segment identification.
///
/// shm[impl shm.segment.magic]
pub const MAGIC: [u8; 8] = *b"RAPAHUB\x01";

/// A slot size class for variable-size slot pools.
///
/// shm[impl shm.varslot.classes]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SizeClass {
    /// Size of each slot in this class (bytes).
    /// Must be at least 16 bytes (for VarSlotMeta overhead).
    pub slot_size: u32,
    /// Number of slots in this class.
    pub count: u32,
}

impl SizeClass {
    /// Create a new size class.
    ///
    /// # Panics
    ///
    /// Panics if `slot_size < 16` (minimum for VarSlotMeta).
    pub const fn new(slot_size: u32, count: u32) -> Self {
        assert!(slot_size >= 16, "slot_size must be >= 16");
        Self { slot_size, count }
    }
}

/// Segment header size in bytes.
///
/// shm[impl shm.segment.header-size]
pub const HEADER_SIZE: usize = 128;

/// Segment format version.
pub const VERSION: u32 = 1;

/// Peer entry size in bytes.
pub const PEER_ENTRY_SIZE: usize = 64;

/// Channel entry size in bytes.
pub const CHANNEL_ENTRY_SIZE: usize = 16;

/// Descriptor size in bytes (one cache line).
pub const DESC_SIZE: usize = 64;

/// Extent header size in bytes (one cache line).
pub const EXTENT_HEADER_SIZE: usize = 64;

/// Magic bytes for extent identification.
///
/// shm[impl shm.varslot.extent-layout]
pub const EXTENT_MAGIC: [u8; 8] = *b"ROAPEXT\x01";

/// Maximum number of extents per size class.
///
/// Each size class can have up to 3 extents (initial + 2 growth steps = 3x capacity).
pub const MAX_EXTENTS_PER_CLASS: usize = 3;

/// Header for an extent appended to the segment.
///
/// Each extent contains slots for a single size class and is appended to the
/// segment file when the host grows that class.
///
/// shm[impl shm.varslot.extent-layout]
#[repr(C, align(64))]
pub struct ExtentHeader {
    /// Magic bytes: "ROAPEXT\x01"
    pub magic: [u8; 8],
    /// Size class index this extent belongs to.
    pub class_idx: u32,
    /// Extent index within the class (1 or 2; extent 0 is the initial inline extent).
    pub extent_idx: u32,
    /// Number of slots in this extent.
    pub slot_count: u32,
    /// Size of each slot in bytes.
    pub slot_size: u32,
    /// Reserved for future use (zero).
    pub _reserved: [u8; 40],
}

const _: () = assert!(size_of::<ExtentHeader>() == EXTENT_HEADER_SIZE);

impl ExtentHeader {
    /// Validate the extent header.
    pub fn validate(&self) -> bool {
        self.magic == EXTENT_MAGIC
    }

    /// Calculate the total size of an extent (header + metadata + data).
    pub fn extent_size(slot_size: u32, slot_count: u32) -> u64 {
        let header_size = EXTENT_HEADER_SIZE as u64;
        // Metadata: 16 bytes per slot (VarSlotMeta), aligned to 16
        let meta_size = slot_count as u64 * 16;
        let meta_aligned = align_up(header_size + meta_size, 64);
        // Data: slot_size bytes per slot
        let data_size = slot_count as u64 * slot_size as u64;
        align_up(meta_aligned + data_size, 64)
    }

    /// Get the offset to the metadata array within this extent.
    #[inline]
    pub fn meta_offset() -> usize {
        EXTENT_HEADER_SIZE
    }

    /// Get the offset to the data array within this extent.
    #[inline]
    pub fn data_offset(slot_count: u32) -> usize {
        let meta_size = slot_count as usize * 16;
        align_up(EXTENT_HEADER_SIZE as u64 + meta_size as u64, 64) as usize
    }
}

/// Segment header at the start of the shared memory region.
///
/// shm[impl shm.segment.header]
#[repr(C)]
pub struct SegmentHeader {
    /// Magic bytes: "RAPAHUB\x01"
    pub magic: [u8; 8],
    /// Segment format version (1)
    pub version: u32,
    /// Size of this header (128)
    pub header_size: u32,
    /// Total segment size in bytes
    pub total_size: u64,
    /// Maximum payload per message
    pub max_payload_size: u32,
    /// Initial channel credit (bytes)
    pub initial_credit: u32,
    /// Maximum number of guests (≤ 255)
    ///
    /// shm[impl shm.topology.max-guests]
    pub max_guests: u32,
    /// Descriptor ring capacity (power of 2)
    pub ring_size: u32,
    /// Offset to peer table
    pub peer_table_offset: u64,
    /// Offset to payload slot region
    pub slot_region_offset: u64,
    /// Size of each payload slot
    pub slot_size: u32,
    /// Number of slots per guest
    pub slots_per_guest: u32,
    /// Max concurrent channels per guest
    pub max_channels: u32,
    /// Host goodbye flag (0 = active)
    ///
    /// shm[impl shm.goodbye.host]
    /// shm[impl shm.goodbye.host-atomic]
    pub host_goodbye: AtomicU32,
    /// Heartbeat interval in nanoseconds (0 = disabled)
    pub heartbeat_interval: u64,
    /// Offset to shared variable-size slot pool (0 = uses fixed per-guest pools).
    ///
    /// When non-zero, the segment uses a shared variable-size slot pool instead
    /// of fixed-size per-guest pools. Guests that don't support variable pools
    /// must reject attachment when this field is non-zero.
    ///
    /// shm[impl shm.varslot.shared]
    pub var_slot_pool_offset: u64,
    /// Current segment size in bytes.
    ///
    /// This may be larger than `total_size` if the segment has been grown
    /// via extent allocation. Guests compare this against their mapped size
    /// to detect when the host has grown the segment.
    ///
    /// shm[impl shm.varslot.extents]
    pub current_size: AtomicU64,
    /// Reserved for future use (zero)
    pub reserved: [u8; 32],
}

const _: () = assert!(size_of::<SegmentHeader>() == HEADER_SIZE);

impl SegmentHeader {
    /// Validate the segment header.
    ///
    /// Returns `true` if magic and version are correct.
    pub fn validate(&self) -> bool {
        self.magic == MAGIC && self.version == VERSION && self.header_size == HEADER_SIZE as u32
    }

    /// Check if the host has signaled goodbye.
    #[inline]
    pub fn is_host_goodbye(&self) -> bool {
        self.host_goodbye.load(Ordering::Acquire) != 0
    }

    /// Signal host goodbye with a reason code.
    #[inline]
    pub fn set_host_goodbye(&self, reason: u32) {
        self.host_goodbye.store(reason, Ordering::Release);
    }
}

/// Configuration for creating a new SHM segment.
#[derive(Debug, Clone)]
pub struct SegmentConfig {
    /// Maximum payload per message
    pub max_payload_size: u32,
    /// Initial channel credit (bytes)
    pub initial_credit: u32,
    /// Maximum number of guests (1-255)
    pub max_guests: u32,
    /// Descriptor ring capacity (must be power of 2)
    pub ring_size: u32,
    /// Size of each payload slot (for fixed-size pools)
    pub slot_size: u32,
    /// Number of slots per guest (for fixed-size pools)
    pub slots_per_guest: u32,
    /// Max concurrent channels per guest
    pub max_channels: u32,
    /// Heartbeat interval in nanoseconds (0 = disabled)
    pub heartbeat_interval: u64,
    /// Variable-size slot pool configuration (optional).
    ///
    /// If `Some`, uses a shared variable-size pool instead of per-guest fixed pools.
    /// shm[impl shm.varslot.shared]
    pub var_slot_classes: Option<Vec<SizeClass>>,
    /// File cleanup behavior for the backing file.
    ///
    /// Controls whether the file is automatically deleted when all processes die.
    pub file_cleanup: shm_primitives::FileCleanup,
}

impl Default for SegmentConfig {
    fn default() -> Self {
        let slot_size = 64 * 1024; // 64 KB slots
        Self {
            // Usable payload area is slot_size - 4 (generation counter).
            max_payload_size: slot_size - 4,
            initial_credit: 256 * 1024, // 256 KB
            max_guests: 16,
            ring_size: 256, // Power of 2
            slot_size,
            slots_per_guest: 16,
            max_channels: 256,
            heartbeat_interval: 0,  // Disabled by default
            var_slot_classes: None, // Use fixed-size pools by default
            file_cleanup: shm_primitives::FileCleanup::Manual,
        }
    }
}

impl SegmentConfig {
    /// Default size classes for variable-size slot pools.
    ///
    /// shm[impl shm.varslot.classes]
    ///
    /// Returns a configuration suitable for mixed workloads:
    /// - 1 KB × 1024 slots = 1 MB (small RPC args)
    /// - 16 KB × 256 slots = 4 MB (typical payloads)
    /// - 256 KB × 32 slots = 8 MB (images, CSS)
    /// - 4 MB × 8 slots = 32 MB (compressed fonts, large blobs)
    pub fn default_size_classes() -> Vec<SizeClass> {
        vec![
            SizeClass::new(1024, 1024),         // 1 KB × 1024
            SizeClass::new(16 * 1024, 256),     // 16 KB × 256
            SizeClass::new(256 * 1024, 32),     // 256 KB × 32
            SizeClass::new(4 * 1024 * 1024, 8), // 4 MB × 8
        ]
    }

    /// Create a config with variable-size slot pools using default size classes.
    pub fn with_var_slots() -> Self {
        let classes = Self::default_size_classes();
        let max_slot_size = classes.iter().map(|c| c.slot_size).max().unwrap_or(0);
        Self {
            max_payload_size: max_slot_size,
            var_slot_classes: Some(classes),
            ..Self::default()
        }
    }
}

impl SegmentConfig {
    /// Validate the configuration.
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.max_payload_size == 0 {
            return Err("max_payload_size must be > 0");
        }
        if self.max_guests == 0 || self.max_guests > 255 {
            return Err("max_guests must be 1-255");
        }
        if !self.ring_size.is_power_of_two() {
            return Err("ring_size must be power of 2");
        }
        if self.ring_size < 2 {
            return Err("ring_size must be at least 2");
        }
        if self.max_channels == 0 {
            return Err("max_channels must be > 0");
        }

        // Validate based on pool type
        if let Some(ref classes) = self.var_slot_classes {
            // Variable-size pool validation
            if classes.is_empty() {
                return Err("var_slot_classes must have at least one class");
            }
            if classes.len() > 256 {
                return Err("var_slot_classes must have at most 256 classes");
            }
            for (i, class) in classes.iter().enumerate() {
                if class.slot_size < 16 {
                    return Err("var_slot_classes slot_size must be >= 16");
                }
                if class.count == 0 {
                    return Err("var_slot_classes count must be > 0");
                }
                // Classes must be sorted by slot_size ascending
                if i > 0 && class.slot_size <= classes[i - 1].slot_size {
                    return Err("var_slot_classes must be sorted by slot_size ascending");
                }
            }
            let max_slot_size = classes.last().map(|c| c.slot_size).unwrap_or(0);
            if self.max_payload_size > max_slot_size {
                return Err("max_payload_size must be <= largest var_slot_class slot_size");
            }
        } else {
            // Fixed-size pool validation
            if self.slot_size < 8 {
                return Err("slot_size must be at least 8");
            }
            if !self.slot_size.is_multiple_of(8) {
                return Err("slot_size must be a multiple of 8");
            }
            if self.slots_per_guest == 0 {
                return Err("slots_per_guest must be > 0");
            }
            // Slot payload area is `slot_size - 4` bytes (generation counter).
            if self.max_payload_size > self.slot_size - 4 {
                return Err("max_payload_size must be <= slot_size - 4");
            }
        }

        Ok(())
    }

    /// Compute the segment layout from this configuration.
    pub fn layout(&self) -> Result<SegmentLayout, &'static str> {
        self.validate()?;
        Ok(SegmentLayout::new(self))
    }
}

/// Computed layout of a SHM segment.
///
/// Ring-related offsets are cache-line aligned (64 bytes).
#[derive(Debug, Clone)]
pub struct SegmentLayout {
    /// Configuration used to compute this layout
    pub config: SegmentConfig,
    /// Offset to peer table
    pub peer_table_offset: u64,
    /// Size of peer table in bytes
    pub peer_table_size: u64,
    /// Offset to slot region (host slots first, then guest slots for fixed pools)
    pub slot_region_offset: u64,
    /// Size of each slot pool (header + slots) - for fixed pools only
    ///
    /// shm[impl shm.segment.pool-size]
    pub pool_size: u64,
    /// Offset to shared variable-size slot pool (if using var slots)
    ///
    /// shm[impl shm.varslot.shared]
    pub var_slot_pool_offset: Option<u64>,
    /// Size of the shared variable-size slot pool
    pub var_slot_pool_size: u64,
    /// Offset to first guest area
    pub guest_areas_offset: u64,
    /// Size of each guest area (rings + channel table)
    pub guest_area_size: u64,
    /// Total segment size
    pub total_size: u64,
}

impl SegmentLayout {
    /// Compute the segment layout from configuration.
    fn new(config: &SegmentConfig) -> Self {
        // Peer table follows header
        let peer_table_offset = align_up(HEADER_SIZE as u64, 64);
        let peer_table_size = (config.max_guests as u64) * (PEER_ENTRY_SIZE as u64);

        // Slot region follows peer table
        let slot_region_offset = align_up(peer_table_offset + peer_table_size, 64);

        // Compute layout based on pool type
        let (pool_size, var_slot_pool_offset, var_slot_pool_size, slot_region_size) =
            if let Some(ref classes) = config.var_slot_classes {
                // Variable-size shared pool
                // shm[impl shm.varslot.shared]
                let var_pool_size = crate::var_slot_pool::VarSlotPool::calculate_size(classes);
                (0, Some(slot_region_offset), var_pool_size, var_pool_size)
            } else {
                // Fixed-size per-guest pools
                // Compute slot pool size per shm-spec:
                // pool_size = slot_pool_header_size + slots_per_guest * slot_size
                // where slot_pool_header_size is a bitmap header rounded up to 64 bytes.
                //
                // shm[impl shm.segment.pool-size]
                // shm[impl shm.slot.pool-header-size]
                let bitmap_words = (config.slots_per_guest as u64).div_ceil(64);
                let bitmap_bytes = bitmap_words * 8;
                let slot_pool_header_size = align_up(bitmap_bytes, 64);
                let pool_size = slot_pool_header_size
                    + (config.slots_per_guest as u64) * config.slot_size as u64;

                // Slot region contains:
                // - Host slot pool (position 0)
                // - One slot pool per potential guest (positions 1..=max_guests)
                //
                // shm[impl shm.segment.host-slots]
                // shm[impl shm.segment.guest-slot-offset]
                let slot_region_size = (config.max_guests as u64 + 1) * pool_size;
                (pool_size, None, 0, slot_region_size)
            };

        // Guest areas follow slot region
        let guest_areas_offset = align_up(slot_region_offset + slot_region_size, 64);

        // Each guest area contains:
        // - Guest→Host ring: ring_size * 64 bytes
        // - Host→Guest ring: ring_size * 64 bytes
        // - Channel table: max_channels * 16 bytes
        //
        // shm[impl shm.ring.layout]
        let rings_size = 2 * (config.ring_size as u64) * (DESC_SIZE as u64);
        let channel_table_size = (config.max_channels as u64) * (CHANNEL_ENTRY_SIZE as u64);
        let guest_area_size = align_up(rings_size, 64) + align_up(channel_table_size, 64);

        // Total size
        let total_size = guest_areas_offset + (config.max_guests as u64) * guest_area_size;

        Self {
            config: config.clone(),
            peer_table_offset,
            peer_table_size,
            slot_region_offset,
            pool_size,
            var_slot_pool_offset,
            var_slot_pool_size,
            guest_areas_offset,
            guest_area_size,
            total_size,
        }
    }

    /// Get the offset to a peer entry.
    ///
    /// shm[impl shm.topology.peer-id]
    #[inline]
    pub fn peer_entry_offset(&self, peer_id: u8) -> u64 {
        assert!(peer_id >= 1 && peer_id <= self.config.max_guests as u8);
        let index = (peer_id - 1) as u64;
        self.peer_table_offset + index * (PEER_ENTRY_SIZE as u64)
    }

    /// Get the offset to the host's slot pool.
    ///
    /// shm[impl shm.segment.host-slots]
    #[inline]
    pub fn host_slot_pool_offset(&self) -> u64 {
        self.slot_region_offset
    }

    /// Get the offset to a guest's area.
    #[inline]
    pub fn guest_area_offset(&self, peer_id: u8) -> u64 {
        assert!(peer_id >= 1 && peer_id <= self.config.max_guests as u8);
        let index = (peer_id - 1) as u64;
        self.guest_areas_offset + index * self.guest_area_size
    }

    /// Get the offset to a guest's rings.
    ///
    /// shm[impl shm.segment.guest-rings]
    #[inline]
    pub fn guest_rings_offset(&self, peer_id: u8) -> u64 {
        self.guest_area_offset(peer_id)
    }

    /// Get the offset to a guest's Guest→Host ring.
    #[inline]
    pub fn guest_to_host_ring_offset(&self, peer_id: u8) -> u64 {
        self.guest_rings_offset(peer_id)
    }

    /// Get the offset to a guest's Host→Guest ring.
    #[inline]
    pub fn host_to_guest_ring_offset(&self, peer_id: u8) -> u64 {
        self.guest_rings_offset(peer_id) + (self.config.ring_size as u64) * (DESC_SIZE as u64)
    }

    /// Get the offset to a guest's slot pool.
    ///
    /// shm[impl shm.segment.guest-slot-offset]
    #[inline]
    pub fn guest_slot_pool_offset(&self, peer_id: u8) -> u64 {
        assert!(peer_id >= 1 && peer_id <= self.config.max_guests as u8);
        self.slot_region_offset + (peer_id as u64) * self.pool_size
    }

    /// Get the offset to a guest's channel table.
    ///
    /// shm[impl shm.flow.channel-table-location]
    #[inline]
    pub fn guest_channel_table_offset(&self, peer_id: u8) -> u64 {
        let rings_size = 2 * (self.config.ring_size as u64) * (DESC_SIZE as u64);
        self.guest_area_offset(peer_id) + align_up(rings_size, 64)
    }

    /// Check if this layout uses variable-size slot pools.
    #[inline]
    pub fn uses_var_slots(&self) -> bool {
        self.var_slot_pool_offset.is_some()
    }

    /// Get the offset to the shared variable-size slot pool.
    ///
    /// Returns `None` if using fixed-size per-guest pools.
    #[inline]
    pub fn var_slot_pool_offset(&self) -> Option<u64> {
        self.var_slot_pool_offset
    }
}

/// Align a value up to the given alignment.
#[inline]
const fn align_up(value: u64, align: u64) -> u64 {
    (value + (align - 1)) & !(align - 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_size_is_128() {
        assert_eq!(size_of::<SegmentHeader>(), 128);
    }

    #[test]
    fn default_config_is_valid() {
        let config = SegmentConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn layout_offsets_are_aligned() {
        let config = SegmentConfig::default();
        let layout = config.layout().unwrap();

        assert_eq!(layout.peer_table_offset % 64, 0);
        assert_eq!(layout.slot_region_offset % 64, 0);
        assert_eq!(layout.guest_areas_offset % 64, 0);

        for peer_id in 1..=config.max_guests as u8 {
            assert_eq!(layout.guest_area_offset(peer_id) % 64, 0);
            assert_eq!(layout.guest_rings_offset(peer_id) % 64, 0);
            assert_eq!(layout.guest_channel_table_offset(peer_id) % 64, 0);
        }
    }

    #[test]
    fn invalid_configs_are_rejected() {
        let config = SegmentConfig {
            max_guests: 0,
            ..Default::default()
        };
        assert!(config.validate().is_err());

        let mut config = config;

        config.max_guests = 256;
        assert!(config.validate().is_err());

        config.max_guests = 16;
        config.ring_size = 3; // Not power of 2
        assert!(config.validate().is_err());
    }

    #[test]
    fn pool_size_matches_bitmap_layout() {
        let config = SegmentConfig::default();
        let layout = config.layout().unwrap();

        let bitmap_words = (config.slots_per_guest as u64).div_ceil(64);
        let bitmap_bytes = bitmap_words * 8;
        let header_size = align_up(bitmap_bytes, 64);
        let expected_pool_size =
            header_size + (config.slots_per_guest as u64) * (config.slot_size as u64);

        assert_eq!(layout.pool_size, expected_pool_size);
    }

    #[test]
    fn guest_area_regions_do_not_overlap() {
        let config = SegmentConfig::default();
        let layout = config.layout().unwrap();

        let rings_size = 2 * (config.ring_size as u64) * (DESC_SIZE as u64);
        let channel_table_size = (config.max_channels as u64) * (CHANNEL_ENTRY_SIZE as u64);

        for peer_id in 1..=config.max_guests as u8 {
            let rings_start = layout.guest_rings_offset(peer_id);
            let channel_table_start = layout.guest_channel_table_offset(peer_id);
            let rings_end = rings_start + align_up(rings_size, 64);

            assert!(
                channel_table_start >= rings_end,
                "Guest {} channel table (offset {}) overlaps rings (end at {})!",
                peer_id,
                channel_table_start,
                rings_end
            );

            let channel_table_end = channel_table_start + align_up(channel_table_size, 64);
            let area_end = layout.guest_area_offset(peer_id) + layout.guest_area_size;
            assert!(
                channel_table_end <= area_end,
                "Guest {} channel table (end {}) exceeds guest area end ({})!",
                peer_id,
                channel_table_end,
                area_end
            );
        }
    }
}
