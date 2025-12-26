//! Hub SHM memory layout definitions.
//!
//! Ported from `rapace-transport-shm` (hub architecture).

use std::sync::atomic::{AtomicU16, AtomicU32, AtomicU64, Ordering};

use crate::MsgDescHot;

use super::layout::DescRingHeader;

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

    /// Validate the hub header.
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
            return Err(HubLayoutError::InvalidConfig(
                "max_peers must be between 1 and MAX_PEERS",
            ));
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
// Peer Table
// =============================================================================

/// Peer entry in the peer table (64 bytes).
#[repr(C, align(64))]
pub struct PeerEntry {
    /// Peer ID.
    pub peer_id: u16,
    /// Peer type (reserved for future).
    pub peer_type: u16,
    /// Flags (active, dying, dead).
    pub flags: AtomicU32,

    /// Heartbeat counter.
    pub epoch: AtomicU64,
    /// Last seen timestamp (nanos since epoch).
    pub last_seen: AtomicU64,

    /// Offset to send ring for this peer (peer->host).
    pub send_ring_offset: u64,
    /// Offset to recv ring for this peer (host->peer).
    pub recv_ring_offset: u64,

    /// Futex for send ring data (peer signals host).
    pub send_data_futex: AtomicU32,
    /// Futex for recv ring data (host signals peer).
    pub recv_data_futex: AtomicU32,

    /// Padding to 64 bytes.
    pub _pad: [u8; 16],
}

const _: () = assert!(core::mem::size_of::<PeerEntry>() == 64);

impl PeerEntry {
    pub fn init(&mut self, send_ring_offset: u64, recv_ring_offset: u64) {
        self.peer_id = 0;
        self.peer_type = 0;
        self.flags = AtomicU32::new(0);
        self.epoch = AtomicU64::new(0);
        self.last_seen = AtomicU64::new(0);
        self.send_ring_offset = send_ring_offset;
        self.recv_ring_offset = recv_ring_offset;
        self.send_data_futex = AtomicU32::new(0);
        self.recv_data_futex = AtomicU32::new(0);
        self._pad = [0; 16];
    }

    pub fn mark_active(&self) {
        self.flags.fetch_or(PEER_FLAG_ACTIVE, Ordering::Release);
        self.flags
            .fetch_and(!(PEER_FLAG_RESERVED | PEER_FLAG_DEAD), Ordering::Release);
    }

    pub fn mark_dead(&self) {
        self.flags.fetch_or(PEER_FLAG_DEAD, Ordering::Release);
        self.flags.fetch_and(!PEER_FLAG_ACTIVE, Ordering::Release);
    }
}

// =============================================================================
// Size Class Headers
// =============================================================================

/// Slot state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum SlotState {
    Free = 0,
    Allocated = 1,
    InFlight = 2,
}

impl SlotState {
    pub fn from_u32(v: u32) -> Option<Self> {
        match v {
            0 => Some(SlotState::Free),
            1 => Some(SlotState::Allocated),
            2 => Some(SlotState::InFlight),
            _ => None,
        }
    }
}

/// Hub slot metadata (16 bytes).
#[repr(C)]
pub struct HubSlotMeta {
    /// Generation counter (increments on free).
    pub generation: AtomicU32,
    /// Slot state.
    pub state: AtomicU32,
    /// Next free in Treiber stack (global index).
    pub next_free: AtomicU32,
    /// Owning peer ID (for reclamation), or NO_OWNER.
    pub owner_peer: AtomicU32,
}

const _: () = assert!(core::mem::size_of::<HubSlotMeta>() == 16);

/// Size class header (128 bytes).
#[repr(C, align(64))]
pub struct SizeClassHeader {
    /// Slot size in bytes.
    pub slot_size: u32,
    /// Slots per extent (power of 2).
    pub slots_per_extent: u32,
    /// Shift for extent id in global index.
    pub extent_slot_shift: u32,
    /// Total slots across extents (for diagnostics).
    pub total_slots: AtomicU32,

    /// Tagged Treiber stack head: (tag<<32)|global_index.
    pub free_head: AtomicU64,

    /// Extent offsets (relative to base) for this class.
    pub extent_offsets: [AtomicU64; MAX_EXTENTS_PER_CLASS],

    /// Futex for waiting allocators (cross-process).
    ///
    /// This is incremented and woken on slot free.
    /// Note: stored in what used to be padding so older versions that don't use it
    /// can still map the same layout without breaking offsets.
    pub slot_available_futex: AtomicU32,
    /// Padding.
    pub _pad: [u8; 4],
}

impl SizeClassHeader {
    pub fn init(&mut self, slot_size: u32, slots_per_extent: u32) {
        self.slot_size = slot_size;
        self.slots_per_extent = slots_per_extent;
        self.extent_slot_shift = slots_per_extent.trailing_zeros();
        self.total_slots = AtomicU32::new(0);
        self.free_head = AtomicU64::new(pack_free_head(FREE_LIST_END, 0));
        for off in &mut self.extent_offsets {
            *off = AtomicU64::new(0);
        }
        self.slot_available_futex = AtomicU32::new(0);
        self._pad = [0; 4];
    }
}

// =============================================================================
// Extents
// =============================================================================

/// Extent header (64 bytes).
#[repr(C, align(64))]
pub struct ExtentHeader {
    /// Size class index.
    pub class: u16,
    /// Extent index within class.
    pub extent_index: u16,
    /// Base global index for this extent.
    pub base_global_index: u32,
    /// Slot count in this extent.
    pub slot_count: u32,
    /// Slot size in bytes.
    pub slot_size: u32,
    /// Offset to slot meta array from start of extent.
    pub meta_offset: u32,
    /// Offset to slot data from start of extent.
    pub data_offset: u32,
    /// Padding.
    pub _pad: [u8; 32],
}

const _: () = assert!(core::mem::size_of::<ExtentHeader>() == 64);

// =============================================================================
// Encoding helpers
// =============================================================================

/// Encode a slot reference into u32.
/// Bits:
/// - 31..29: class (0-7)
/// - 28..0: global index (0..2^29-1)
pub fn encode_slot_ref(class: u16, global_index: u32) -> u32 {
    ((class as u32) << 29) | (global_index & ((1 << 29) - 1))
}

pub fn decode_slot_ref(slot_ref: u32) -> (u16, u32) {
    let class = (slot_ref >> 29) as u16;
    let global_index = slot_ref & ((1 << 29) - 1);
    (class, global_index)
}

pub fn encode_global_index(extent_id: u32, slot_in_extent: u32, extent_slot_shift: u32) -> u32 {
    (extent_id << extent_slot_shift) | slot_in_extent
}

pub fn decode_global_index(global_index: u32, extent_slot_shift: u32) -> (u32, u32) {
    let extent_id = global_index >> extent_slot_shift;
    let slot_in_extent = global_index & ((1 << extent_slot_shift) - 1);
    (extent_id, slot_in_extent)
}

pub fn pack_free_head(global_index: u32, tag: u32) -> u64 {
    ((tag as u64) << 32) | global_index as u64
}

pub fn unpack_free_head(head: u64) -> (u32, u32) {
    (head as u32, (head >> 32) as u32)
}

// =============================================================================
// Offsets
// =============================================================================

#[derive(Debug, Clone)]
pub struct HubOffsets {
    pub header: usize,
    pub peer_table: usize,
    pub ring_region: usize,
    pub size_class_headers: usize,
    pub extent_region: usize,
}

impl HubOffsets {
    pub fn calculate(max_peers: u16, ring_capacity: u32) -> Result<Self, HubLayoutError> {
        if max_peers == 0 || max_peers > MAX_PEERS {
            return Err(HubLayoutError::InvalidConfig(
                "max_peers must be between 1 and MAX_PEERS",
            ));
        }

        if !ring_capacity.is_power_of_two() || ring_capacity == 0 {
            return Err(HubLayoutError::InvalidConfig(
                "ring_capacity must be non-zero power of 2",
            ));
        }

        let header = 0;
        let peer_table = align_up(header + core::mem::size_of::<HubHeader>(), 64);

        let peer_table_size = max_peers as usize * core::mem::size_of::<PeerEntry>();

        let ring_header_size = core::mem::size_of::<DescRingHeader>();
        let ring_descs_size = ring_capacity as usize * core::mem::size_of::<MsgDescHot>();
        let ring_size = ring_header_size + ring_descs_size;
        let ring_region = align_up(peer_table + peer_table_size, 64);
        let ring_region_size = max_peers as usize * 2 * ring_size;

        let size_class_headers = align_up(ring_region + ring_region_size, 64);
        let size_class_headers_size = NUM_SIZE_CLASSES * core::mem::size_of::<SizeClassHeader>();

        let extent_region = align_up(size_class_headers + size_class_headers_size, 64);

        Ok(Self {
            header,
            peer_table,
            ring_region,
            size_class_headers,
            extent_region,
        })
    }
}

// =============================================================================
// Size calculations
// =============================================================================

pub fn calculate_extent_size(slot_size: u32, slot_count: u32) -> Result<usize, &'static str> {
    let header_size = core::mem::size_of::<ExtentHeader>();
    let meta_size = (slot_count as usize)
        .checked_mul(core::mem::size_of::<HubSlotMeta>())
        .ok_or("extent meta size overflow")?;
    let data_size = (slot_count as usize)
        .checked_mul(slot_size as usize)
        .ok_or("extent data size overflow")?;

    let total = header_size
        .checked_add(meta_size)
        .and_then(|v| v.checked_add(data_size))
        .ok_or("extent total size overflow")?;

    Ok(align_up(total, 64))
}

pub fn calculate_initial_hub_size(
    max_peers: u16,
    ring_capacity: u32,
) -> Result<usize, &'static str> {
    let offsets = HubOffsets::calculate(max_peers, ring_capacity)
        .map_err(|_| "hub offsets calculation failed")?;

    let mut total = offsets.extent_region;
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
// Helpers
// =============================================================================

fn align_up(x: usize, align: usize) -> usize {
    debug_assert!(align.is_power_of_two());
    (x + align - 1) & !(align - 1)
}
