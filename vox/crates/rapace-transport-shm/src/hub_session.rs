//! Hub session management.
//!
//! This module provides the host and peer abstractions for the hub architecture.
//!
//! - `HubHost`: Created by the host process to manage the shared SHM and all peers
//! - `HubPeer`: Used by plugin processes to connect to an existing hub

use std::fs::{File, OpenOptions};
use std::io;
use std::os::unix::io::AsRawFd;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use crate::doorbell::Doorbell;
use crate::hub_alloc::{HubAllocator, init_extent_free_list};
use crate::hub_layout::{
    DEFAULT_HUB_RING_CAPACITY, ExtentHeader, HUB_SIZE_CLASSES, HubHeader, HubOffsets, HubSlotMeta,
    MAX_PEERS, NUM_SIZE_CLASSES, PEER_FLAG_ACTIVE, PEER_FLAG_RESERVED, PeerEntry, SizeClassHeader,
    calculate_extent_size, calculate_initial_hub_size,
};
use crate::layout::{DescRing, DescRingHeader};

use rapace_core::MsgDescHot;

/// Configuration for creating a hub.
#[derive(Debug, Clone)]
pub struct HubConfig {
    /// Maximum number of peers (including host).
    pub max_peers: u16,
    /// Ring capacity per peer.
    pub ring_capacity: u32,
}

impl Default for HubConfig {
    fn default() -> Self {
        Self {
            max_peers: MAX_PEERS,
            ring_capacity: DEFAULT_HUB_RING_CAPACITY,
        }
    }
}

/// Shared SHM mapping.
struct HubMapping {
    /// Base address of the mapping.
    base_addr: *mut u8,
    /// Current size of the mapping.
    size: usize,
    /// The underlying file (kept open).
    _file: File,
}

// SAFETY: HubMapping is Send + Sync because the memory is synchronized via atomics.
unsafe impl Send for HubMapping {}
unsafe impl Sync for HubMapping {}

impl Drop for HubMapping {
    fn drop(&mut self) {
        // SAFETY: base_addr and size were valid when created.
        unsafe {
            libc::munmap(self.base_addr as *mut libc::c_void, self.size);
        }
    }
}

/// Host-side hub session.
///
/// Creates and manages the shared SHM file. Allocates peers, manages rings,
/// and provides the allocator.
pub struct HubHost {
    /// The memory mapping.
    mapping: Arc<HubMapping>,
    /// Computed offsets.
    offsets: HubOffsets,
    /// Configuration.
    config: HubConfig,
    /// The allocator view.
    allocator: HubAllocator,
    /// Path to the SHM file.
    path: std::path::PathBuf,
}

// SAFETY: HubHost is Send + Sync because it uses atomic operations for all shared state.
unsafe impl Send for HubHost {}
unsafe impl Sync for HubHost {}

/// Information about an added peer.
pub struct PeerInfo {
    /// The assigned peer ID.
    pub peer_id: u16,
    /// The doorbell for this peer (host keeps this end).
    pub doorbell: Doorbell,
    /// Raw FD for the peer's doorbell end (pass to plugin via CLI).
    pub peer_doorbell_fd: i32,
}

impl HubHost {
    /// Create a new hub at the given path.
    pub fn create(path: impl AsRef<Path>, config: HubConfig) -> Result<Self, HubSessionError> {
        let path = path.as_ref();

        // Calculate sizes
        let offsets = HubOffsets::calculate(config.max_peers, config.ring_capacity)
            .map_err(|e| HubSessionError::Layout(e.to_string()))?;
        let total_size = calculate_initial_hub_size(config.max_peers, config.ring_capacity)
            .map_err(|e| HubSessionError::Layout(e.to_string()))?;

        // Create and truncate file
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)
            .map_err(HubSessionError::Io)?;

        // Set size
        file.set_len(total_size as u64)
            .map_err(HubSessionError::Io)?;

        // Memory map
        let base_addr = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                total_size,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                file.as_raw_fd(),
                0,
            )
        };

        if base_addr == libc::MAP_FAILED {
            return Err(HubSessionError::Io(io::Error::last_os_error()));
        }

        let base_addr = base_addr as *mut u8;

        // Initialize header
        let header = unsafe { &mut *(base_addr as *mut HubHeader) };
        header.init(config.max_peers, config.ring_capacity);
        header
            .current_size
            .store(total_size as u64, Ordering::Release);
        header.peer_table_offset = offsets.peer_table as u64;
        header.ring_region_offset = offsets.ring_region as u64;
        header.size_class_offset = offsets.size_class_headers as u64;
        header.extent_region_offset = offsets.extent_region as u64;

        // Initialize peer table
        for i in 0..config.max_peers {
            let peer_entry = unsafe {
                &mut *(base_addr
                    .add(offsets.peer_table + i as usize * std::mem::size_of::<PeerEntry>())
                    as *mut PeerEntry)
            };
            peer_entry.init(0, 0);
        }

        // Initialize rings for each peer slot
        let ring_size = std::mem::size_of::<DescRingHeader>()
            + config.ring_capacity as usize * std::mem::size_of::<MsgDescHot>();

        for i in 0..config.max_peers {
            let send_ring_offset = offsets.ring_region + i as usize * 2 * ring_size;
            let recv_ring_offset = send_ring_offset + ring_size;

            // Initialize send ring
            let send_ring_header =
                unsafe { &mut *(base_addr.add(send_ring_offset) as *mut DescRingHeader) };
            send_ring_header.init(config.ring_capacity);

            // Initialize recv ring
            let recv_ring_header =
                unsafe { &mut *(base_addr.add(recv_ring_offset) as *mut DescRingHeader) };
            recv_ring_header.init(config.ring_capacity);

            // Update peer entry with ring offsets
            let peer_entry = unsafe {
                &mut *(base_addr
                    .add(offsets.peer_table + i as usize * std::mem::size_of::<PeerEntry>())
                    as *mut PeerEntry)
            };
            peer_entry.send_ring_offset = send_ring_offset as u64;
            peer_entry.recv_ring_offset = recv_ring_offset as u64;
        }

        // Initialize size class headers
        let mut size_class_ptrs = [std::ptr::null_mut(); NUM_SIZE_CLASSES];
        for (i, (slot_size, _slot_count)) in HUB_SIZE_CLASSES.iter().enumerate() {
            let class_header = unsafe {
                &mut *(base_addr
                    .add(offsets.size_class_headers + i * std::mem::size_of::<SizeClassHeader>())
                    as *mut SizeClassHeader)
            };

            // Calculate extent_slot_shift (log2 of slots per extent)
            // For initial extents, we use the configured slot count
            let slots_per_extent = HUB_SIZE_CLASSES[i].1;
            let extent_slot_shift = (32 - slots_per_extent.leading_zeros() - 1) as u8;

            class_header.init(*slot_size, extent_slot_shift);
            size_class_ptrs[i] = class_header as *mut SizeClassHeader;
        }

        // Initialize extents
        let mut current_offset = offsets.extent_region;
        for (class, (slot_size, slot_count)) in HUB_SIZE_CLASSES.iter().enumerate() {
            let extent_size = calculate_extent_size(*slot_size, *slot_count)
                .map_err(|e| HubSessionError::Layout(e.to_string()))?;

            // Initialize extent header
            let extent_header =
                unsafe { &mut *(base_addr.add(current_offset) as *mut ExtentHeader) };

            let meta_offset = std::mem::size_of::<ExtentHeader>() as u32;
            let data_offset = meta_offset + *slot_count * std::mem::size_of::<HubSlotMeta>() as u32;

            extent_header.init(class as u8, *slot_count, 0, meta_offset, data_offset);

            // Update size class header with extent offset
            let class_header = unsafe { &mut *size_class_ptrs[class] };
            class_header.extent_offsets[0].store(current_offset as u64, Ordering::Release);
            class_header.extent_count = 1;

            current_offset += extent_size;
        }

        // Update extent count in header
        header
            .extent_count
            .store(NUM_SIZE_CLASSES as u32, Ordering::Release);

        let mapping = Arc::new(HubMapping {
            base_addr,
            size: total_size,
            _file: file,
        });

        // Create allocator
        let allocator = unsafe { HubAllocator::from_raw(size_class_ptrs, base_addr) };

        // Initialize free lists for all extents
        let mut extent_offset = offsets.extent_region;
        for (class, (slot_size, slot_count)) in HUB_SIZE_CLASSES.iter().enumerate() {
            let extent_size = calculate_extent_size(*slot_size, *slot_count)
                .map_err(|e| HubSessionError::Layout(e.to_string()))?;

            unsafe {
                init_extent_free_list(&allocator, class, 0, extent_offset as u64);
            }

            extent_offset += extent_size;
        }

        Ok(Self {
            mapping,
            offsets,
            config,
            allocator,
            path: path.to_path_buf(),
        })
    }

    /// Add a new peer to the hub.
    ///
    /// Returns peer info including the peer ID and doorbell.
    pub fn add_peer(&self) -> Result<PeerInfo, HubSessionError> {
        let header = self.header();

        // Allocate peer ID
        let peer_id = header.peer_id_counter.fetch_add(1, Ordering::AcqRel);
        if peer_id >= self.config.max_peers {
            return Err(HubSessionError::TooManyPeers);
        }

        // Get peer entry (mutable during initialization)
        // SAFETY: Host controls peer allocation; initialization is serialized by the host.
        let peer_entry = unsafe { &mut *self.peer_entry_ptr(peer_id) };

        // Initialize peer entry
        peer_entry.peer_id = peer_id;
        peer_entry.peer_type = 1; // Plugin
        peer_entry
            .flags
            .store(PEER_FLAG_RESERVED, Ordering::Release);
        peer_entry.epoch.store(0, Ordering::Release);
        peer_entry.last_seen.store(0, Ordering::Release);

        // Create doorbell
        let (doorbell, peer_fd) = Doorbell::create_pair().map_err(HubSessionError::Io)?;

        // Increment active peer count
        header.active_peers.fetch_add(1, Ordering::AcqRel);

        Ok(PeerInfo {
            peer_id,
            doorbell,
            peer_doorbell_fd: peer_fd,
        })
    }

    /// Mark a peer as active (called after plugin confirms connection).
    pub fn activate_peer(&self, peer_id: u16) -> Result<(), HubSessionError> {
        if peer_id >= self.config.max_peers {
            return Err(HubSessionError::InvalidPeerId);
        }

        let peer_entry = self.peer_entry(peer_id);
        peer_entry.mark_active();
        Ok(())
    }

    /// Remove a peer and reclaim its resources.
    pub fn remove_peer(&self, peer_id: u16) -> Result<(), HubSessionError> {
        if peer_id >= self.config.max_peers {
            return Err(HubSessionError::InvalidPeerId);
        }

        let peer_entry = self.peer_entry(peer_id);
        peer_entry.mark_dead();

        // Reclaim all slots owned by this peer
        self.allocator.reclaim_peer_slots(peer_id as u32);

        // Decrement active peer count
        self.header().active_peers.fetch_sub(1, Ordering::AcqRel);

        Ok(())
    }

    /// Get the hub header.
    fn header(&self) -> &HubHeader {
        unsafe { &*(self.mapping.base_addr as *const HubHeader) }
    }

    /// Get a peer entry pointer.
    ///
    /// # Safety
    ///
    /// Callers must ensure they uphold Rust aliasing rules when writing through this pointer.
    /// Host-side peer entries are mutated during peer setup/teardown and otherwise read via atomics.
    fn peer_entry_ptr(&self, peer_id: u16) -> *mut PeerEntry {
        unsafe {
            self.mapping
                .base_addr
                .add(self.offsets.peer_table + peer_id as usize * std::mem::size_of::<PeerEntry>())
                as *mut PeerEntry
        }
    }

    /// Get a peer entry (read-only view).
    fn peer_entry(&self, peer_id: u16) -> &PeerEntry {
        // SAFETY: points into the mapped peer table.
        unsafe { &*self.peer_entry_ptr(peer_id) }
    }

    /// Get the send ring for a peer (peer -> host).
    pub fn peer_send_ring(&self, peer_id: u16) -> DescRing {
        let peer_entry = self.peer_entry(peer_id);
        let ring_offset = peer_entry.send_ring_offset as usize;

        let header_ptr = unsafe { self.mapping.base_addr.add(ring_offset) as *mut DescRingHeader };
        let descs_ptr = unsafe {
            self.mapping
                .base_addr
                .add(ring_offset + std::mem::size_of::<DescRingHeader>())
                as *mut MsgDescHot
        };

        unsafe { DescRing::from_raw(header_ptr, descs_ptr) }
    }

    /// Get the recv ring for a peer (host -> peer).
    pub fn peer_recv_ring(&self, peer_id: u16) -> DescRing {
        let peer_entry = self.peer_entry(peer_id);
        let ring_offset = peer_entry.recv_ring_offset as usize;

        let header_ptr = unsafe { self.mapping.base_addr.add(ring_offset) as *mut DescRingHeader };
        let descs_ptr = unsafe {
            self.mapping
                .base_addr
                .add(ring_offset + std::mem::size_of::<DescRingHeader>())
                as *mut MsgDescHot
        };

        unsafe { DescRing::from_raw(header_ptr, descs_ptr) }
    }

    /// Get the allocator.
    pub fn allocator(&self) -> &HubAllocator {
        &self.allocator
    }

    /// Get the path to the SHM file.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Get the send_data_futex for a peer (for signaling).
    pub fn peer_send_data_futex(&self, peer_id: u16) -> &std::sync::atomic::AtomicU32 {
        let peer_entry = self.peer_entry(peer_id);
        &peer_entry.send_data_futex
    }

    /// Get the recv_data_futex for a peer (for signaling).
    pub fn peer_recv_data_futex(&self, peer_id: u16) -> &std::sync::atomic::AtomicU32 {
        let peer_entry = self.peer_entry(peer_id);
        &peer_entry.recv_data_futex
    }

    /// Get active peer count.
    pub fn active_peer_count(&self) -> u16 {
        self.header().active_peers.load(Ordering::Acquire)
    }

    /// Check if a peer is active.
    pub fn is_peer_active(&self, peer_id: u16) -> bool {
        if peer_id >= self.config.max_peers {
            return false;
        }
        self.peer_entry(peer_id).is_active()
    }
}

/// Peer-side hub session.
///
/// Opens an existing hub SHM and provides access to this peer's rings and the allocator.
pub struct HubPeer {
    /// The memory mapping.
    mapping: Arc<HubMapping>,
    /// This peer's ID.
    peer_id: u16,
    /// Computed offsets.
    offsets: HubOffsets,
    /// The allocator view.
    allocator: HubAllocator,
}

// SAFETY: HubPeer is Send + Sync because it uses atomic operations for all shared state.
unsafe impl Send for HubPeer {}
unsafe impl Sync for HubPeer {}

impl HubPeer {
    /// Open an existing hub SHM.
    pub fn open(path: impl AsRef<Path>, peer_id: u16) -> Result<Self, HubSessionError> {
        let path = path.as_ref();

        // Open file
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .map_err(HubSessionError::Io)?;

        let file_size = file.metadata().map_err(HubSessionError::Io)?.len() as usize;

        // Memory map
        let base_addr = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                file_size,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                file.as_raw_fd(),
                0,
            )
        };

        if base_addr == libc::MAP_FAILED {
            return Err(HubSessionError::Io(io::Error::last_os_error()));
        }

        let base_addr = base_addr as *mut u8;

        // Validate header
        let header = unsafe { &*(base_addr as *const HubHeader) };
        header
            .validate()
            .map_err(|e| HubSessionError::Layout(e.to_string()))?;

        let max_peers = header.max_peers;
        let ring_capacity = header.ring_capacity;

        if peer_id >= max_peers {
            return Err(HubSessionError::InvalidPeerId);
        }

        // Calculate offsets
        let offsets = HubOffsets::calculate(max_peers, ring_capacity)
            .map_err(|e| HubSessionError::Layout(e.to_string()))?;

        // Build size class pointers
        let mut size_class_ptrs = [std::ptr::null_mut(); NUM_SIZE_CLASSES];
        for (i, ptr) in size_class_ptrs.iter_mut().enumerate() {
            *ptr = unsafe {
                base_addr
                    .add(offsets.size_class_headers + i * std::mem::size_of::<SizeClassHeader>())
                    as *mut SizeClassHeader
            };
        }

        let mapping = Arc::new(HubMapping {
            base_addr,
            size: file_size,
            _file: file,
        });

        let allocator = unsafe { HubAllocator::from_raw(size_class_ptrs, base_addr) };

        Ok(Self {
            mapping,
            peer_id,
            offsets,
            allocator,
        })
    }

    /// Register this peer as active.
    pub fn register(&self) {
        let peer_entry = self.peer_entry();
        peer_entry
            .flags
            .fetch_or(PEER_FLAG_ACTIVE, Ordering::Release);
        peer_entry.flags.fetch_and(
            !(crate::hub_layout::PEER_FLAG_RESERVED | crate::hub_layout::PEER_FLAG_DEAD),
            Ordering::Release,
        );
    }

    /// Get this peer's ID.
    pub fn peer_id(&self) -> u16 {
        self.peer_id
    }

    /// Get this peer's entry.
    fn peer_entry(&self) -> &PeerEntry {
        unsafe {
            &*(self.mapping.base_addr.add(
                self.offsets.peer_table + self.peer_id as usize * std::mem::size_of::<PeerEntry>(),
            ) as *const PeerEntry)
        }
    }

    /// Get this peer's send ring (peer -> host).
    pub fn send_ring(&self) -> DescRing {
        let peer_entry = self.peer_entry();
        let ring_offset = peer_entry.send_ring_offset as usize;

        let header_ptr = unsafe { self.mapping.base_addr.add(ring_offset) as *mut DescRingHeader };
        let descs_ptr = unsafe {
            self.mapping
                .base_addr
                .add(ring_offset + std::mem::size_of::<DescRingHeader>())
                as *mut MsgDescHot
        };

        unsafe { DescRing::from_raw(header_ptr, descs_ptr) }
    }

    /// Get this peer's recv ring (host -> peer).
    pub fn recv_ring(&self) -> DescRing {
        let peer_entry = self.peer_entry();
        let ring_offset = peer_entry.recv_ring_offset as usize;

        let header_ptr = unsafe { self.mapping.base_addr.add(ring_offset) as *mut DescRingHeader };
        let descs_ptr = unsafe {
            self.mapping
                .base_addr
                .add(ring_offset + std::mem::size_of::<DescRingHeader>())
                as *mut MsgDescHot
        };

        unsafe { DescRing::from_raw(header_ptr, descs_ptr) }
    }

    /// Get the allocator.
    pub fn allocator(&self) -> &HubAllocator {
        &self.allocator
    }

    /// Get the send_data_futex (for signaling host).
    pub fn send_data_futex(&self) -> &std::sync::atomic::AtomicU32 {
        &self.peer_entry().send_data_futex
    }

    /// Get the recv_data_futex (for waiting on host).
    pub fn recv_data_futex(&self) -> &std::sync::atomic::AtomicU32 {
        &self.peer_entry().recv_data_futex
    }

    /// Update heartbeat.
    pub fn update_heartbeat(&self) {
        let peer_entry = self.peer_entry();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;

        peer_entry.last_seen.store(now, Ordering::Release);
        peer_entry.epoch.fetch_add(1, Ordering::Relaxed);
    }
}

/// Errors from hub session operations.
#[derive(Debug)]
pub enum HubSessionError {
    /// I/O error.
    Io(io::Error),
    /// Layout error.
    Layout(String),
    /// Too many peers.
    TooManyPeers,
    /// Invalid peer ID.
    InvalidPeerId,
}

impl std::fmt::Display for HubSessionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "I/O error: {}", e),
            Self::Layout(e) => write!(f, "layout error: {}", e),
            Self::TooManyPeers => write!(f, "too many peers"),
            Self::InvalidPeerId => write!(f, "invalid peer ID"),
        }
    }
}

impl std::error::Error for HubSessionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;

    #[tokio::test]
    async fn test_hub_create_and_open() {
        let temp_dir = std::env::temp_dir();
        let path = temp_dir.join(format!("test_hub_{}.shm", std::process::id()));

        // Create hub
        let host = HubHost::create(&path, HubConfig::default()).unwrap();

        // Check header
        let header = host.header();
        assert_eq!(&header.magic, b"RAPAHUB\0");
        assert!(header.current_size.load(Ordering::Acquire) > 0);

        // Add a peer
        let peer_info = host.add_peer().unwrap();
        assert_eq!(peer_info.peer_id, 0);

        // Open from peer side
        let peer = HubPeer::open(&path, peer_info.peer_id).unwrap();
        assert_eq!(peer.peer_id(), 0);

        // Register peer
        peer.register();
        assert!(host.is_peer_active(0));

        // Close doorbell FD
        crate::doorbell::close_peer_fd(peer_info.peer_doorbell_fd);

        // Cleanup
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn test_hub_allocator() {
        let temp_dir = std::env::temp_dir();
        let path = temp_dir.join(format!("test_hub_alloc_{}.shm", std::process::id()));

        let host = HubHost::create(&path, HubConfig::default()).unwrap();

        // Allocate a small slot
        let (class, index, generation) = host.allocator().alloc(100, 0).unwrap();
        assert_eq!(class, 0); // Should be 1KB class

        // Free it
        host.allocator()
            .mark_in_flight(class, index, generation)
            .unwrap();
        host.allocator().free(class, index, generation).unwrap();

        // Cleanup
        std::fs::remove_file(&path).ok();
    }
}
