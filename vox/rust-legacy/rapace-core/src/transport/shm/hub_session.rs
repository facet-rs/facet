//! Hub session management (host + many peers).
//!
//! Ported from `rapace-transport-shm` (hub architecture).

use std::fs::{File, OpenOptions};
use std::io;
use std::os::unix::io::AsRawFd;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use super::hub_alloc::{HubAllocator, init_extent_free_list};
use super::hub_layout::{
    DEFAULT_HUB_RING_CAPACITY, ExtentHeader, HUB_SIZE_CLASSES, HubHeader, HubOffsets, HubSlotMeta,
    MAX_PEERS, NUM_SIZE_CLASSES, PEER_FLAG_ACTIVE, PEER_FLAG_RESERVED, PeerEntry, SizeClassHeader,
    calculate_extent_size, calculate_initial_hub_size,
};
use super::layout::{DescRing, DescRingHeader};
use crate::MsgDescHot;
use shm_primitives::Doorbell;

/// Configuration for creating a hub.
#[derive(Debug, Clone)]
pub struct HubConfig {
    pub max_peers: u16,
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

struct HubMapping {
    base_addr: *mut u8,
    size: usize,
    _file: File,
}

unsafe impl Send for HubMapping {}
unsafe impl Sync for HubMapping {}

impl Drop for HubMapping {
    fn drop(&mut self) {
        unsafe {
            libc::munmap(self.base_addr as *mut libc::c_void, self.size);
        }
    }
}

/// Host-side hub session.
pub struct HubHost {
    mapping: Arc<HubMapping>,
    offsets: HubOffsets,
    config: HubConfig,
    allocator: HubAllocator,
    path: std::path::PathBuf,
}

unsafe impl Send for HubHost {}
unsafe impl Sync for HubHost {}

/// Information about an added peer.
pub struct PeerInfo {
    pub peer_id: u16,
    pub doorbell: Doorbell,
    pub peer_doorbell_fd: i32,
}

impl HubHost {
    pub fn create(path: impl AsRef<Path>, config: HubConfig) -> Result<Self, HubSessionError> {
        let path = path.as_ref();

        let offsets = HubOffsets::calculate(config.max_peers, config.ring_capacity)
            .map_err(|e| HubSessionError::Layout(e.to_string()))?;
        let total_size = calculate_initial_hub_size(config.max_peers, config.ring_capacity)
            .map_err(|e| HubSessionError::Layout(e.to_string()))?;

        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)
            .map_err(HubSessionError::Io)?;

        file.set_len(total_size as u64)
            .map_err(HubSessionError::Io)?;

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

        let header = unsafe { &mut *(base_addr as *mut HubHeader) };
        header.init(config.max_peers, config.ring_capacity);
        header
            .current_size
            .store(total_size as u64, Ordering::Release);
        header.peer_table_offset = offsets.peer_table as u64;
        header.ring_region_offset = offsets.ring_region as u64;
        header.size_class_offset = offsets.size_class_headers as u64;
        header.extent_region_offset = offsets.extent_region as u64;

        for i in 0..config.max_peers {
            let peer_entry = unsafe {
                &mut *(base_addr
                    .add(offsets.peer_table + i as usize * std::mem::size_of::<PeerEntry>())
                    as *mut PeerEntry)
            };
            peer_entry.init(0, 0);
        }

        let ring_size = std::mem::size_of::<DescRingHeader>()
            + config.ring_capacity as usize * std::mem::size_of::<MsgDescHot>();

        for i in 0..config.max_peers {
            let send_ring_offset = offsets.ring_region + i as usize * 2 * ring_size;
            let recv_ring_offset = send_ring_offset + ring_size;

            let send_ring_header =
                unsafe { &mut *(base_addr.add(send_ring_offset) as *mut DescRingHeader) };
            send_ring_header.init(config.ring_capacity);

            let recv_ring_header =
                unsafe { &mut *(base_addr.add(recv_ring_offset) as *mut DescRingHeader) };
            recv_ring_header.init(config.ring_capacity);

            // Store offsets in peer entries (read-only afterwards).
            let peer_entry = unsafe { &mut *self_peer_entry_ptr(base_addr, &offsets, i) };
            peer_entry.send_ring_offset = send_ring_offset as u64;
            peer_entry.recv_ring_offset = recv_ring_offset as u64;
        }

        let mut size_class_ptrs: [*mut SizeClassHeader; NUM_SIZE_CLASSES] =
            [std::ptr::null_mut(); NUM_SIZE_CLASSES];

        for class in 0..NUM_SIZE_CLASSES {
            let ptr = unsafe {
                base_addr.add(
                    offsets.size_class_headers + class * std::mem::size_of::<SizeClassHeader>(),
                ) as *mut SizeClassHeader
            };
            size_class_ptrs[class] = ptr;

            let (slot_size, slot_count) = HUB_SIZE_CLASSES[class];
            let slots_per_extent = slot_count.next_power_of_two();
            unsafe {
                (*ptr).init(slot_size, slots_per_extent);
            }
        }

        let allocator = unsafe { HubAllocator::from_raw(size_class_ptrs, base_addr) };

        // Initialize extents inline in the initial mapping.
        let mut extent_offset = offsets.extent_region;
        for (class, (slot_size, slot_count)) in HUB_SIZE_CLASSES.iter().enumerate() {
            let extent_size = calculate_extent_size(*slot_size, *slot_count)
                .map_err(|e| HubSessionError::Layout(e.to_string()))?;

            // Write extent header.
            let extent_header =
                unsafe { &mut *(base_addr.add(extent_offset) as *mut ExtentHeader) };
            extent_header.class = class as u16;
            extent_header.extent_index = 0;
            extent_header.base_global_index = 0;
            extent_header.slot_count = *slot_count;
            extent_header.slot_size = *slot_size;
            extent_header.meta_offset = std::mem::size_of::<ExtentHeader>() as u32;
            extent_header.data_offset = (std::mem::size_of::<ExtentHeader>()
                + (*slot_count as usize) * std::mem::size_of::<HubSlotMeta>())
                as u32;
            extent_header._pad = [0; 32];

            // Initialize slot metas.
            for i in 0..*slot_count {
                let meta_ptr = unsafe {
                    base_addr.add(extent_offset + extent_header.meta_offset as usize)
                        as *mut HubSlotMeta
                };
                let meta = unsafe { &mut *meta_ptr.add(i as usize) };
                meta.generation.store(0, Ordering::Relaxed);
                meta.state.store(0, Ordering::Relaxed);
                meta.next_free.store(0, Ordering::Relaxed);
                meta.owner_peer.store(0, Ordering::Relaxed);
            }

            // Store extent offset in size class header.
            unsafe {
                (*size_class_ptrs[class]).extent_offsets[0]
                    .store(extent_offset as u64, Ordering::Release);
            }

            unsafe {
                init_extent_free_list(&allocator, class, 0, extent_offset as u64);
            }

            extent_offset += extent_size;
        }

        Ok(Self {
            mapping: Arc::new(HubMapping {
                base_addr,
                size: total_size,
                _file: file,
            }),
            offsets,
            config,
            allocator,
            path: path.to_path_buf(),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn add_peer(&self) -> Result<PeerInfo, HubSessionError> {
        let header = self.header();
        let peer_id = header.peer_id_counter.fetch_add(1, Ordering::AcqRel);
        if peer_id >= self.config.max_peers {
            return Err(HubSessionError::TooManyPeers);
        }

        let peer_entry = unsafe { &mut *self.peer_entry_ptr(peer_id) };
        peer_entry.peer_id = peer_id;
        peer_entry.peer_type = 1;
        peer_entry
            .flags
            .store(PEER_FLAG_RESERVED, Ordering::Release);
        peer_entry.epoch.store(0, Ordering::Release);
        peer_entry.last_seen.store(0, Ordering::Release);

        let (doorbell, peer_fd) = Doorbell::create_pair().map_err(HubSessionError::Io)?;
        header.active_peers.fetch_add(1, Ordering::AcqRel);

        Ok(PeerInfo {
            peer_id,
            doorbell,
            peer_doorbell_fd: peer_fd,
        })
    }

    pub fn activate_peer(&self, peer_id: u16) -> Result<(), HubSessionError> {
        if peer_id >= self.config.max_peers {
            return Err(HubSessionError::InvalidPeerId);
        }
        let peer_entry = self.peer_entry(peer_id);
        peer_entry
            .flags
            .fetch_or(PEER_FLAG_ACTIVE, Ordering::Release);
        peer_entry
            .flags
            .fetch_and(!PEER_FLAG_RESERVED, Ordering::Release);
        Ok(())
    }

    pub fn remove_peer(&self, peer_id: u16) -> Result<(), HubSessionError> {
        if peer_id >= self.config.max_peers {
            return Err(HubSessionError::InvalidPeerId);
        }

        let peer_entry = self.peer_entry(peer_id);
        peer_entry.mark_dead();

        self.allocator.reclaim_peer_slots(peer_id as u32);
        self.header().active_peers.fetch_sub(1, Ordering::AcqRel);
        Ok(())
    }

    fn header(&self) -> &HubHeader {
        unsafe { &*(self.mapping.base_addr as *const HubHeader) }
    }

    fn peer_entry_ptr(&self, peer_id: u16) -> *mut PeerEntry {
        unsafe {
            self.mapping
                .base_addr
                .add(self.offsets.peer_table + peer_id as usize * std::mem::size_of::<PeerEntry>())
                as *mut PeerEntry
        }
    }

    fn peer_entry(&self, peer_id: u16) -> &PeerEntry {
        unsafe { &*self.peer_entry_ptr(peer_id) }
    }

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

    pub fn allocator(&self) -> &HubAllocator {
        &self.allocator
    }

    pub fn peer_send_data_futex(&self, peer_id: u16) -> &std::sync::atomic::AtomicU32 {
        &self.peer_entry(peer_id).send_data_futex
    }

    pub fn peer_recv_data_futex(&self, peer_id: u16) -> &std::sync::atomic::AtomicU32 {
        &self.peer_entry(peer_id).recv_data_futex
    }

    pub fn is_peer_active(&self, peer_id: u16) -> bool {
        self.peer_entry(peer_id).flags.load(Ordering::Acquire) & PEER_FLAG_ACTIVE != 0
    }
}

fn self_peer_entry_ptr(base_addr: *mut u8, offsets: &HubOffsets, peer_id: u16) -> *mut PeerEntry {
    unsafe {
        base_addr.add(offsets.peer_table + peer_id as usize * std::mem::size_of::<PeerEntry>())
            as *mut PeerEntry
    }
}

/// Plugin-side hub session.
pub struct HubPeer {
    mapping: Arc<HubMapping>,
    peer_id: u16,
    offsets: HubOffsets,
    allocator: HubAllocator,
}

unsafe impl Send for HubPeer {}
unsafe impl Sync for HubPeer {}

impl HubPeer {
    pub fn open(path: impl AsRef<Path>, peer_id: u16) -> Result<Self, HubSessionError> {
        let path = path.as_ref();

        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .map_err(HubSessionError::Io)?;

        let file_size = file.metadata().map_err(HubSessionError::Io)?.len() as usize;

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

        let header = unsafe { &*(base_addr as *const HubHeader) };
        header
            .validate()
            .map_err(|e| HubSessionError::Layout(e.to_string()))?;

        let offsets = HubOffsets::calculate(header.max_peers, header.ring_capacity)
            .map_err(|e| HubSessionError::Layout(e.to_string()))?;

        if peer_id >= header.max_peers {
            return Err(HubSessionError::InvalidPeerId);
        }

        let mut size_class_ptrs: [*mut SizeClassHeader; NUM_SIZE_CLASSES] =
            [std::ptr::null_mut(); NUM_SIZE_CLASSES];
        for (class, slot) in size_class_ptrs
            .iter_mut()
            .enumerate()
            .take(NUM_SIZE_CLASSES)
        {
            *slot = unsafe {
                base_addr.add(
                    offsets.size_class_headers + class * std::mem::size_of::<SizeClassHeader>(),
                ) as *mut SizeClassHeader
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

    pub fn register(&self) {
        let peer_entry = self.peer_entry();
        peer_entry
            .flags
            .fetch_or(PEER_FLAG_ACTIVE, Ordering::Release);
        peer_entry
            .flags
            .fetch_and(!PEER_FLAG_RESERVED, Ordering::Release);
    }

    pub fn peer_id(&self) -> u16 {
        self.peer_id
    }

    fn peer_entry(&self) -> &PeerEntry {
        unsafe {
            &*(self.mapping.base_addr.add(
                self.offsets.peer_table + self.peer_id as usize * std::mem::size_of::<PeerEntry>(),
            ) as *const PeerEntry)
        }
    }

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

    pub fn allocator(&self) -> &HubAllocator {
        &self.allocator
    }

    pub fn send_data_futex(&self) -> &std::sync::atomic::AtomicU32 {
        &self.peer_entry().send_data_futex
    }

    pub fn recv_data_futex(&self) -> &std::sync::atomic::AtomicU32 {
        &self.peer_entry().recv_data_futex
    }

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
    Io(io::Error),
    Layout(String),
    TooManyPeers,
    InvalidPeerId,
}

impl std::fmt::Display for HubSessionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HubSessionError::Io(e) => write!(f, "I/O error: {e}"),
            HubSessionError::Layout(e) => write!(f, "layout error: {e}"),
            HubSessionError::TooManyPeers => write!(f, "too many peers"),
            HubSessionError::InvalidPeerId => write!(f, "invalid peer id"),
        }
    }
}

impl std::error::Error for HubSessionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            HubSessionError::Io(e) => Some(e),
            _ => None,
        }
    }
}
