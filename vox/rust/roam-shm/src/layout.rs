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
///
/// v1: MsgDesc (64-byte fixed descriptors) + per-guest slot pools
/// v2: BipBuffer (variable-length byte SPSC) + shared VarSlotPool
pub const VERSION: u32 = 2;

/// Previous version (for error messages when encountering old segments).
pub const VERSION_V1: u32 = 1;

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
    /// v1: Descriptor ring capacity (power of 2)
    /// v2: BipBuffer data region size in bytes
    pub ring_size: u32,
    /// Offset to peer table
    pub peer_table_offset: u64,
    /// Offset to payload slot region
    pub slot_region_offset: u64,
    /// v1: Size of each payload slot
    /// v2: Must be 0 (fixed pools eliminated)
    pub slot_size: u32,
    /// v1: Number of slots per guest
    /// v2: Inline threshold (max inline payload, 0 = default 256)
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
    /// Offset to guest areas (BipBuffers + channel tables).
    ///
    /// Guests read this directly rather than computing it from var_slot_pool
    /// size (which requires knowing the size classes).
    pub guest_areas_offset: u64,
    /// Number of variable-size slot classes.
    pub num_var_slot_classes: u32,
    /// Reserved for future use (zero)
    pub reserved: [u8; 20],
}

const _: () = assert!(size_of::<SegmentHeader>() == HEADER_SIZE);

impl SegmentHeader {
    /// Validate the segment header.
    ///
    /// Returns an error string if invalid, or Ok(()) if valid.
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.magic != MAGIC {
            return Err("invalid magic bytes");
        }
        if self.version == VERSION_V1 {
            return Err("this is a v1 segment, upgrade required");
        }
        if self.version != VERSION {
            return Err("unsupported segment version");
        }
        if self.header_size != HEADER_SIZE as u32 {
            return Err("invalid header size");
        }
        if self.slot_size != 0 {
            return Err("v2 segment must have slot_size = 0 (fixed pools eliminated)");
        }
        if self.var_slot_pool_offset == 0 {
            return Err("v2 segment must have non-zero var_slot_pool_offset");
        }
        Ok(())
    }

    /// Returns true if the header has valid magic and version.
    pub fn is_valid(&self) -> bool {
        self.validate().is_ok()
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

/// Configuration for creating a new SHM segment (v2).
#[derive(Debug, Clone)]
pub struct SegmentConfig {
    /// Maximum payload per message.
    pub max_payload_size: u32,
    /// Initial channel credit (bytes).
    pub initial_credit: u32,
    /// Maximum number of guests (1-255).
    pub max_guests: u32,
    /// BipBuffer data region size in bytes per direction.
    ///
    /// Each guest gets two BipBuffers (G→H and H→G), each with this capacity.
    pub bipbuf_capacity: u32,
    /// Inline threshold: frames with `header_size + payload_len <= inline_threshold`
    /// go inline in the BipBuffer. Larger payloads use VarSlotPool.
    /// 0 means use the default (256 bytes).
    pub inline_threshold: u32,
    /// Max concurrent channels per guest.
    pub max_channels: u32,
    /// Heartbeat interval in nanoseconds (0 = disabled).
    pub heartbeat_interval: u64,
    /// Variable-size slot pool configuration (mandatory in v2).
    ///
    /// shm[impl shm.varslot.shared]
    pub var_slot_classes: Vec<SizeClass>,
    /// File cleanup behavior for the backing file.
    pub file_cleanup: shm_primitives::FileCleanup,
}

impl Default for SegmentConfig {
    fn default() -> Self {
        let classes = Self::default_size_classes();
        let max_slot_size = classes.iter().map(|c| c.slot_size).max().unwrap_or(0);
        Self {
            max_payload_size: max_slot_size,
            initial_credit: 256 * 1024, // 256 KB
            max_guests: 16,
            bipbuf_capacity: 65536, // 64 KB per direction
            inline_threshold: 0,    // 0 = default (256 bytes)
            max_channels: 256,
            heartbeat_interval: 0, // Disabled by default
            var_slot_classes: classes,
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

    /// Get the effective inline threshold (resolving 0 to default).
    #[inline]
    pub fn effective_inline_threshold(&self) -> u32 {
        if self.inline_threshold == 0 {
            roam_frame::DEFAULT_INLINE_THRESHOLD
        } else {
            self.inline_threshold
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
        if self.bipbuf_capacity == 0 {
            return Err("bipbuf_capacity must be > 0");
        }
        if self.max_channels == 0 {
            return Err("max_channels must be > 0");
        }

        // Variable-size pool validation (mandatory in v2)
        let classes = &self.var_slot_classes;
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

        Ok(())
    }

    /// Compute the segment layout from this configuration.
    pub fn layout(&self) -> Result<SegmentLayout, &'static str> {
        self.validate()?;
        Ok(SegmentLayout::new(self))
    }
}

/// Computed layout of a SHM segment (v2).
///
/// All offsets are cache-line aligned (64 bytes).
#[derive(Debug, Clone)]
pub struct SegmentLayout {
    /// Configuration used to compute this layout
    pub config: SegmentConfig,
    /// Offset to peer table
    pub peer_table_offset: u64,
    /// Size of peer table in bytes
    pub peer_table_size: u64,
    /// Offset to shared variable-size slot pool
    ///
    /// shm[impl shm.varslot.shared]
    pub var_slot_pool_offset: u64,
    /// Size of the shared variable-size slot pool
    pub var_slot_pool_size: u64,
    /// Offset to first guest area
    pub guest_areas_offset: u64,
    /// Size of each guest area (BipBuffers + channel table)
    pub guest_area_size: u64,
    /// Size of one BipBuffer (header + data region)
    pub bipbuf_size: u64,
    /// Total segment size
    pub total_size: u64,
}

/// BipBuffer header size in bytes (2 cache lines).
///
/// shm[impl shm.bipbuf.layout]
pub const BIPBUF_HEADER_SIZE: usize = shm_primitives::BIPBUF_HEADER_SIZE;

impl SegmentLayout {
    /// Compute the segment layout from configuration.
    fn new(config: &SegmentConfig) -> Self {
        // Peer table follows header
        let peer_table_offset = align_up(HEADER_SIZE as u64, 64);
        let peer_table_size = (config.max_guests as u64) * (PEER_ENTRY_SIZE as u64);

        // Shared variable-size slot pool follows peer table (mandatory in v2)
        let var_slot_pool_offset = align_up(peer_table_offset + peer_table_size, 64);
        let var_slot_pool_size =
            crate::var_slot_pool::VarSlotPool::calculate_size(&config.var_slot_classes);

        // Guest areas follow var slot pool
        let guest_areas_offset = align_up(var_slot_pool_offset + var_slot_pool_size, 64);

        // Each guest area contains:
        // - G2H BipBuffer header (128 bytes) + data (bipbuf_capacity bytes)
        // - H2G BipBuffer header (128 bytes) + data (bipbuf_capacity bytes)
        // - Channel table (max_channels × 16 bytes)
        let bipbuf_size = BIPBUF_HEADER_SIZE as u64 + config.bipbuf_capacity as u64;
        let bipbufs_size = 2 * bipbuf_size;
        let channel_table_size = (config.max_channels as u64) * (CHANNEL_ENTRY_SIZE as u64);
        let guest_area_size = align_up(bipbufs_size, 64) + align_up(channel_table_size, 64);

        // Total size
        let total_size = guest_areas_offset + (config.max_guests as u64) * guest_area_size;

        Self {
            config: config.clone(),
            peer_table_offset,
            peer_table_size,
            var_slot_pool_offset,
            var_slot_pool_size,
            guest_areas_offset,
            guest_area_size,
            bipbuf_size,
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

    /// Get the offset to a guest's area.
    #[inline]
    pub fn guest_area_offset(&self, peer_id: u8) -> u64 {
        assert!(peer_id >= 1 && peer_id <= self.config.max_guests as u8);
        let index = (peer_id - 1) as u64;
        self.guest_areas_offset + index * self.guest_area_size
    }

    /// Get the offset to a guest's Guest→Host BipBuffer header.
    #[inline]
    pub fn guest_to_host_bipbuf_offset(&self, peer_id: u8) -> u64 {
        self.guest_area_offset(peer_id)
    }

    /// Get the offset to a guest's Host→Guest BipBuffer header.
    #[inline]
    pub fn host_to_guest_bipbuf_offset(&self, peer_id: u8) -> u64 {
        self.guest_area_offset(peer_id) + self.bipbuf_size
    }

    /// Get the offset to a guest's channel table.
    ///
    /// shm[impl shm.flow.channel-table-location]
    #[inline]
    pub fn guest_channel_table_offset(&self, peer_id: u8) -> u64 {
        let bipbufs_size = 2 * self.bipbuf_size;
        self.guest_area_offset(peer_id) + align_up(bipbufs_size, 64)
    }

    /// Get the offset to the shared variable-size slot pool.
    #[inline]
    pub fn var_slot_pool_offset(&self) -> u64 {
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
        assert_eq!(layout.var_slot_pool_offset % 64, 0);
        assert_eq!(layout.guest_areas_offset % 64, 0);

        for peer_id in 1..=config.max_guests as u8 {
            assert_eq!(layout.guest_area_offset(peer_id) % 64, 0);
            assert_eq!(layout.guest_channel_table_offset(peer_id) % 64, 0);
        }
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn invalid_configs_are_rejected() {
        let mut config = SegmentConfig::default();

        config.max_guests = 0;
        assert!(config.validate().is_err());

        config.max_guests = 256;
        assert!(config.validate().is_err());

        config.max_guests = 16;
        config.bipbuf_capacity = 0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn guest_area_regions_do_not_overlap() {
        let config = SegmentConfig::default();
        let layout = config.layout().unwrap();

        let bipbufs_size = 2 * layout.bipbuf_size;
        let channel_table_size = (config.max_channels as u64) * (CHANNEL_ENTRY_SIZE as u64);

        for peer_id in 1..=config.max_guests as u8 {
            let bipbufs_start = layout.guest_to_host_bipbuf_offset(peer_id);
            let channel_table_start = layout.guest_channel_table_offset(peer_id);
            let bipbufs_end = bipbufs_start + align_up(bipbufs_size, 64);

            assert!(
                channel_table_start >= bipbufs_end,
                "Guest {} channel table (offset {}) overlaps bipbufs (end at {})!",
                peer_id,
                channel_table_start,
                bipbufs_end
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

    #[test]
    fn bipbuf_size_is_correct() {
        let config = SegmentConfig::default();
        let layout = config.layout().unwrap();

        assert_eq!(
            layout.bipbuf_size,
            BIPBUF_HEADER_SIZE as u64 + config.bipbuf_capacity as u64
        );
    }

    #[test]
    fn effective_inline_threshold_defaults() {
        let config = SegmentConfig::default();
        assert_eq!(config.inline_threshold, 0);
        assert_eq!(
            config.effective_inline_threshold(),
            roam_frame::DEFAULT_INLINE_THRESHOLD
        );

        let config = SegmentConfig {
            inline_threshold: 512,
            ..Default::default()
        };
        assert_eq!(config.effective_inline_threshold(), 512);
    }
}
