//! Hub SHM memory layout definitions.
//!
//! This module defines the `repr(C)` structures for the multi-peer hub architecture.
//! Unlike the original two-peer session model, the hub supports many plugins sharing
//! a single SHM file with variable-size slot allocation via size classes.
//!
//! # Memory Layout
//!
//! ```text
//! +-------------------------------------------------------------------+
//! | HUB HEADER (256 bytes)                                            |
//! |   magic: "RAPAHUB\0", version, max_peers, peer_id_counter         |
//! |   current_size (atomic), extent_count (atomic)                    |
//! +-------------------------------------------------------------------+
//! | PEER TABLE (max_peers entries, 64 bytes each)                     |
//! |   Per peer: peer_id, flags, epoch, last_seen, futex words         |
//! |   Per peer: send_ring_offset, recv_ring_offset                    |
//! +-------------------------------------------------------------------+
//! | RING REGION (max_peers * 2 rings * ~17KB each)                    |
//! |   Each ring: DescRingHeader (192B) + capacity * MsgDescHot (64B)  |
//! +-------------------------------------------------------------------+
//! | SIZE CLASS HEADERS (NUM_SIZE_CLASSES * 128 bytes)                 |
//! |   Per class: slot_size, free_head, slot_available, extent_offsets |
//! +-------------------------------------------------------------------+
//! | EXTENT REGION (growable, appended at end)                         |
//! |   Extent 0: [ExtentHeader][SlotMeta*N][SlotData*N]                |
//! |   Extent 1: [ExtentHeader][SlotMeta*N][SlotData*N]                |
//! |   ... (new extents appended on growth)                            |
//! +-------------------------------------------------------------------+
//! ```

use std::sync::atomic::{AtomicU16, AtomicU32, AtomicU64, Ordering};

use rapace_core::MsgDescHot;

// =============================================================================
// Constants
// =============================================================================

/// Magic bytes identifying a rapace hub SHM segment.
pub const HUB_MAGIC: [u8; 8] = *b"RAPAHUB\0";

/// Current hub protocol version (major.minor packed into u32).
pub const HUB_PROTOCOL_VERSION: u32 = 1 << 16; // v1.0

/// Maximum number of peers supported.
pub const MAX_PEERS: u16 = 32;

/// Number of size classes.
pub const NUM_SIZE_CLASSES: usize = 5;

/// Maximum extents per size class.
pub const MAX_EXTENTS_PER_CLASS: usize = 16;

/// Default descriptor ring capacity per peer (power of 2).
pub const DEFAULT_HUB_RING_CAPACITY: u32 = 256;

/// Sentinel value indicating end of free list.
pub const FREE_LIST_END: u32 = u32::MAX;

/// Sentinel value indicating no owner.
pub const NO_OWNER: u32 = u32::MAX;

/// Size class configuration: (slot_size, initial_slot_count).
pub const HUB_SIZE_CLASSES: [(u32, u32); NUM_SIZE_CLASSES] = [
    (1024, 1024),  // 1KB * 1024 = 1MB (small RPC args)
    (16384, 256),  // 16KB * 256 = 4MB (typical payloads)
    (262144, 32),  // 256KB * 32 = 8MB (images, CSS)
    (4194304, 8),  // 4MB * 8 = 32MB (compressed fonts)
    (16777216, 4), // 16MB * 4 = 64MB (decompressed fonts)
];

// =============================================================================
// Peer Flags
// =============================================================================

/// Peer is active and healthy.
pub const PEER_FLAG_ACTIVE: u32 = 1 << 0;
/// Peer is being shut down.
pub const PEER_FLAG_DYING: u32 = 1 << 1;
/// Peer has died (crash detected).
pub const PEER_FLAG_DEAD: u32 = 1 << 2;
/// Peer slot is reserved by host but not yet claimed by plugin.
pub const PEER_FLAG_RESERVED: u32 = 1 << 3;

// =============================================================================
// Hub Header
// =============================================================================

/// Hub header at the start of the SHM region (256 bytes).
///
/// Contains version info, peer management, and size tracking for growth.
#[repr(C, align(64))]
pub struct HubHeader {
    /// Magic bytes: "RAPAHUB\0".
    pub magic: [u8; 8],
    /// Protocol version (major.minor packed).
    pub version: u32,
    /// Feature flags.
    pub flags: u32,

    /// Maximum number of peers this hub supports.
    pub max_peers: u16,
    /// Number of currently active peers.
    pub active_peers: AtomicU16,
    /// Counter for allocating peer IDs.
    pub peer_id_counter: AtomicU16,
    /// Reserved.
    pub _pad1: u16,

    /// Current mapped size of the SHM file (for growth detection).
    pub current_size: AtomicU64,
    /// Number of extents currently allocated.
    pub extent_count: AtomicU32,
    /// Ring capacity per peer.
    pub ring_capacity: u32,

    /// Offset to peer table from start of file.
    pub peer_table_offset: u64,
    /// Offset to ring region from start of file.
    pub ring_region_offset: u64,
    /// Offset to size class headers from start of file.
    pub size_class_offset: u64,
    /// Offset to extent region from start of file.
    pub extent_region_offset: u64,

    /// Padding to 256 bytes.
    pub _pad2: [u8; 168],
}

const _: () = assert!(core::mem::size_of::<HubHeader>() == 256);

impl HubHeader {
    /// Initialize a new hub header.
    pub fn init(&mut self, max_peers: u16, ring_capacity: u32) {
        self.magic = HUB_MAGIC;
        self.version = HUB_PROTOCOL_VERSION;
        self.flags = 0;
        self.max_peers = max_peers;
        self.active_peers = AtomicU16::new(0);
        self.peer_id_counter = AtomicU16::new(0);
        self._pad1 = 0;
        self.current_size = AtomicU64::new(0);
        self.extent_count = AtomicU32::new(0);
        self.ring_capacity = ring_capacity;
        self.peer_table_offset = 0;
        self.ring_region_offset = 0;
        self.size_class_offset = 0;
        self.extent_region_offset = 0;
        self._pad2 = [0; 168];
    }

    /// Validate the header.
    pub fn validate(&self) -> Result<(), HubLayoutError> {
        if self.magic != HUB_MAGIC {
            return Err(HubLayoutError::InvalidMagic);
        }
        let major = self.version >> 16;
        let our_major = HUB_PROTOCOL_VERSION >> 16;
        if major != our_major {
            return Err(HubLayoutError::IncompatibleVersion {
                expected: HUB_PROTOCOL_VERSION,
                found: self.version,
            });
        }
        if self.max_peers == 0 || self.max_peers > MAX_PEERS {
            return Err(HubLayoutError::InvalidConfig("max_peers out of range"));
        }
        if !self.ring_capacity.is_power_of_two() || self.ring_capacity == 0 {
            return Err(HubLayoutError::InvalidConfig(
                "ring_capacity must be non-zero power of 2",
            ));
        }
        Ok(())
    }
}

// =============================================================================
// Peer Entry
// =============================================================================

/// Entry in the peer table (64 bytes).
///
/// Each plugin gets one peer entry with its rings referenced by offset.
#[repr(C, align(64))]
pub struct PeerEntry {
    /// Peer ID (0 = host, 1+ = plugins).
    pub peer_id: u16,
    /// Peer type (0 = host, 1 = plugin).
    pub peer_type: u16,
    /// Flags (ACTIVE, DYING, DEAD, RESERVED).
    pub flags: AtomicU32,

    /// Heartbeat epoch counter.
    pub epoch: AtomicU64,
    /// Last seen timestamp (nanos since Unix epoch).
    pub last_seen: AtomicU64,

    /// Offset to this peer's send ring (peer -> host).
    pub send_ring_offset: u64,
    /// Offset to this peer's recv ring (host -> peer).
    pub recv_ring_offset: u64,

    /// Futex for send ring data available.
    pub send_data_futex: AtomicU32,
    /// Futex for recv ring data available.
    pub recv_data_futex: AtomicU32,
}

const _: () = assert!(core::mem::size_of::<PeerEntry>() == 64);

impl PeerEntry {
    /// Initialize a new peer entry.
    pub fn init(&mut self, peer_id: u16, peer_type: u16) {
        self.peer_id = peer_id;
        self.peer_type = peer_type;
        self.flags = AtomicU32::new(0);
        self.epoch = AtomicU64::new(0);
        self.last_seen = AtomicU64::new(0);
        self.send_ring_offset = 0;
        self.recv_ring_offset = 0;
        self.send_data_futex = AtomicU32::new(0);
        self.recv_data_futex = AtomicU32::new(0);
    }

    /// Check if this peer is active.
    #[inline]
    pub fn is_active(&self) -> bool {
        self.flags.load(Ordering::Acquire) & PEER_FLAG_ACTIVE != 0
    }

    /// Check if this peer is dead.
    #[inline]
    pub fn is_dead(&self) -> bool {
        self.flags.load(Ordering::Acquire) & PEER_FLAG_DEAD != 0
    }

    /// Mark this peer as active.
    pub fn mark_active(&self) {
        self.flags.fetch_or(PEER_FLAG_ACTIVE, Ordering::Release);
        self.flags.fetch_and(
            !(PEER_FLAG_DYING | PEER_FLAG_DEAD | PEER_FLAG_RESERVED),
            Ordering::Release,
        );
    }

    /// Mark this peer as dead.
    pub fn mark_dead(&self) {
        self.flags.fetch_or(PEER_FLAG_DEAD, Ordering::Release);
        self.flags
            .fetch_and(!(PEER_FLAG_ACTIVE | PEER_FLAG_DYING), Ordering::Release);
    }
}

// =============================================================================
// Size Class Header
// =============================================================================

/// Header for a size class (128 bytes).
///
/// Each size class has its own Treiber stack free list and extent tracking.
#[repr(C, align(64))]
pub struct SizeClassHeader {
    /// Size of slots in this class (bytes).
    pub slot_size: u32,
    /// Total slots currently available in this class.
    pub total_slots: AtomicU32,

    /// Free list head: (tag << 32) | global_index.
    /// Uses tagged pointer for ABA safety.
    pub free_head: AtomicU64,

    /// Futex for slot availability signaling.
    pub slot_available: AtomicU32,

    /// log2(slots per extent) for fast index decoding.
    pub extent_slot_shift: u8,
    /// Number of extents in this class.
    pub extent_count: u8,
    /// Reserved.
    pub _pad1: [u8; 2],

    /// Extent directory: maps extent_id -> file offset.
    /// AtomicU64 to avoid data races when host adds extents.
    pub extent_offsets: [AtomicU64; MAX_EXTENTS_PER_CLASS],
}

const _: () = assert!(core::mem::size_of::<SizeClassHeader>() == 64 + MAX_EXTENTS_PER_CLASS * 8);
// 64 + 16*8 = 192 bytes... let me recalculate

// Actually: 4 + 4 + 8 + 4 + 1 + 1 + 2 + 16*8 = 24 + 128 = 152 bytes
// We need padding to reach 192 or some cache-line multiple

impl SizeClassHeader {
    /// Initialize a size class header.
    pub fn init(&mut self, slot_size: u32, extent_slot_shift: u8) {
        self.slot_size = slot_size;
        self.total_slots = AtomicU32::new(0);
        self.free_head = AtomicU64::new(pack_free_head(FREE_LIST_END, 0));
        self.slot_available = AtomicU32::new(0);
        self.extent_slot_shift = extent_slot_shift;
        self.extent_count = 0;
        self._pad1 = [0; 2];
        for offset in &self.extent_offsets {
            // Use a store since we're initializing
            offset.store(0, Ordering::Relaxed);
        }
    }
}

// =============================================================================
// Extent Header
// =============================================================================

/// Header for an extent within a size class (64 bytes).
///
/// Each extent is self-contained with its own slot metadata and data.
#[repr(C, align(64))]
pub struct ExtentHeader {
    /// Which size class this extent belongs to (0-4).
    pub size_class: u8,
    /// Reserved.
    pub _pad1: [u8; 3],
    /// Number of slots in this extent.
    pub slot_count: u32,

    /// File offset of next extent in this class (0 if last).
    pub next_extent_offset: AtomicU64,

    /// Base global index for slots in this extent.
    pub base_global_index: u32,
    /// Reserved.
    pub _pad2: [u8; 4],

    /// Offset from extent start to SlotMeta array.
    pub meta_offset: u32,
    /// Offset from extent start to slot data.
    pub data_offset: u32,

    /// Padding to 64 bytes.
    pub _pad3: [u8; 32],
}

const _: () = assert!(core::mem::size_of::<ExtentHeader>() == 64);

impl ExtentHeader {
    /// Initialize an extent header.
    pub fn init(
        &mut self,
        size_class: u8,
        slot_count: u32,
        base_global_index: u32,
        meta_offset: u32,
        data_offset: u32,
    ) {
        self.size_class = size_class;
        self._pad1 = [0; 3];
        self.slot_count = slot_count;
        self.next_extent_offset = AtomicU64::new(0);
        self.base_global_index = base_global_index;
        self._pad2 = [0; 4];
        self.meta_offset = meta_offset;
        self.data_offset = data_offset;
        self._pad3 = [0; 32];
    }
}

// =============================================================================
// Hub Slot Metadata
// =============================================================================

/// Metadata for a single slot in the hub (16 bytes).
///
/// Extended from the original SlotMeta to include owner tracking and
/// the next_free link (not stored in slot data for MPMC safety).
#[repr(C)]
pub struct HubSlotMeta {
    /// Generation counter, incremented on each allocation.
    pub generation: AtomicU32,
    /// Current state (Free=0, Allocated=1, InFlight=2).
    pub state: AtomicU32,
    /// Global index of next free slot in this class (for Treiber stack).
    pub next_free: AtomicU32,
    /// Peer ID that owns this slot (NO_OWNER if none).
    pub owner_peer: AtomicU32,
}

const _: () = assert!(core::mem::size_of::<HubSlotMeta>() == 16);

impl HubSlotMeta {
    /// Initialize a slot as free.
    pub fn init_free(&mut self, next_free: u32) {
        self.generation = AtomicU32::new(0);
        self.state = AtomicU32::new(SlotState::Free as u32);
        self.next_free = AtomicU32::new(next_free);
        self.owner_peer = AtomicU32::new(NO_OWNER);
    }
}

// =============================================================================
// Slot State (reused from layout.rs)
// =============================================================================

/// Slot state in the data segment.
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotState {
    /// Slot is free and available for allocation.
    Free = 0,
    /// Slot is allocated, sender is writing payload.
    Allocated = 1,
    /// Descriptor enqueued, awaiting receiver.
    InFlight = 2,
}

impl SlotState {
    /// Convert from u32.
    pub fn from_u32(v: u32) -> Option<Self> {
        match v {
            0 => Some(SlotState::Free),
            1 => Some(SlotState::Allocated),
            2 => Some(SlotState::InFlight),
            _ => None,
        }
    }
}

// =============================================================================
// Slot Reference Encoding
// =============================================================================

/// Encode a slot reference into payload_slot field.
///
/// Format: `bits[31:29]` = class (0-7), `bits[28:0]` = global_index
#[inline]
pub fn encode_slot_ref(class: u8, global_index: u32) -> u32 {
    debug_assert!(class < 8, "class must fit in 3 bits");
    debug_assert!(global_index < (1 << 29), "global_index must fit in 29 bits");
    ((class as u32) << 29) | (global_index & 0x1FFF_FFFF)
}

/// Decode a slot reference from payload_slot field.
///
/// Returns (class, global_index).
#[inline]
pub fn decode_slot_ref(slot_ref: u32) -> (u8, u32) {
    let class = (slot_ref >> 29) as u8;
    let global_index = slot_ref & 0x1FFF_FFFF;
    (class, global_index)
}

/// Decode a global index into (extent_id, slot_in_extent).
#[inline]
pub fn decode_global_index(global_index: u32, extent_slot_shift: u8) -> (u32, u32) {
    let slots_per_extent_mask = (1u32 << extent_slot_shift) - 1;
    let extent_id = global_index >> extent_slot_shift;
    let slot_in_extent = global_index & slots_per_extent_mask;
    (extent_id, slot_in_extent)
}

/// Encode extent_id and slot_in_extent into a global index.
#[inline]
pub fn encode_global_index(extent_id: u32, slot_in_extent: u32, extent_slot_shift: u8) -> u32 {
    (extent_id << extent_slot_shift) | slot_in_extent
}

// =============================================================================
// Free List Helpers
// =============================================================================

/// Pack a free list head from global index and tag.
#[inline]
pub fn pack_free_head(global_index: u32, tag: u32) -> u64 {
    ((tag as u64) << 32) | (global_index as u64)
}

/// Unpack a free list head into (global_index, tag).
#[inline]
pub fn unpack_free_head(packed: u64) -> (u32, u32) {
    let global_index = packed as u32;
    let tag = (packed >> 32) as u32;
    (global_index, tag)
}

// =============================================================================
// Layout Calculations
// =============================================================================

/// Offsets within the hub SHM segment.
#[derive(Debug, Clone, Copy)]
pub struct HubOffsets {
    pub header: usize,
    pub peer_table: usize,
    pub ring_region: usize,
    pub size_class_headers: usize,
    pub extent_region: usize,
}

impl HubOffsets {
    /// Calculate offsets for the hub layout.
    pub fn calculate(max_peers: u16, ring_capacity: u32) -> Result<Self, &'static str> {
        let header_size = core::mem::size_of::<HubHeader>();
        let peer_entry_size = core::mem::size_of::<PeerEntry>();
        let ring_header_size = core::mem::size_of::<crate::layout::DescRingHeader>();
        let desc_size = core::mem::size_of::<MsgDescHot>();
        let size_class_header_size = core::mem::size_of::<SizeClassHeader>();

        let peer_table_size = peer_entry_size
            .checked_mul(max_peers as usize)
            .ok_or("peer table size overflow")?;

        // Each peer has 2 rings (send and recv)
        let ring_size = ring_header_size
            .checked_add(
                desc_size
                    .checked_mul(ring_capacity as usize)
                    .ok_or("ring desc size overflow")?,
            )
            .ok_or("ring size overflow")?;
        let ring_region_size = ring_size
            .checked_mul(2)
            .and_then(|v| v.checked_mul(max_peers as usize))
            .ok_or("ring region size overflow")?;

        let size_class_region_size = size_class_header_size
            .checked_mul(NUM_SIZE_CLASSES)
            .ok_or("size class region overflow")?;

        let header = 0usize;
        let peer_table = header
            .checked_add(header_size)
            .ok_or("peer table offset overflow")?;
        let ring_region = peer_table
            .checked_add(peer_table_size)
            .ok_or("ring region offset overflow")?;
        let size_class_headers = ring_region
            .checked_add(ring_region_size)
            .ok_or("size class offset overflow")?;
        let extent_region = size_class_headers
            .checked_add(size_class_region_size)
            .ok_or("extent region offset overflow")?;

        Ok(Self {
            header,
            peer_table,
            ring_region,
            size_class_headers,
            extent_region,
        })
    }
}

/// Calculate the initial hub size (before extent data).
pub fn calculate_hub_base_size(max_peers: u16, ring_capacity: u32) -> Result<usize, &'static str> {
    let offsets = HubOffsets::calculate(max_peers, ring_capacity)?;
    Ok(offsets.extent_region)
}

/// Calculate the size needed for a single extent.
pub fn calculate_extent_size(slot_size: u32, slot_count: u32) -> Result<usize, &'static str> {
    let extent_header_size = core::mem::size_of::<ExtentHeader>();
    let slot_meta_size = core::mem::size_of::<HubSlotMeta>();

    let meta_total = slot_meta_size
        .checked_mul(slot_count as usize)
        .ok_or("extent meta size overflow")?;
    let data_total = (slot_size as usize)
        .checked_mul(slot_count as usize)
        .ok_or("extent data size overflow")?;

    extent_header_size
        .checked_add(meta_total)
        .and_then(|v| v.checked_add(data_total))
        .ok_or("extent total size overflow")
}

/// Calculate the total initial hub size including all initial extents.
pub fn calculate_initial_hub_size(
    max_peers: u16,
    ring_capacity: u32,
) -> Result<usize, &'static str> {
    let base_size = calculate_hub_base_size(max_peers, ring_capacity)?;

    let mut total = base_size;
    for (slot_size, slot_count) in HUB_SIZE_CLASSES {
        let extent_size = calculate_extent_size(slot_size, slot_count)?;
        total = total
            .checked_add(extent_size)
            .ok_or("total hub size overflow")?;
    }

    Ok(total)
}

// =============================================================================
// Errors
// =============================================================================

/// Errors from hub layout validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HubLayoutError {
    /// Invalid magic bytes.
    InvalidMagic,
    /// Incompatible protocol version.
    IncompatibleVersion { expected: u32, found: u32 },
    /// Hub too small.
    HubTooSmall { required: usize, found: usize },
    /// Invalid configuration in header.
    InvalidConfig(&'static str),
}

impl std::fmt::Display for HubLayoutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidMagic => write!(f, "invalid hub magic bytes (expected RAPAHUB)"),
            Self::IncompatibleVersion { expected, found } => {
                write!(
                    f,
                    "incompatible hub version: expected {}.{}, found {}.{}",
                    expected >> 16,
                    expected & 0xFFFF,
                    found >> 16,
                    found & 0xFFFF
                )
            }
            Self::HubTooSmall { required, found } => {
                write!(f, "hub too small: need {} bytes, got {}", required, found)
            }
            Self::InvalidConfig(msg) => write!(f, "invalid hub config: {}", msg),
        }
    }
}

impl std::error::Error for HubLayoutError {}

/// Errors from hub slot operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HubSlotError {
    /// No free slots available in any size class.
    NoFreeSlots,
    /// Payload too large for largest size class.
    PayloadTooLarge { len: usize, max: usize },
    /// Invalid slot reference.
    InvalidSlotRef,
    /// Generation mismatch (stale reference).
    StaleGeneration,
    /// Slot in unexpected state.
    InvalidState,
    /// Invalid size class.
    InvalidSizeClass,
    /// Invalid extent.
    InvalidExtent,
}

impl std::fmt::Display for HubSlotError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoFreeSlots => write!(f, "no free slots available"),
            Self::PayloadTooLarge { len, max } => {
                write!(f, "payload too large: {} bytes, max {}", len, max)
            }
            Self::InvalidSlotRef => write!(f, "invalid slot reference"),
            Self::StaleGeneration => write!(f, "stale generation"),
            Self::InvalidState => write!(f, "invalid slot state"),
            Self::InvalidSizeClass => write!(f, "invalid size class"),
            Self::InvalidExtent => write!(f, "invalid extent"),
        }
    }
}

impl std::error::Error for HubSlotError {}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hub_header_size() {
        assert_eq!(core::mem::size_of::<HubHeader>(), 256);
    }

    #[test]
    fn test_peer_entry_size() {
        assert_eq!(core::mem::size_of::<PeerEntry>(), 64);
    }

    #[test]
    fn test_extent_header_size() {
        assert_eq!(core::mem::size_of::<ExtentHeader>(), 64);
    }

    #[test]
    fn test_hub_slot_meta_size() {
        assert_eq!(core::mem::size_of::<HubSlotMeta>(), 16);
    }

    #[test]
    fn test_slot_ref_encoding() {
        // Test class 0, index 0
        let encoded = encode_slot_ref(0, 0);
        let (class, index) = decode_slot_ref(encoded);
        assert_eq!(class, 0);
        assert_eq!(index, 0);

        // Test class 4, index 12345
        let encoded = encode_slot_ref(4, 12345);
        let (class, index) = decode_slot_ref(encoded);
        assert_eq!(class, 4);
        assert_eq!(index, 12345);

        // Test max class (7), max index
        let max_index = (1 << 29) - 1;
        let encoded = encode_slot_ref(7, max_index);
        let (class, index) = decode_slot_ref(encoded);
        assert_eq!(class, 7);
        assert_eq!(index, max_index);
    }

    #[test]
    fn test_global_index_encoding() {
        let extent_slot_shift = 10; // 1024 slots per extent

        // Test extent 0, slot 0
        let global = encode_global_index(0, 0, extent_slot_shift);
        let (extent, slot) = decode_global_index(global, extent_slot_shift);
        assert_eq!(extent, 0);
        assert_eq!(slot, 0);

        // Test extent 2, slot 500
        let global = encode_global_index(2, 500, extent_slot_shift);
        let (extent, slot) = decode_global_index(global, extent_slot_shift);
        assert_eq!(extent, 2);
        assert_eq!(slot, 500);
    }

    #[test]
    fn test_calculate_initial_hub_size() {
        let size = calculate_initial_hub_size(MAX_PEERS, DEFAULT_HUB_RING_CAPACITY).unwrap();
        // Should be around 109MB + overhead
        assert!(size > 100_000_000, "expected > 100MB, got {}", size);
        assert!(size < 150_000_000, "expected < 150MB, got {}", size);
        println!(
            "Initial hub size: {} bytes ({:.1} MB)",
            size,
            size as f64 / 1_000_000.0
        );
    }

    #[test]
    fn test_hub_offsets() {
        let offsets = HubOffsets::calculate(MAX_PEERS, DEFAULT_HUB_RING_CAPACITY).unwrap();
        assert_eq!(offsets.header, 0);
        assert_eq!(offsets.peer_table, 256); // After HubHeader
        // peer_table = 32 peers * 64 bytes = 2048
        assert_eq!(offsets.ring_region, 256 + 2048);
        println!("Hub offsets: {:?}", offsets);
    }
}
